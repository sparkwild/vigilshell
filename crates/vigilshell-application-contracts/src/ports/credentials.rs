//! 应用持有秘密的范围和凭据能力；具体平台调用由适配器提供。
use super::FutureResult;
use std::fmt;
use vigilshell_domain::workspace::*;
use zeroize::Zeroizing;
/// 仅限运行期的脱敏一致性诊断；实现不得输出输入字节或校验标记。
pub trait SecretDiagnostic: Send + Sync {
    fn id(&self) -> &str;
    fn check(&self, checkpoint: &'static str, bytes: &[u8]) -> bool;
    fn record(&self, checkpoint: &'static str, matches: Option<bool>);
}

pub struct SecretMaterial {
    pub diagnostic: Option<std::sync::Arc<dyn SecretDiagnostic>>,
    pub kind: AuthKind,
    pub bytes: Zeroizing<Vec<u8>>,
    pub label: String,
}
impl SecretMaterial {
    pub fn duplicate(&self) -> Self {
        Self {
            diagnostic: self.diagnostic.clone(),
            kind: self.kind,
            bytes: Zeroizing::new(self.bytes.to_vec()),
            label: self.label.clone(),
        }
    }
}
impl fmt::Debug for SecretMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretMaterial")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}
pub trait SecretStore: Send + Sync {
    fn read<'a>(
        &'a self,
        namespace: &'a str,
        reference: &'a str,
        context: CredentialContext,
    ) -> FutureResult<'a, SecretMaterial>;
    fn write<'a>(
        &'a self,
        namespace: &'a str,
        reference: &'a str,
        secret: &'a SecretMaterial,
        context: CredentialContext,
    ) -> FutureResult<'a, ()>;
    fn remove<'a>(&'a self, namespace: &'a str, reference: &'a str) -> FutureResult<'a, ()>;
    /// Terminal state: late OS results must not restore the cached key.
    fn close(&self) {}
}

#[derive(Clone, Debug)]
pub struct DisplayContext {
    pub locale: String,
    pub theme: String,
}
pub trait NativePrompt: Send + Sync {
    fn capture(
        &self,
        kind: AuthKind,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> FutureResult<'_, SecretMaterial>;
    fn update_context(&self, context: DisplayContext);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn debug_redacts_secret_material() {
        let secret = SecretMaterial {
            diagnostic: None,
            kind: AuthKind::Password,
            bytes: Zeroizing::new(b"test-secret".to_vec()),
            label: "private-label".into(),
        };
        assert!(!format!("{secret:?}").contains("test-secret"));
        assert!(!format!("{secret:?}").contains("private-label"));
    }
}
