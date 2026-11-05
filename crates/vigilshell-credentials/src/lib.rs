//! 加密密码库、系统主密钥存储及按需原生输入。秘密不通过桌面 JSON-RPC。
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    process::Command,
    sync::{Semaphore, watch},
};

use vigilshell_application_contracts::ports::{
    DisplayContext, FutureResult, NativePrompt, SecretMaterial,
};
use vigilshell_domain::workspace::*;
use zeroize::Zeroizing;
mod diagnostics;
pub mod prompt_response;
pub const MAX_SECRET: usize = 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptRequest {
    pub kind: String,
    pub locale: String,
    pub theme: String,
}
pub mod messages {
    include!(concat!(env!("OUT_DIR"), "/native_messages.rs"));
}

pub struct NativePrompter {
    executable: PathBuf,
    context: watch::Sender<DisplayContext>,
    gate: Semaphore,
}
impl NativePrompter {
    pub fn new(executable: PathBuf) -> Self {
        let (context, _) = watch::channel(DisplayContext {
            locale: "en".into(),
            theme: "light".into(),
        });
        Self {
            executable,
            context,
            gate: Semaphore::new(1),
        }
    }
    async fn prompt(
        &self,
        kind: &str,
        cancelled: &AtomicBool,
    ) -> Result<(Zeroizing<Vec<u8>>, String, Option<bool>), WorkspaceError> {
        let mut context = self.context.subscribe();
        let mut child = Command::new(&self.executable)
            .kill_on_drop(true)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|_| WorkspaceError::NativeUnavailable)?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or(WorkspaceError::NativeUnavailable)?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or(WorkspaceError::NativeUnavailable)?;
        async fn send(
            stdin: &mut tokio::process::ChildStdin,
            kind: &str,
            c: &DisplayContext,
        ) -> Result<(), WorkspaceError> {
            let mut bytes = serde_json::to_vec(&PromptRequest {
                kind: kind.into(),
                locale: c.locale.clone(),
                theme: c.theme.clone(),
            })
            .map_err(|_| WorkspaceError::NativeUnavailable)?;
            bytes.push(b'\n');
            stdin
                .write_all(&bytes)
                .await
                .map_err(|_| WorkspaceError::NativeUnavailable)
        }
        let initial = context.borrow().clone();
        send(&mut stdin, kind, &initial).await?;
        let read = prompt_response::read(&mut stdout);
        tokio::pin!(read);
        let result = loop {
            tokio::select! {
                result=&mut read=>break result,
                _=tokio::time::sleep(Duration::from_millis(40))=>if cancelled.load(Ordering::Acquire){break Err(WorkspaceError::Cancelled)},
                change=context.changed()=>{if change.is_err(){break Err(WorkspaceError::NativeUnavailable);}let value=context.borrow().clone();if send(&mut stdin,kind,&value).await.is_err(){break Err(WorkspaceError::NativeUnavailable);}}
            }
        };
        drop(stdin);
        if result.is_err() {
            let _ = child.start_kill();
        }
        if tokio::time::timeout(Duration::from_secs(1), child.wait())
            .await
            .is_err()
        {
            let _ = child.kill().await;
        }
        result
    }
}
impl NativePrompt for NativePrompter {
    fn update_context(&self, context: DisplayContext) {
        self.context.send_replace(context);
    }
    fn capture(
        &self,
        kind: AuthKind,
        cancelled: Arc<AtomicBool>,
    ) -> FutureResult<'_, SecretMaterial> {
        Box::pin(async move {
            let _permit = self.gate.try_acquire().map_err(|_| WorkspaceError::Busy)?;
            let (bytes, label, input_consistent) = self
                .prompt(
                    if kind == AuthKind::Password {
                        "password"
                    } else {
                        "key"
                    },
                    &cancelled,
                )
                .await?;
            if kind == AuthKind::Password {
                return prepare_password(bytes, input_consistent);
            }
            match prepare_key(&bytes, None, &label) {
                Ok(secret) => Ok(secret),
                Err(ImportError::NeedPassphrase) => {
                    let (passphrase, _, _) = self.prompt("passphrase", &cancelled).await?;
                    let passphrase = std::str::from_utf8(&passphrase)
                        .map_err(|_| WorkspaceError::WrongPassphrase)?;
                    prepare_key(&bytes, Some(passphrase), &label).map_err(|e| match e {
                        ImportError::Failed(e) => e,
                        ImportError::NeedPassphrase => WorkspaceError::WrongPassphrase,
                    })
                }
                Err(ImportError::Failed(e)) => Err(e),
            }
        })
    }
}

// 原生密码框是单行输入；复制附带的末尾 CR/LF 不属于密码正文。
// 在生成 SecretMaterial 前统一处理，让草稿测试、保存和后续连接使用相同字节。
fn prepare_password(
    mut bytes: Zeroizing<Vec<u8>>,
    input_consistent: Option<bool>,
) -> Result<SecretMaterial, WorkspaceError> {
    while matches!(bytes.last(), Some(b'\r' | b'\n')) {
        bytes.pop();
    }
    if bytes.is_empty() || bytes.len() > 64 * 1024 {
        return Err(WorkspaceError::InvalidInput);
    }
    let diagnostic = diagnostics::capture(&bytes);
    if let Some(trace) = &diagnostic {
        trace.record("native_editor_value", input_consistent);
    }
    Ok(SecretMaterial {
        diagnostic,
        kind: AuthKind::Password,
        bytes,
        label: String::new(),
    })
}

#[derive(Debug)]
enum ImportError {
    NeedPassphrase,
    Failed(WorkspaceError),
}
fn prepare_key(
    bytes: &[u8],
    passphrase: Option<&str>,
    filename: &str,
) -> Result<SecretMaterial, ImportError> {
    let invalid = || ImportError::Failed(WorkspaceError::InvalidKey);
    if bytes.len() > MAX_SECRET {
        return Err(invalid());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    if !text.trim_start().starts_with("-----BEGIN ") {
        return Err(invalid());
    }
    // Legacy PEM EC encryption is not supported by the upstream parser (it decodes
    // PKCS#5 plaintext as RSA). Reject explicitly instead of blaming the passphrase.
    if text.contains("Proc-Type: 4,ENCRYPTED")
        && (text.contains("-----BEGIN EC PRIVATE KEY-----")
            || !text.contains("DEK-Info: AES-128-CBC,"))
    {
        return Err(ImportError::Failed(WorkspaceError::UnsupportedKeyFormat));
    }
    if passphrase.is_none() && text.contains("-----BEGIN ENCRYPTED PRIVATE KEY-----") {
        return Err(ImportError::NeedPassphrase);
    }
    let key = russh::keys::decode_secret_key(text, passphrase).map_err(|e| {
        if passphrase.is_some() {
            ImportError::Failed(WorkspaceError::WrongPassphrase)
        } else if matches!(e, russh::keys::Error::KeyIsEncrypted) {
            ImportError::NeedPassphrase
        } else {
            invalid()
        }
    })?;
    if !matches!(
        key.algorithm(),
        russh::keys::Algorithm::Ed25519
            | russh::keys::Algorithm::Rsa { .. }
            | russh::keys::Algorithm::Ecdsa { .. }
    ) {
        return Err(ImportError::Failed(WorkspaceError::UnsupportedKeyFormat));
    }
    let normalized = key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .map_err(|_| invalid())?;
    Ok(SecretMaterial {
        diagnostic: None,
        kind: AuthKind::PrivateKey,
        bytes: Zeroizing::new(normalized.as_bytes().to_vec()),
        label: format!(
            "{} · {}",
            filename,
            key.public_key().fingerprint(russh::keys::HashAlg::Sha256)
        ),
    })
}

mod configuration;
mod platform;
mod vault;
pub use platform::PlatformVaultKeyStore;
pub use vault::EncryptedSecretStore;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_openssh_and_pem_keys_and_matches_openssh_fingerprints() {
        for (kind, pem) in [
            ("ed25519", false),
            ("rsa", false),
            ("ecdsa", false),
            ("rsa", true),
            ("ecdsa", true),
        ] {
            for encrypted in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("fixture");
                let password = if encrypted {
                    "synthetic-passphrase"
                } else {
                    ""
                };
                let mut command = std::process::Command::new("ssh-keygen");
                command
                    .args(["-q", "-t", kind, "-N", password, "-f"])
                    .arg(&path);
                if pem {
                    command.args(["-m", "PEM"]);
                }
                assert!(command.status().unwrap().success());
                let bytes = Zeroizing::new(std::fs::read(&path).unwrap());
                if kind == "ecdsa" && pem && encrypted {
                    assert!(matches!(
                        prepare_key(&bytes, None, "fixture"),
                        Err(ImportError::Failed(WorkspaceError::UnsupportedKeyFormat))
                    ));
                    continue;
                }
                if encrypted {
                    assert!(
                        matches!(
                            prepare_key(&bytes, None, "fixture"),
                            Err(ImportError::NeedPassphrase)
                        ),
                        "{kind} pem={pem}"
                    );
                    assert!(matches!(
                        prepare_key(&bytes, Some("wrong"), "fixture"),
                        Err(ImportError::Failed(WorkspaceError::WrongPassphrase))
                    ));
                }
                let secret = prepare_key(
                    &bytes,
                    if encrypted { Some(password) } else { None },
                    "fixture",
                )
                .unwrap_or_else(|e| panic!("{kind} pem={pem} encrypted={encrypted}: {e:?}"));
                let expected = std::process::Command::new("ssh-keygen")
                    .args(["-lf"])
                    .arg(path.with_extension("pub"))
                    .args(["-E", "sha256"])
                    .output()
                    .unwrap();
                assert!(expected.status.success());
                let text = String::from_utf8(expected.stdout).unwrap();
                let fingerprint = text.split_whitespace().nth(1).unwrap();
                assert!(secret.label.ends_with(fingerprint));
                assert!(
                    secret
                        .bytes
                        .starts_with(b"-----BEGIN OPENSSH PRIVATE KEY-----")
                );
            }
        }
    }
    #[test]
    fn rejects_invalid_or_oversized_material() {
        for bytes in [
            b"not a key".to_vec(),
            vec![0; MAX_SECRET + 1],
            b"-----BEGIN RSA PRIVATE KEY-----\ninvalid\n-----END RSA PRIVATE KEY-----".to_vec(),
        ] {
            assert!(matches!(
                prepare_key(&bytes, None, "fixture"),
                Err(ImportError::Failed(WorkspaceError::InvalidKey))
            ));
        }
    }
}

mod key_store;
pub use key_store::{VaultKeyMaterial, VaultKeyStore};

#[cfg(test)]
mod password_tests {
    use super::*;

    #[test]
    fn password_removes_only_trailing_line_endings() {
        for (input, expected) in [
            ("fixture\n", "fixture"),
            ("fixture\r\n", "fixture"),
            ("fixture\r", "fixture"),
            ("fixture\n\r\n", "fixture"),
            ("  密码 fixture \r\n", "  密码 fixture "),
            ("\nfixture\ninside\n", "\nfixture\ninside"),
            ("fixture\n ", "fixture\n "),
            (" fixture ", " fixture "),
            ("fixture", "fixture"),
        ] {
            let secret = prepare_password(input.as_bytes().to_vec().into(), Some(true)).unwrap();
            assert_eq!(secret.bytes.as_slice(), expected.as_bytes());
            assert_eq!(secret.kind, AuthKind::Password);
        }
    }

    #[test]
    fn password_checks_empty_and_limit_after_normalization() {
        for input in ["", "\r", "\n", "\r\n\n"] {
            assert!(matches!(
                prepare_password(input.as_bytes().to_vec().into(), None),
                Err(WorkspaceError::InvalidInput)
            ));
        }
        let mut maximum = vec![b'x'; 64 * 1024];
        maximum.extend_from_slice(b"\r\n");
        assert_eq!(
            prepare_password(maximum.into(), None).unwrap().bytes.len(),
            64 * 1024
        );
        assert!(matches!(
            prepare_password(vec![b'x'; 64 * 1024 + 1].into(), None),
            Err(WorkspaceError::InvalidInput)
        ));
    }

    #[tokio::test]
    async fn password_native_payload_preserves_spaces_and_removes_clipboard_crlf() {
        let mut frame = Zeroizing::new(Vec::new());
        prompt_response::write(&mut *frame, 1, Some(true), "", b" fixture-password \r\n").unwrap();
        let (bytes, _, input_consistent) =
            prompt_response::read(&mut frame.as_slice()).await.unwrap();
        let captured = prepare_password(bytes, input_consistent).unwrap();
        assert_eq!(captured.bytes.as_slice(), b" fixture-password ");
    }
}
