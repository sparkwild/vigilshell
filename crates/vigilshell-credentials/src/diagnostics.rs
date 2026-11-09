//! 显式诊断期间仅保留随机加盐的内存校验标记；不输出或持久化密码、长度、盐或摘要。
use sha2::{Digest, Sha256};
use std::sync::Arc;
use vigilshell_application_contracts::ports::SecretDiagnostic;
use zeroize::Zeroizing;

struct InputTrace {
    id: String,
    salt: Zeroizing<[u8; 32]>,
    expected: Zeroizing<[u8; 32]>,
}
fn digest(salt: &[u8], bytes: &[u8]) -> Zeroizing<[u8; 32]> {
    let mut hash = Sha256::new();
    hash.update(salt);
    hash.update(bytes);
    Zeroizing::new(hash.finalize().into())
}
pub(crate) fn enabled() -> bool {
    std::env::var_os("VIGILSHELL_AUTH_DIAGNOSTICS").is_some_and(|v| v == "1")
}
pub(crate) fn capture(bytes: &[u8]) -> Option<Arc<dyn SecretDiagnostic>> {
    if !enabled() {
        return None;
    }
    let trace = InputTrace::new(bytes)?;
    tracing::info!(target: "vigilshell_auth_diagnostics", stage="credential_consistency",
        trace_id=trace.id(), checkpoint="password_input_ready", outcome="baseline");
    Some(Arc::new(trace))
}
impl InputTrace {
    fn new(bytes: &[u8]) -> Option<Self> {
        let mut salt = Zeroizing::new([0; 32]);
        getrandom::fill(&mut *salt).ok()?;
        Some(Self {
            id: vigilshell_domain::workspace::new_id(),
            expected: digest(salt.as_slice(), bytes),
            salt,
        })
    }
}
impl SecretDiagnostic for InputTrace {
    fn id(&self) -> &str {
        &self.id
    }
    fn check(&self, checkpoint: &'static str, bytes: &[u8]) -> bool {
        let matches = *digest(self.salt.as_slice(), bytes) == *self.expected;
        self.record(checkpoint, Some(matches));
        matches
    }
    fn record(&self, checkpoint: &'static str, matches: Option<bool>) {
        tracing::info!(target: "vigilshell_auth_diagnostics",
            stage = "credential_consistency", trace_id = %self.id, checkpoint,
            outcome = match matches { Some(true) => "match", Some(false) => "mismatch", None => "unavailable" });
    }
}

#[cfg(test)]
pub(crate) fn test_trace(bytes: &[u8]) -> Arc<dyn SecretDiagnostic> {
    Arc::new(InputTrace::new(bytes).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn markers_detect_changes_without_equating_separate_inputs() {
        let first = InputTrace::new(b" synthetic\npassword ").unwrap();
        let second = InputTrace::new(b" synthetic\npassword ").unwrap();
        assert!(first.check("fixture", b" synthetic\npassword "));
        assert!(!first.check("fixture", b"synthetic\npassword"));
        assert_ne!(first.id, second.id);
        assert!(*first.expected != *second.expected);
    }
    #[test]
    fn log_only_contains_stage_identity_and_comparison_result() {
        // Callsite interest is process-global; concurrent diagnostic tests can
        // register this event before the scoped subscriber is installed.
        if std::env::var_os("VIGILSHELL_DIAGNOSTICS_TEST_CHILD").is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "diagnostics::tests::log_only_contains_stage_identity_and_comparison_result",
                    "--nocapture",
                ])
                .env("VIGILSHELL_DIAGNOSTICS_TEST_CHILD", "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        #[derive(Clone)]
        struct Output(Arc<std::sync::Mutex<Vec<u8>>>);
        impl std::io::Write for Output {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let output = Output(Arc::default());
        let writer = output.clone();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let trace = InputTrace::new(b"never-log-this-synthetic-password").unwrap();
        tracing::subscriber::with_default(subscriber, || {
            trace.check("fixture", b"never-log-this-synthetic-password");
            trace.check("fixture", b"different-synthetic-password");
        });
        let text = String::from_utf8(output.0.lock().unwrap().clone()).unwrap();
        assert!(!text.contains("synthetic-password"));
        let rows: Vec<serde_json::Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows.len(), 2);
        for row in &rows {
            let fields = row["fields"].as_object().unwrap();
            assert_eq!(fields.len(), 4);
            for key in ["stage", "trace_id", "checkpoint", "outcome"] {
                assert!(fields.contains_key(key));
            }
        }
        assert_eq!(rows[0]["fields"]["outcome"], "match");
        assert_eq!(rows[1]["fields"]["outcome"], "mismatch");
    }
}
