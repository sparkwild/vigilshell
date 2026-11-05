use vigilshell_application_contracts::ports::SshTransport;

use vigilshell_application_contracts::ports::{SecretMaterial, TransportEvent};
use vigilshell_domain::workspace::*;
use vigilshell_transport_russh::RusshTransport;
#[tokio::test]
#[ignore = "requires an isolated OpenSSH server and synthetic key directory"]
async fn openssh_key_algorithms_and_pty() {
    let directory = std::path::PathBuf::from(
        std::env::var("VIGILSHELL_TEST_SSH_DIRECTORY").expect("isolated fixture directory"),
    );
    let username = std::env::var("VIGILSHELL_TEST_SSH_USER").expect("isolated SSH user");
    for file in ["client-plain", "rsa", "ecdsa"] {
        let transport = RusshTransport;
        let input = ConnectionInput {
            notes: String::new(),
            terminal: Box::default(),
            proxy_id: None,
            tunnels: Vec::new(),
            environment: Environment::None,
            name: "OpenSSH fixture".into(),
            host: "127.0.0.1".into(),
            port: 22483,
            username: username.clone(),
            auth: AuthKind::PrivateKey,
        };
        let key = transport.probe(input.clone()).await.unwrap();
        let secret = SecretMaterial {
            diagnostic: None,
            kind: AuthKind::PrivateKey,
            bytes: std::fs::read(directory.join(file)).unwrap().into(),
            label: String::new(),
        };
        let mut session = transport.authenticate(input, key, secret).await.unwrap();
        session.open_terminal(80, 24).await.unwrap();
        session
            .write(b"unset HISTFILE; printf '\\nRUST_OPENSSH_OK\\n'\r")
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let mut output = Vec::new();
            loop {
                match session.read().await.unwrap() {
                    TransportEvent::Data(bytes) => {
                        output.extend(bytes);
                        assert!(output.len() < 1024 * 1024);
                        if output
                            .windows(b"\r\nRUST_OPENSSH_OK\r\n".len())
                            .any(|w| w == b"\r\nRUST_OPENSSH_OK\r\n")
                        {
                            break;
                        }
                    }
                    _ => panic!("SSH ended before command output"),
                }
            }
        })
        .await
        .unwrap();
        session.close().await.unwrap();
    }
}
