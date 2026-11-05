use super::{FutureResult, SecretMaterial};
use vigilshell_domain::workspace::*;
#[derive(Clone, Debug)]
pub enum TransportEvent {
    Data(Vec<u8>),
    Exit(u32),
    ExitSignal(String),
    Eof,
    Disconnected {
        error: Option<WorkspaceError>,
        message: Option<String>,
    },
}
pub trait SshSession: Send {
    fn open_terminal(&mut self, columns: u16, rows: u16) -> FutureResult<'_, ()>;
    fn read(&mut self) -> FutureResult<'_, TransportEvent>;
    fn write<'a>(&'a mut self, data: &'a [u8]) -> FutureResult<'a, ()>;
    fn resize(&mut self, columns: u16, rows: u16) -> FutureResult<'_, ()>;
    fn close(&mut self) -> FutureResult<'_, ()>;
}
pub struct ProxyRoute {
    pub proxy: ProxyRecord,
    pub secret: Option<SecretMaterial>,
}
impl ProxyRoute {
    pub fn duplicate(&self) -> Self {
        Self {
            proxy: self.proxy.clone(),
            secret: self.secret.as_ref().map(SecretMaterial::duplicate),
        }
    }
}
pub trait SshForwarding: Send + Sync {
    fn is_closed(&self) -> bool {
        false
    }
    fn start(&self, rule: TunnelRule) -> FutureResult<'_, TunnelStatus>;
    fn statuses(&self) -> Vec<TunnelStatus>;
    fn close(&self) -> FutureResult<'_, ()>;
}
pub trait SshTransport: Send + Sync {
    fn agent(
        &self,
        _input: ConnectionInput,
        _key: HostKey,
        _secret: super::SecretMaterial,
        _route: Option<ProxyRoute>,
    ) -> FutureResult<'_, std::sync::Arc<dyn super::AgentConnection>> {
        Box::pin(async { Err(WorkspaceError::ToolsUnavailable) })
    }
    fn tools(
        &self,
        _input: ConnectionInput,
        _key: HostKey,
        _secret: SecretMaterial,
        _route: Option<ProxyRoute>,
    ) -> FutureResult<'_, std::sync::Arc<dyn crate::ports::ServerTools>> {
        Box::pin(async { Err(WorkspaceError::ToolsUnavailable) })
    }

    fn probe_via(
        &self,
        input: ConnectionInput,
        route: Option<ProxyRoute>,
    ) -> FutureResult<'_, HostKey> {
        if route.is_some() {
            Box::pin(async { Err(WorkspaceError::ProxyFailed) })
        } else {
            self.probe(input)
        }
    }
    fn authenticate_via(
        &self,
        input: ConnectionInput,
        key: HostKey,
        secret: SecretMaterial,
        route: Option<ProxyRoute>,
    ) -> FutureResult<'_, Box<dyn SshSession>> {
        if route.is_some() {
            Box::pin(async { Err(WorkspaceError::ProxyFailed) })
        } else {
            self.authenticate(input, key, secret)
        }
    }
    fn forwarding(
        &self,
        _input: ConnectionInput,
        _key: HostKey,
        _secret: SecretMaterial,
        _route: Option<ProxyRoute>,
    ) -> FutureResult<'_, std::sync::Arc<dyn SshForwarding>> {
        Box::pin(async { Err(WorkspaceError::TunnelFailed) })
    }

    fn probe(&self, input: ConnectionInput) -> FutureResult<'_, HostKey>;
    fn authenticate(
        &self,
        input: ConnectionInput,
        key: HostKey,
        secret: SecretMaterial,
    ) -> FutureResult<'_, Box<dyn SshSession>>;
}
