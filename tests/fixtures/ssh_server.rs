//! Loopback-only SSH/PTY/forwarding fixture. It echoes bytes and never executes a shell command.
use russh::{
    Channel, ChannelId, Pty,
    server::{self, Msg, Server as _, Session},
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
#[derive(Clone, Default)]
pub struct Shared {
    pub allow_exec: Arc<AtomicBool>,
    pub reject_pty: Arc<AtomicBool>,
    pub executed: Arc<Mutex<Vec<Vec<u8>>>>,
    pub logins: Arc<AtomicUsize>,
    pub deny_remote: Arc<AtomicBool>,
    pub exit_on_input: Arc<Mutex<Option<u32>>>,
    pub disconnect_on_input: Arc<Mutex<Option<String>>>,
}
pub struct Handler {
    pub shared: Shared,
    listeners: HashMap<u32, JoinHandle<()>>,
}
impl Drop for Handler {
    fn drop(&mut self) {
        for (_, task) in self.listeners.drain() {
            task.abort();
        }
    }
}
impl server::Server for Handler {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        Self {
            shared: self.shared.clone(),
            listeners: HashMap::new(),
        }
    }
}
impl server::Handler for Handler {
    type Error = russh::Error;
    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<server::Auth, Self::Error> {
        if user == "fixture" && password == "sidebar-fixture" {
            self.shared.logins.fetch_add(1, Ordering::SeqCst);
            Ok(server::Auth::Accept)
        } else {
            Ok(server::Auth::reject())
        }
    }
    async fn channel_open_session(
        &mut self,
        _: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }
    async fn pty_request(
        &mut self,
        c: ChannelId,
        _: &str,
        _: u32,
        _: u32,
        _: u32,
        _: u32,
        _: &[(Pty, u32)],
        s: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.shared.reject_pty.load(Ordering::SeqCst) {
            s.channel_failure(c)?;
            return Ok(());
        }
        s.channel_success(c)?;
        Ok(())
    }
    async fn exec_request(
        &mut self,
        c: ChannelId,
        data: &[u8],
        s: &mut Session,
    ) -> Result<(), Self::Error> {
        if !self.shared.allow_exec.load(Ordering::SeqCst) {
            s.channel_failure(c)?;
            return Ok(());
        }
        self.shared.executed.lock().unwrap().push(data.to_vec());
        // 保留确认之前的输出，用于验证适配器不会丢失早到的数据。
        s.data(c, "fixture-中文\n".as_bytes().to_vec())?;
        s.channel_success(c)?;
        s.extended_data(c, 1, b"fixture-stderr\n".to_vec())?;
        s.exit_status_request(c, 7)?;
        s.eof(c)?;
        s.close(c)?;
        Ok(())
    }
    async fn shell_request(&mut self, c: ChannelId, s: &mut Session) -> Result<(), Self::Error> {
        s.channel_success(c)?;
        Ok(())
    }
    async fn data(
        &mut self,
        c: ChannelId,
        data: &[u8],
        s: &mut Session,
    ) -> Result<(), Self::Error> {
        s.data(c, data.to_vec())?;
        if let Some(message) = self.shared.disconnect_on_input.lock().unwrap().as_deref() {
            s.disconnect(russh::Disconnect::ByApplication, message, "en")?;
        }
        if let Some(code) = *self.shared.exit_on_input.lock().unwrap() {
            s.eof(c)?;
            s.exit_status_request(c, code)?;
            s.close(c)?;
        }
        Ok(())
    }
    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host: &str,
        port: u32,
        _: &str,
        _: u32,
        reply: server::ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        if host != "127.0.0.1" {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return Ok(());
        }
        match TcpStream::connect((host, port as u16)).await {
            Ok(mut socket) => {
                reply.accept().await;
                tokio::spawn(async move {
                    let mut channel = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut channel).await;
                });
            }
            Err(_) => {
                reply.reject(russh::ChannelOpenFailure::ConnectFailed).await;
            }
        }
        Ok(())
    }
    async fn tcpip_forward(
        &mut self,
        address: &str,
        port: &mut u32,
        s: &mut Session,
    ) -> Result<bool, Self::Error> {
        if address != "127.0.0.1" || self.shared.deny_remote.load(Ordering::SeqCst) {
            return Ok(false);
        }
        let Ok(listener) = TcpListener::bind((address, *port as u16)).await else {
            return Ok(false);
        };
        *port = listener.local_addr()?.port() as u32;
        let handle = s.handle();
        let port = *port;
        self.listeners.insert(
            port,
            tokio::spawn(async move {
                while let Ok((mut socket, origin)) = listener.accept().await {
                    let h = handle.clone();
                    tokio::spawn(async move {
                        if let Ok(channel) = h
                            .channel_open_forwarded_tcpip(
                                "127.0.0.1",
                                port,
                                origin.ip().to_string(),
                                origin.port() as u32,
                            )
                            .await
                        {
                            let mut channel = channel.into_stream();
                            let _ = tokio::io::copy_bidirectional(&mut socket, &mut channel).await;
                        }
                    });
                }
            }),
        );
        Ok(true)
    }
    async fn cancel_tcpip_forward(
        &mut self,
        _: &str,
        port: u32,
        _: &mut Session,
    ) -> Result<bool, Self::Error> {
        if let Some(task) = self.listeners.remove(&port) {
            task.abort();
            let _ = task.await;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
pub struct Fixture {
    pub port: u16,
    pub shared: Shared,
    task: JoinHandle<()>,
    _keys: tempfile::TempDir,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Fixture {
    pub async fn start() -> Self {
        let keys = tempfile::tempdir().unwrap();
        let path = keys.path().join("host");
        assert!(
            std::process::Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let config = Arc::new(server::Config {
            keys: vec![russh::keys::load_secret_key(&path, None).unwrap()],
            auth_rejection_time: std::time::Duration::from_millis(1),
            ..Default::default()
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let shared = Shared::default();
        let mut handler = Handler {
            shared: shared.clone(),
            listeners: HashMap::new(),
        };
        let task = tokio::spawn(async move {
            let _ = handler.run_on_socket(config, &listener).await;
        });
        Self {
            port,
            shared,
            task,
            _keys: keys,
        }
    }
}
pub async fn unused_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    l.local_addr().unwrap().port()
}
