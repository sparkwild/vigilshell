use super::*;
use std::collections::HashMap;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Semaphore,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
struct Running {
    rule: TunnelRule,
    cancel: CancellationToken,
    listener: Option<tokio::task::JoinHandle<()>>,
}
struct RemoteRule {
    rule: TunnelRule,
    cancel: CancellationToken,
}
pub(super) struct RemoteForwards {
    rules: Mutex<HashMap<(String, u16), RemoteRule>>,
    tasks: TaskTracker,
    capacity: Arc<Semaphore>,
    states: Arc<Mutex<HashMap<String, TunnelStatus>>>,
}
impl RemoteForwards {
    pub(super) async fn accept(
        &self,
        channel: Channel<client::Msg>,
        address: &str,
        port: u32,
        reply: client::ChannelOpenHandle,
    ) {
        let route = u16::try_from(port).ok().and_then(|port| {
            self.rules
                .lock()
                .unwrap()
                .get(&(address.into(), port))
                .map(|r| (r.rule.clone(), r.cancel.clone()))
        });
        let permit = self.capacity.clone().try_acquire_owned();
        let (Some((rule, cancel)), Ok(permit)) = (route, permit) else {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return;
        };
        if cancel.is_cancelled() {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return;
        }
        reply.accept().await;
        let states = self.states.clone();
        self.tasks.spawn(async move{
            let _permit=permit;
            let work=async{let mut socket=tokio::time::timeout(Duration::from_secs(10),TcpStream::connect((rule.target_host.as_deref().unwrap(),rule.target_port.unwrap()))).await.map_err(|_|())?.map_err(|_|())?;let mut remote=channel.into_stream();tokio::io::copy_bidirectional(&mut socket,&mut remote).await.map_err(|_|())?;Ok::<_,()>(())};
            tokio::select!{biased;()=cancel.cancelled()=>{},r=work=>if r.is_err() && !cancel.is_cancelled(){last_error(&states,&rule.id,"tunnel_target_unreachable");}}
        });
    }
}
fn last_error(states: &Mutex<HashMap<String, TunnelStatus>>, id: &str, code: &str) {
    if let Some(s) = states.lock().unwrap().get_mut(id) {
        s.error = Some(code.into());
    }
}
struct Forwarding {
    handle: Arc<client::Handle<Handler>>,
    guard: Mutex<Option<SocketGuard>>,
    remote: Arc<RemoteForwards>,
    rules: Mutex<HashMap<String, Running>>,
    gate: tokio::sync::Mutex<()>,
    stopped: AtomicBool,
}
use std::sync::atomic::{AtomicBool, Ordering};
impl Drop for Forwarding {
    fn drop(&mut self) {
        for r in self.rules.lock().unwrap().values() {
            r.cancel.cancel();
        }
        self.guard.lock().unwrap().take();
        self.remote.tasks.close();
    }
}
pub(super) async fn connect(
    input: ConnectionInput,
    key: HostKey,
    secret: SecretMaterial,
    route: Option<ProxyRoute>,
) -> Result<Arc<dyn SshForwarding>, WorkspaceError> {
    let remote = Arc::new(RemoteForwards {
        rules: Mutex::new(HashMap::new()),
        tasks: TaskTracker::new(),
        capacity: Arc::new(Semaphore::new(32)),
        states: Arc::new(Mutex::new(HashMap::new())),
    });
    let (handle, guard) =
        authenticated(input, key, secret, route, Some(remote.clone()), None).await?;
    Ok(Arc::new(Forwarding {
        handle: Arc::new(handle),
        guard: Mutex::new(Some(guard)),
        remote,
        rules: Mutex::new(HashMap::new()),
        gate: tokio::sync::Mutex::new(()),
        stopped: AtomicBool::new(false),
    }))
}
impl SshForwarding for Forwarding {
    fn is_closed(&self) -> bool {
        self.stopped.load(Ordering::Acquire) || self.handle.is_closed()
    }
    fn start(&self, rule: TunnelRule) -> FutureResult<'_, TunnelStatus> {
        Box::pin(async move {
            rule.validate()?;
            let _gate = self.gate.lock().await;
            if self.is_closed() {
                return Err(WorkspaceError::TunnelFailed);
            }
            let previous = self.rules.lock().unwrap().remove(&rule.id);
            if let Some(previous) = previous {
                previous.cancel.cancel();
                if let Some(task) = previous.listener {
                    let _ = task.await;
                }
                if previous.rule.kind == "remote" {
                    self.remote
                        .rules
                        .lock()
                        .unwrap()
                        .remove(&(previous.rule.bind_host.clone(), previous.rule.listen_port));
                    let _ = deadline(
                        self.handle.cancel_tcpip_forward(
                            previous.rule.bind_host,
                            previous.rule.listen_port as u32,
                        ),
                        10,
                    )
                    .await;
                }
            }
            let cancel = CancellationToken::new();
            let mut listener_task = None;
            let mut status = TunnelStatus {
                rule_id: rule.id.clone(),
                phase: "running".into(),
                error: None,
            };
            if rule.kind == "remote" {
                self.remote.rules.lock().unwrap().insert(
                    (rule.bind_host.clone(), rule.listen_port),
                    RemoteRule {
                        rule: rule.clone(),
                        cancel: cancel.clone(),
                    },
                );
                if deadline(
                    self.handle
                        .tcpip_forward(rule.bind_host.clone(), rule.listen_port as u32),
                    10,
                )
                .await
                .is_err()
                {
                    self.remote
                        .rules
                        .lock()
                        .unwrap()
                        .remove(&(rule.bind_host.clone(), rule.listen_port));
                    status.phase = "failed".into();
                    status.error = Some("tunnel_remote_refused".into());
                }
            } else {
                match TcpListener::bind((rule.bind_host.as_str(), rule.listen_port)).await {
                    Err(_) => {
                        status.phase = "failed".into();
                        status.error = Some("tunnel_bind_failed".into());
                    }
                    Ok(listener) => {
                        let handle = self.handle.clone();
                        let capacity = self.remote.capacity.clone();
                        let tasks = self.remote.tasks.clone();
                        let states = self.remote.states.clone();
                        let rule = rule.clone();
                        let cancel = cancel.clone();
                        listener_task=Some(self.remote.tasks.spawn(async move{
                        loop{let accepted=tokio::select!{biased;()=cancel.cancelled()=>break,r=listener.accept()=>r};
                            let Ok((socket,origin))=accepted else{last_error(&states,&rule.id,"tunnel_accept_failed");break};
                            let Ok(permit)=capacity.clone().try_acquire_owned()else{drop(socket);last_error(&states,&rule.id,"tunnel_capacity");continue};
                            let handle=handle.clone();let cancel=cancel.clone();let rule=rule.clone();let states=states.clone();
                            tasks.spawn(async move{let _permit=permit;let work=forward_local(handle,socket,origin,rule.clone());tokio::select!{biased;()=cancel.cancelled()=>{},r=work=>if let Err(code)=r && !cancel.is_cancelled(){last_error(&states,&rule.id,code);}}});
                        }
                    }));
                    }
                }
            }
            if status.phase == "running" {
                self.rules.lock().unwrap().insert(
                    rule.id.clone(),
                    Running {
                        rule: rule.clone(),
                        cancel,
                        listener: listener_task,
                    },
                );
            } else {
                cancel.cancel();
            }
            self.remote
                .states
                .lock()
                .unwrap()
                .insert(rule.id, status.clone());
            Ok(status)
        })
    }
    fn statuses(&self) -> Vec<TunnelStatus> {
        let mut states = self
            .remote
            .states
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        if self.is_closed() {
            for s in &mut states {
                s.phase = "failed".into();
                s.error = Some("tunnel_disconnected".into());
            }
        }
        states.sort_by(|a, b| a.rule_id.cmp(&b.rule_id));
        states
    }
    fn close(&self) -> FutureResult<'_, ()> {
        Box::pin(async move {
            let _gate = self.gate.lock().await;
            self.stopped.store(true, Ordering::Release);
            for r in self.rules.lock().unwrap().values() {
                r.cancel.cancel();
            }
            self.remote.rules.lock().unwrap().clear();
            self.guard.lock().unwrap().take();
            let _ = self
                .handle
                .disconnect(Disconnect::ByApplication, "", "")
                .await;
            self.remote.tasks.close();
            self.remote.tasks.wait().await;
            Ok(())
        })
    }
}
async fn forward_local(
    handle: Arc<client::Handle<Handler>>,
    mut socket: TcpStream,
    origin: std::net::SocketAddr,
    rule: TunnelRule,
) -> Result<(), &'static str> {
    let (host, port) = if rule.kind == "socks5" {
        tokio::time::timeout(Duration::from_secs(10), socks_target(&mut socket))
            .await
            .map_err(|_| "tunnel_client_timeout")??
    } else {
        (rule.target_host.unwrap(), rule.target_port.unwrap())
    };
    let channel = deadline(
        handle.channel_open_direct_tcpip(
            host,
            port as u32,
            origin.ip().to_string(),
            origin.port() as u32,
        ),
        10,
    )
    .await;
    let mut channel = match channel {
        Ok(c) => c.into_stream(),
        Err(_) => {
            if rule.kind == "socks5" {
                let _ = socket.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await;
            }
            return Err("tunnel_target_unreachable");
        }
    };
    if rule.kind == "socks5" {
        socket
            .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
            .await
            .map_err(|_| "tunnel_client_closed")?;
    }
    tokio::io::copy_bidirectional(&mut socket, &mut channel)
        .await
        .map_err(|_| "tunnel_disconnected")?;
    Ok(())
}
async fn socks_target(socket: &mut TcpStream) -> Result<(String, u16), &'static str> {
    let mut prefix = [0u8; 2];
    socket
        .read_exact(&mut prefix)
        .await
        .map_err(|_| "tunnel_client_closed")?;
    if prefix[0] != 5 {
        return Err("tunnel_client_protocol");
    }
    let mut methods = vec![0; prefix[1] as usize];
    socket
        .read_exact(&mut methods)
        .await
        .map_err(|_| "tunnel_client_closed")?;
    if !methods.contains(&0) {
        let _ = socket.write_all(&[5, 255]).await;
        return Err("tunnel_client_protocol");
    }
    socket
        .write_all(&[5, 0])
        .await
        .map_err(|_| "tunnel_client_closed")?;
    let mut request = [0; 4];
    socket
        .read_exact(&mut request)
        .await
        .map_err(|_| "tunnel_client_closed")?;
    if request[0] != 5 || request[1] != 1 || request[2] != 0 {
        let _ = socket.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await;
        return Err("tunnel_client_protocol");
    }
    let host = match request[3] {
        1 => {
            let mut bytes = [0; 4];
            socket
                .read_exact(&mut bytes)
                .await
                .map_err(|_| "tunnel_client_closed")?;
            std::net::Ipv4Addr::from(bytes).to_string()
        }
        4 => {
            let mut bytes = [0; 16];
            socket
                .read_exact(&mut bytes)
                .await
                .map_err(|_| "tunnel_client_closed")?;
            std::net::Ipv6Addr::from(bytes).to_string()
        }
        3 => {
            let n = socket.read_u8().await.map_err(|_| "tunnel_client_closed")?;
            let mut bytes = vec![0; n as usize];
            socket
                .read_exact(&mut bytes)
                .await
                .map_err(|_| "tunnel_client_closed")?;
            String::from_utf8(bytes).map_err(|_| "tunnel_client_protocol")?
        }
        _ => return Err("tunnel_client_protocol"),
    };
    let port = socket
        .read_u16()
        .await
        .map_err(|_| "tunnel_client_closed")?;
    if !valid_host(&host) || port == 0 {
        return Err("tunnel_client_protocol");
    }
    Ok((host, port))
}
