//! 合成身份、随机回环端口；只执行测试提供的命令，不使用用户 SSH 配置或凭据。
use russh::{
    Channel, ChannelId,
    server::{self, Msg, Server as _, Session},
};
use std::{
    collections::HashMap,
    process::Stdio,
    sync::{Arc, Mutex},
};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;
#[derive(Clone, Default)]
pub struct State {
    pub commands: Arc<Mutex<Vec<String>>>,
    pub omit_exit: Arc<std::sync::atomic::AtomicBool>,
}
struct Handler {
    state: State,
    stops: HashMap<ChannelId, CancellationToken>,
}
impl Drop for Handler {
    fn drop(&mut self) {
        for stop in self.stops.values() {
            stop.cancel();
        }
    }
}
impl server::Server for Handler {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        Self {
            state: self.state.clone(),
            stops: HashMap::new(),
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
        Ok(
            if user == "command-fixture" && password == "synthetic-only" {
                server::Auth::Accept
            } else {
                server::Auth::reject()
            },
        )
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
    async fn exec_request(
        &mut self,
        id: ChannelId,
        bytes: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let command = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(command.starts_with("exec /usr/bin/env -i PATH=/usr/bin:/bin LC_ALL=C "));
        self.state.commands.lock().unwrap().push(command.clone());
        let stop = CancellationToken::new();
        self.stops.insert(id, stop.clone());
        session.channel_success(id)?;
        let handle = session.handle();
        let omit = self.state.omit_exit.clone();
        tokio::spawn(async move {
            let mut child = tokio::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(command)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let mut out = child.stdout.take().unwrap();
            let mut err = child.stderr.take().unwrap();
            let out_handle = handle.clone();
            let err_handle = handle.clone();
            let a = tokio::spawn(async move {
                let mut b = [0; 4096];
                while let Ok(n) = out.read(&mut b).await {
                    if n == 0 {
                        break;
                    }
                    if out_handle.data(id, b[..n].to_vec()).await.is_err() {
                        break;
                    }
                }
            });
            let b = tokio::spawn(async move {
                let mut b = [0; 4096];
                while let Ok(n) = err.read(&mut b).await {
                    if n == 0 {
                        break;
                    }
                    if err_handle
                        .extended_data(id, 1, b[..n].to_vec())
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });
            let status = tokio::select! {result=child.wait()=>result.unwrap(),_=stop.cancelled()=>{let _=child.start_kill();child.wait().await.unwrap()}};
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), async {
                let _ = a.await;
                let _ = b.await;
            })
            .await;
            if !omit.load(std::sync::atomic::Ordering::SeqCst) {
                if let Some(code) = status.code() {
                    let _ = handle.exit_status_request(id, code as u32).await;
                } else {
                    let _ = handle
                        .exit_signal_request(
                            id,
                            russh::Sig::KILL,
                            false,
                            String::new(),
                            String::new(),
                        )
                        .await;
                }
            }
            let _ = handle.eof(id).await;
            let _ = handle.close(id).await;
        });
        Ok(())
    }
    async fn signal(
        &mut self,
        id: ChannelId,
        _: russh::Sig,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(s) = self.stops.get(&id) {
            s.cancel();
        }
        Ok(())
    }
    async fn channel_close(&mut self, id: ChannelId, _: &mut Session) -> Result<(), Self::Error> {
        if let Some(s) = self.stops.remove(&id) {
            s.cancel();
        }
        Ok(())
    }
}
pub struct Fixture {
    pub port: u16,
    pub state: State,
    task: tokio::task::JoinHandle<()>,
    _directory: tempfile::TempDir,
}
impl Fixture {
    pub async fn start() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("host");
        assert!(
            std::process::Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let key = russh::keys::load_secret_key(&path, None).unwrap();
        let config = Arc::new(server::Config {
            keys: vec![key],
            auth_rejection_time: std::time::Duration::from_millis(1),
            ..Default::default()
        });
        let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = socket.local_addr().unwrap().port();
        let state = State::default();
        let mut handler = Handler {
            state: state.clone(),
            stops: HashMap::new(),
        };
        let task = tokio::spawn(async move {
            handler.run_on_socket(config, &socket).await.unwrap();
        });
        Self {
            port,
            state,
            task,
            _directory: directory,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
