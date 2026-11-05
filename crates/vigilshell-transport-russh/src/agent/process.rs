use super::*;
pub(super) struct Process {
    pub(super) channel: Option<Channel<client::Msg>>,
    pub(super) pending: std::collections::VecDeque<AgentProcessEvent>,
}
impl Process {
    pub(super) async fn confirm(&mut self) -> Result<(), WorkspaceError> {
        tokio::time::timeout(Duration::from_secs(10), async {
            let mut size = 0;
            loop {
                match self
                    .channel
                    .as_mut()
                    .ok_or(WorkspaceError::Disconnected)?
                    .wait()
                    .await
                {
                    Some(ChannelMsg::Success) => return Ok(()),
                    Some(ChannelMsg::Data { data }) => {
                        size += data.len();
                        self.pending
                            .push_back(AgentProcessEvent::Stdout(data.to_vec()));
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        size += data.len();
                        self.pending
                            .push_back(AgentProcessEvent::Stderr(data.to_vec()));
                    }
                    Some(ChannelMsg::Failure | ChannelMsg::Close) | None => {
                        return Err(WorkspaceError::ChannelRejected);
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        self.pending.push_back(AgentProcessEvent::Exit(exit_status))
                    }
                    Some(ChannelMsg::ExitSignal {
                        signal_name,
                        core_dumped,
                        ..
                    }) => self.pending.push_back(AgentProcessEvent::ExitSignal {
                        signal: format!("{signal_name:?}"),
                        core_dumped,
                    }),
                    Some(ChannelMsg::Eof) => self.pending.push_back(AgentProcessEvent::Eof),
                    _ => {}
                }
                if size > 65536 || self.pending.len() > 64 {
                    return Err(WorkspaceError::StreamUnavailable);
                }
            }
        })
        .await
        .map_err(|_| WorkspaceError::Timeout)?
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if let Some(channel) = self.channel.take()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            runtime.spawn(async move {
                let _ = tokio::time::timeout(Duration::from_secs(1), async {
                    let _ = channel.signal(russh::Sig::TERM).await;
                    let _ = channel.close().await;
                })
                .await;
            });
        }
    }
}
impl AgentProcess for Process {
    fn read(&mut self) -> FutureResult<'_, AgentProcessEvent> {
        Box::pin(async move {
            if let Some(event) = self.pending.pop_front() {
                return Ok(event);
            }
            loop {
                let Some(channel) = self.channel.as_mut() else {
                    return Ok(AgentProcessEvent::Closed);
                };
                match channel.wait().await {
                    Some(ChannelMsg::Data { data }) => {
                        return Ok(AgentProcessEvent::Stdout(data.to_vec()));
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        return Ok(AgentProcessEvent::Stderr(data.to_vec()));
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        return Ok(AgentProcessEvent::Exit(exit_status));
                    }
                    Some(ChannelMsg::ExitSignal {
                        signal_name,
                        core_dumped,
                        ..
                    }) => {
                        return Ok(AgentProcessEvent::ExitSignal {
                            signal: format!("{signal_name:?}"),
                            core_dumped,
                        });
                    }
                    Some(ChannelMsg::Eof) => return Ok(AgentProcessEvent::Eof),
                    Some(ChannelMsg::Failure) => return Err(WorkspaceError::ChannelRejected),
                    Some(ChannelMsg::Close) | None => return Ok(AgentProcessEvent::Closed),
                    _ => {}
                }
            }
        })
    }
    fn input<'a>(&'a mut self, bytes: &'a [u8]) -> FutureResult<'a, ()> {
        Box::pin(async move {
            if bytes.len() > 65536 {
                return Err(WorkspaceError::InvalidInput);
            }
            self.channel
                .as_ref()
                .ok_or(WorkspaceError::Disconnected)?
                .data(bytes)
                .await
                .map_err(error)
        })
    }
    fn signal_stop(&mut self) -> FutureResult<'_, ()> {
        Box::pin(async move {
            self.channel
                .as_ref()
                .ok_or(WorkspaceError::Disconnected)?
                .signal(russh::Sig::TERM)
                .await
                .map_err(error)
        })
    }
    fn eof(&mut self) -> FutureResult<'_, ()> {
        Box::pin(async move {
            self.channel
                .as_ref()
                .ok_or(WorkspaceError::Disconnected)?
                .eof()
                .await
                .map_err(error)
        })
    }
    fn resize(&mut self, cols: u32, rows: u32) -> FutureResult<'_, ()> {
        Box::pin(async move {
            if !(1..=1000).contains(&cols) || !(1..=1000).contains(&rows) {
                return Err(WorkspaceError::InvalidInput);
            }
            self.channel
                .as_ref()
                .ok_or(WorkspaceError::Disconnected)?
                .window_change(cols, rows, 0, 0)
                .await
                .map_err(error)
        })
    }
    fn stop(&mut self) -> FutureResult<'_, ()> {
        Box::pin(async move {
            let Some(channel) = self.channel.take() else {
                return Ok(());
            };
            let _ = channel.signal(russh::Sig::TERM).await;
            channel.close().await.map_err(error)
        })
    }
}
