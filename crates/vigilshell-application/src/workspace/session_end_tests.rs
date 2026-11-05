use super::*;

struct EndingSession(
    VecDeque<Result<TransportEvent, WorkspaceError>>,
    CancellationToken,
);
impl SshSession for EndingSession {
    fn open_terminal(&mut self, _: u16, _: u16) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn read(&mut self) -> FutureResult<'_, TransportEvent> {
        let next = self
            .0
            .pop_front()
            .expect("terminal must stop reading after the end");
        Box::pin(async { next })
    }
    fn write<'a>(&'a mut self, _: &'a [u8]) -> FutureResult<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn resize(&mut self, _: u16, _: u16) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn close(&mut self) -> FutureResult<'_, ()> {
        self.1.cancel();
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn terminal_end_preserves_output_exit_reason_and_errors() {
    let cases = [
        (
            vec![Ok(TransportEvent::Exit(0)), Ok(TransportEvent::Eof)],
            false,
            SessionEndKind::RemoteExit,
            Some(0),
            None,
        ),
        (
            vec![Ok(TransportEvent::Exit(7)), Ok(TransportEvent::Eof)],
            false,
            SessionEndKind::RemoteExit,
            Some(7),
            None,
        ),
        (
            vec![Ok(TransportEvent::Eof)],
            false,
            SessionEndKind::RemoteClosed,
            None,
            None,
        ),
        (
            vec![Ok(TransportEvent::Disconnected {
                error: None,
                message: Some("server policy".into()),
            })],
            false,
            SessionEndKind::RemoteClosed,
            None,
            None,
        ),
        (
            vec![Err(WorkspaceError::Timeout)],
            false,
            SessionEndKind::TransportError,
            None,
            Some(WorkspaceError::Timeout),
        ),
        (
            vec![Ok(TransportEvent::Disconnected {
                error: Some(WorkspaceError::ConnectionReset),
                message: None,
            })],
            false,
            SessionEndKind::TransportError,
            None,
            Some(WorkspaceError::ConnectionReset),
        ),
        (vec![], true, SessionEndKind::UserDisconnected, None, None),
    ];
    for (events, cancelled, kind, exit, error) in cases {
        let mut f = Fixture::new();
        let output = Arc::new(Mutex::new(Vec::new()));
        Arc::get_mut(&mut f.service).unwrap().sink = Arc::new(EchoSink(output.clone()));
        let profile = f.create().await;
        let id = new_id();
        let cancel = CancellationToken::new();
        f.service.state.lock().unwrap().sessions.insert(
            id.clone(),
            Session {
                control_deadline: None,
                control_idle_deadline: None,
                control_idle_seconds: None,
                control: Default::default(),
                write_gate: Default::default(),
                output_history: Default::default(),
                output_bytes: 0,
                output_cursor: 0,
                agent_inputs: Default::default(),
                owner: f.owner.clone(),
                view: SessionView {
                    agent_control: None,
                    group_id: new_id(),
                    id: id.clone(),
                    operation_id: new_id(),
                    connection: profile,
                    phase: "running".into(),
                    error: None,
                    exit_code: None,
                    end_reason: None,
                },
                cancel: cancel.clone(),
                controls: None,
            },
        );
        let mut sequence = VecDeque::from([Ok(TransportEvent::Data(b"last output".to_vec()))]);
        sequence.extend(events);
        let (_tx, rx) = mpsc::channel(1);
        if cancelled {
            cancel.cancel();
        }
        let result = f
            .service
            .run_session(
                &f.owner,
                &id,
                Box::new(EndingSession(sequence, cancel.clone())),
                rx,
                cancel,
            )
            .await;
        assert_eq!(result.err(), error);
        {
            let state = f.service.state.lock().unwrap();
            let view = &state.sessions[&id].view;
            assert_eq!(view.end_reason.as_ref().unwrap().kind, kind);
            assert_eq!(view.exit_code, exit);
            if !cancelled {
                assert_eq!(*output.lock().unwrap(), b"last output");
            }
        }
        f.service.state.lock().unwrap().sessions.remove(&id);
        f.service.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn ssh_eof_and_disconnect_preserve_exit_status_reason_and_final_output() {
    for expected_exit in [Some(7), None] {
        let ssh = live_fixture::Fixture::start().await;
        *ssh.shared.exit_on_input.lock().unwrap() = expected_exit;
        if expected_exit.is_none() {
            *ssh.shared.disconnect_on_input.lock().unwrap() = Some("fixture remote policy".into());
        }
        let transport = vigilshell_transport_russh::RusshTransport;
        let mut input = request(None, None).input;
        input.host = "127.0.0.1".into();
        input.port = ssh.port;
        input.username = "fixture".into();
        let key = transport.probe(input.clone()).await.unwrap();
        let mut session = transport
            .authenticate(
                input,
                key,
                SecretMaterial {
                    diagnostic: None,
                    kind: AuthKind::Password,
                    bytes: b"sidebar-fixture".to_vec().into(),
                    label: "test".into(),
                },
            )
            .await
            .unwrap();
        session.open_terminal(80, 24).await.unwrap();
        session.write(b"last output").await.unwrap();
        let (output, exit, message) = tokio::time::timeout(Duration::from_secs(3), async {
            let mut output = Vec::new();
            let mut exit = None;
            let mut message = None;
            loop {
                match session.read().await.unwrap() {
                    TransportEvent::Data(bytes) => output.extend(bytes),
                    TransportEvent::Exit(code) => exit = Some(code),
                    TransportEvent::Eof => break,
                    TransportEvent::Disconnected {
                        error: None,
                        message: remote,
                    } => {
                        message = remote;
                        break;
                    }
                    _ => panic!("unexpected termination"),
                }
            }
            (output, exit, message)
        })
        .await
        .unwrap();
        assert_eq!(output, b"last output");
        assert_eq!(exit, expected_exit);
        assert_eq!(
            message.as_deref(),
            if expected_exit.is_none() {
                Some("fixture remote policy")
            } else {
                None
            }
        );
        let _ = session.close().await;
    }
}
