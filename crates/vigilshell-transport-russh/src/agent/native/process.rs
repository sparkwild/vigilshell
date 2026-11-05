use super::*;
pub(super) struct Silent;
impl NativeOperationOutput for Silent {
    fn append(&self, _: &'static str, _: Vec<u8>) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
pub(super) struct ResultData {
    pub code: u32,
    pub text: String,
}
pub(super) async fn capture(
    connection: &Connection,
    command: String,
    output: Arc<dyn NativeOperationOutput>,
    control: &FileExecutionControl,
    stream: bool,
) -> AgentResult<ResultData> {
    if control.cancel.load(Ordering::Acquire) {
        return Err("native_cancelled");
    }
    let mut process = connection
        .open_exec_encoded(command, false, false, encoding_rs::UTF_8)
        .await
        .map_err(|_| "native_process_disconnected")?;
    let mut bytes = Vec::new();
    let mut code = None;
    let mut stopping = None;
    let mut interval = tokio::time::interval(Duration::from_millis(100));
    let result = loop {
        let event = tokio::select! {biased;
            _=interval.tick()=>{
                if control.cancel.load(Ordering::Acquire)&&stopping.is_none(){let _=tokio::time::timeout(Duration::from_secs(1),process.signal_stop()).await;stopping=Some(std::time::Instant::now());}
                if stopping.is_some_and(|at|at.elapsed()>Duration::from_secs(3)){break Err("native_result_unknown");}
                continue;
            },
            event=process.read()=>event.map_err(|_|"native_process_disconnected"),
        };
        match event {
            Ok(AgentProcessEvent::Stdout(data)) => {
                if data.len() > 65536 || bytes.len() + data.len() > 1024 * 1024 {
                    if stream {
                        bytes.clear();
                    } else {
                        break Err("native_output_limit");
                    }
                }
                if stream && output.append("stdout", data.clone()).await.is_err() {
                    break Err("native_output_unavailable");
                }
                bytes.extend(data);
            }
            Ok(AgentProcessEvent::Stderr(data)) => {
                if data.len() > 65536 {
                    break Err("native_output_limit");
                }
                if stream && output.append("stderr", data).await.is_err() {
                    break Err("native_output_unavailable");
                }
            }
            Ok(AgentProcessEvent::Exit(exit)) => {
                if code.replace(exit).is_some() {
                    break Err("native_result_unknown");
                }
            }
            Ok(AgentProcessEvent::ExitSignal { .. }) => break Err("native_result_unknown"),
            Ok(AgentProcessEvent::Eof) => {}
            Ok(AgentProcessEvent::Closed) => break code.ok_or("native_result_unknown"),
            Err(error) => break Err(error),
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(1), process.stop()).await;
    if control.cancel.load(Ordering::Acquire) {
        return Err("native_result_unknown");
    }
    Ok(ResultData {
        code: result?,
        text: if stream {
            String::from_utf8_lossy(&bytes).into_owned()
        } else {
            String::from_utf8(bytes).map_err(|_| "native_output_invalid")?
        },
    })
}

// 探测和正式操作使用相同的固定环境及参数转义。
pub(super) fn command(path: &str, args: &[String], environment: &[(&str, String)]) -> String {
    let environment = environment
        .iter()
        .map(|(k, v)| format!("{k}={}", shell_quote(v)))
        .collect::<Vec<_>>()
        .join(" ");
    let args = args
        .iter()
        .map(|v| shell_quote(v))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "exec /usr/bin/env -i PATH=/usr/sbin:/usr/bin:/sbin:/bin HOME=\"$HOME\" LC_ALL=C DEBIAN_FRONTEND=noninteractive SYSTEMD_COLORS=0 SYSTEMD_PAGER= {environment} {} {args}",
        shell_quote(path)
    )
}
