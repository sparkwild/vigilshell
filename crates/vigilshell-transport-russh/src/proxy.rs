use super::*;
use base64::Engine;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_socks::tcp::Socks5Stream;
fn socks_error(error: tokio_socks::Error) -> WorkspaceError {
    match error {
        tokio_socks::Error::PasswordAuthFailure(_)
        | tokio_socks::Error::AuthorizationRequired
        | tokio_socks::Error::NoAcceptableAuthMethods => WorkspaceError::ProxyAuthenticationFailed,
        _ => WorkspaceError::ProxyFailed,
    }
}
pub(super) async fn socket(
    input: &ConnectionInput,
    route: Option<ProxyRoute>,
) -> Result<(TcpStream, SocketGuard), WorkspaceError> {
    let Some(route) = route else {
        return super::socket(input).await;
    };
    route.proxy.validate()?;
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut endpoint = input.clone();
        endpoint.host = route.proxy.host.clone();
        endpoint.port = route.proxy.port;
        let (mut socket, guard) = super::socket(&endpoint)
            .await
            .map_err(|_| WorkspaceError::ProxyUnavailable)?;
        let password = route
            .secret
            .as_ref()
            .map(|s| {
                std::str::from_utf8(&s.bytes).map_err(|_| WorkspaceError::CredentialUnavailable)
            })
            .transpose()?;
        if route.proxy.kind == "socks5" {
            let connected = if let Some(password) = password {
                Socks5Stream::connect_with_password_and_socket(
                    socket,
                    (input.host.as_str(), input.port),
                    &route.proxy.username,
                    password,
                )
                .await
            } else {
                Socks5Stream::connect_with_socket(socket, (input.host.as_str(), input.port)).await
            };
            return Ok((connected.map_err(socks_error)?.into_inner(), guard));
        }
        let host = if input.host.contains(':') {
            format!("[{}]", input.host)
        } else {
            input.host.clone()
        };
        let target = format!("{}:{}", host, input.port);
        let mut request =
            Zeroizing::new(format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n"));
        if let Some(password) = password {
            let pair = Zeroizing::new(format!("{}:{}", route.proxy.username, password));
            let encoded =
                Zeroizing::new(base64::engine::general_purpose::STANDARD.encode(pair.as_bytes()));
            request.push_str("Proxy-Authorization: Basic ");
            request.push_str(encoded.as_str());
            request.push_str("\r\n");
        }
        request.push_str("\r\n");
        socket
            .write_all(request.as_bytes())
            .await
            .map_err(|_| WorkspaceError::ProxyFailed)?;
        // Consume exactly the response headers: any following SSH banner belongs to russh.
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            if headers.len() >= 8192 {
                return Err(WorkspaceError::ProxyFailed);
            }
            headers.push(
                socket
                    .read_u8()
                    .await
                    .map_err(|_| WorkspaceError::ProxyFailed)?,
            );
        }
        let mut fields = [httparse::EMPTY_HEADER; 64];
        let mut response = httparse::Response::new(&mut fields);
        response
            .parse(&headers)
            .map_err(|_| WorkspaceError::ProxyFailed)?;
        match response.code {
            Some(200..=299) => Ok((socket, guard)),
            Some(407) => Err(WorkspaceError::ProxyAuthenticationFailed),
            _ => Err(WorkspaceError::ProxyFailed),
        }
    })
    .await
    .map_err(|_| WorkspaceError::ProxyTimeout)?
}
