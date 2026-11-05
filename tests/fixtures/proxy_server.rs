use base64::Engine;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
pub async fn proxy(kind: &str, auth: bool) -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let kind = kind.to_string();
    (
        port,
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let kind = kind.clone();
                tokio::spawn(async move {
                    let target = if kind == "http" {
                        let mut b = vec![];
                        while !b.ends_with(b"\r\n\r\n") {
                            let Ok(n) = socket.read_u8().await else {
                                return;
                            };
                            b.push(n);
                            if b.len() > 8192 {
                                return;
                            }
                        }
                        let headers = String::from_utf8(b).unwrap();
                        if auth
                            && !headers.contains(
                                &base64::engine::general_purpose::STANDARD
                                    .encode("proxy-user:proxy-pass"),
                            )
                        {
                            let _ = socket
                                .write_all(b"HTTP/1.1 407 Auth required\r\n\r\n")
                                .await;
                            return;
                        }
                        let target = headers.split_whitespace().nth(1).unwrap().to_string();
                        if socket.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.is_err() {
                            return;
                        }
                        target
                    } else {
                        let mut p = [0; 2];
                        if socket.read_exact(&mut p).await.is_err() {
                            return;
                        }
                        let mut methods = vec![0; p[1] as usize];
                        socket.read_exact(&mut methods).await.unwrap();
                        socket
                            .write_all(&[5, if auth { 2 } else { 0 }])
                            .await
                            .unwrap();
                        if auth {
                            assert_eq!(socket.read_u8().await.unwrap(), 1);
                            let n = socket.read_u8().await.unwrap();
                            let mut user = vec![0; n as usize];
                            socket.read_exact(&mut user).await.unwrap();
                            let n = socket.read_u8().await.unwrap();
                            let mut password = vec![0; n as usize];
                            socket.read_exact(&mut password).await.unwrap();
                            let ok = user == b"proxy-user" && password == b"proxy-pass";
                            socket
                                .write_all(&[1, if ok { 0 } else { 1 }])
                                .await
                                .unwrap();
                            if !ok {
                                return;
                            }
                        }
                        let mut req = [0; 4];
                        if socket.read_exact(&mut req).await.is_err() {
                            return;
                        }
                        let host = match req[3] {
                            1 => {
                                let mut ip = [0; 4];
                                socket.read_exact(&mut ip).await.unwrap();
                                std::net::Ipv4Addr::from(ip).to_string()
                            }
                            3 => {
                                let n = socket.read_u8().await.unwrap();
                                let mut bytes = vec![0; n as usize];
                                socket.read_exact(&mut bytes).await.unwrap();
                                String::from_utf8(bytes).unwrap()
                            }
                            _ => return,
                        };
                        let port = socket.read_u16().await.unwrap();
                        socket
                            .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0])
                            .await
                            .unwrap();
                        format!("{host}:{port}")
                    };
                    if !target.starts_with("127.0.0.1:") {
                        return;
                    }
                    if let Ok(mut target) = TcpStream::connect(target).await {
                        let _ = tokio::io::copy_bidirectional(&mut socket, &mut target).await;
                    }
                });
            }
        }),
    )
}
