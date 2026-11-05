use super::*;
use serde_json::{Value, json};
use vigilshell_domain::agent::ExecutionEnvironment;
impl Connection {
    pub(super) async fn inspect_environment(&self) -> Result<ExecutionEnvironment, WorkspaceError> {
        native::inspect_environment(self).await
    }
    pub(super) async fn overview(&self) -> Result<Value, WorkspaceError> {
        let command = format!(
            "exec /usr/bin/env -i PATH=/usr/bin:/bin LC_ALL=C /bin/sh -c {}",
            vigilshell_domain::agent::shell_quote(crate::tools::FULL)
        );
        let sample =
            crate::tools::parse_sample(&crate::tools::shared_exec(&self.handle, &command).await?);
        if sample.view.memory_total.is_none() || sample.view.uptime.is_none() {
            return Err(WorkspaceError::ToolsUnavailable);
        }
        let cpu = {
            let mut previous = self.previous_cpu.lock().unwrap();
            let percent = match (*previous, sample.cpu_total, sample.cpu_idle) {
                (Some((old_total, old_idle)), Some(total), Some(idle))
                    if total > old_total
                        && idle >= old_idle
                        && idle - old_idle <= total - old_total =>
                {
                    Some(100. * (1. - (idle - old_idle) as f64 / (total - old_total) as f64))
                }
                _ => None,
            };
            *previous = sample.cpu_total.zip(sample.cpu_idle);
            percent
        };
        Ok(
            json!({"hostname":sample.view.hostname,"kernel":sample.view.kernel,"architecture":sample.view.architecture,"distribution":sample.view.distribution,"uptimeSeconds":sample.view.uptime,"load":sample.load,"cpu":{"percent":cpu,"totalTicks":sample.cpu_total,"idleTicks":sample.cpu_idle},"memory":{"total":sample.view.memory_total,"available":sample.view.memory_available,"free":sample.memory_free,"shared":sample.memory_shared,"buffers":sample.memory_buffers},"swap":{"total":sample.swap_total,"free":sample.swap_free},"disks":sample.view.disks.iter().map(|d|json!({"mount":d.mount,"total":d.total,"available":d.available})).collect::<Vec<_>>()}),
        )
    }
    pub(super) async fn processes(&self) -> Result<Value, WorkspaceError> {
        let text=self.control_bounded("exec /usr/bin/env -i PATH=/usr/bin:/bin LC_ALL=C /usr/bin/ps -eo pid=,ppid=,uid=,rss=,stat=,comm=".into(),512*1024).await?;
        let mut processes = Vec::new();
        let mut limited = false;
        for line in text.lines() {
            if processes.len() >= 4096 {
                limited = true;
                break;
            }
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 6 {
                return Err(WorkspaceError::StreamUnavailable);
            }
            let number = |i: usize| {
                fields[i]
                    .parse::<u64>()
                    .map_err(|_| WorkspaceError::StreamUnavailable)
            };
            processes.push(json!({"pid":number(0)?,"parentPid":number(1)?,"uid":number(2)?,"memoryKiB":number(3)?,"state":fields[4],"name":fields[5..].join(" ")}));
        }
        Ok(json!({"processes":processes,"limited":limited}))
    }
}
