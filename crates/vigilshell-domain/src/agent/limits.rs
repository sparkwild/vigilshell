//! 限制按真实执行类型计算；传输、终端和普通任务不共享默认时限或连接名额。
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ExecutionClass {
    Task,
    Transfer,
    Terminal,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionLimits {
    pub max_task_seconds: Option<u32>,
    pub terminal_seconds: Option<u32>,
    pub terminal_idle_seconds: Option<u32>,
    pub transfer_seconds: Option<u32>,
    pub transfer_stall_seconds: Option<u32>,
    pub concurrent_tasks: u16,
    pub concurrent_transfers: u16,
    pub concurrent_terminals: u16,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            max_task_seconds: Some(1800),
            terminal_seconds: Some(7200),
            terminal_idle_seconds: None,
            transfer_seconds: None,
            transfer_stall_seconds: Some(300),
            concurrent_tasks: 4,
            concurrent_transfers: 2,
            concurrent_terminals: 2,
        }
    }
}

impl ExecutionLimits {
    pub fn validate(&self) -> AgentResult<()> {
        if [
            self.max_task_seconds,
            self.terminal_seconds,
            self.terminal_idle_seconds,
            self.transfer_seconds,
            self.transfer_stall_seconds,
        ]
        .contains(&Some(0))
            || [
                self.concurrent_tasks,
                self.concurrent_transfers,
                self.concurrent_terminals,
            ]
            .iter()
            .any(|v| !(1..=16).contains(v))
        {
            return Err("invalid_limits");
        }
        Ok(())
    }

    /// 缺省使用所属类型上限；调用者只能缩短时限，不能以零绕过上限。
    pub fn timeout(
        &self,
        class: ExecutionClass,
        requested: Option<u32>,
    ) -> AgentResult<Option<u32>> {
        self.validate()?;
        let ceiling = match class {
            ExecutionClass::Task => self.max_task_seconds,
            ExecutionClass::Transfer => self.transfer_seconds,
            ExecutionClass::Terminal => self.terminal_seconds,
        };
        match requested {
            Some(0) => Err("invalid_timeout"),
            Some(seconds) if ceiling.is_some_and(|maximum| seconds > maximum) => {
                Err("invalid_timeout")
            }
            Some(seconds) => Ok(Some(seconds)),
            None => Ok(ceiling),
        }
    }

    pub fn concurrency(&self, class: ExecutionClass) -> usize {
        usize::from(match class {
            ExecutionClass::Task => self.concurrent_tasks,
            ExecutionClass::Transfer => self.concurrent_transfers,
            ExecutionClass::Terminal => self.concurrent_terminals,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_types_have_independent_deadlines() {
        let limits = ExecutionLimits::default();
        assert_eq!(limits.timeout(ExecutionClass::Task, None), Ok(Some(1800)));
        assert_eq!(
            limits.timeout(ExecutionClass::Terminal, None),
            Ok(Some(7200))
        );
        assert_eq!(
            limits.timeout(ExecutionClass::Terminal, Some(3600)),
            Ok(Some(3600))
        );
        assert_eq!(
            limits.timeout(ExecutionClass::Transfer, Some(86400)),
            Ok(Some(86400))
        );
        assert_eq!(limits.timeout(ExecutionClass::Transfer, None), Ok(None));
        assert_eq!(
            limits.timeout(ExecutionClass::Task, Some(1801)),
            Err("invalid_timeout")
        );
        assert_eq!(
            limits.timeout(ExecutionClass::Task, Some(0)),
            Err("invalid_timeout")
        );
    }

    #[test]
    fn execution_classes_have_independent_concurrency() {
        let limits = ExecutionLimits {
            concurrent_tasks: 1,
            concurrent_transfers: 2,
            ..ExecutionLimits::default()
        };
        assert_eq!(limits.validate(), Ok(()));
        assert_eq!(limits.concurrency(ExecutionClass::Task), 1);
        assert_eq!(limits.concurrency(ExecutionClass::Terminal), 2);
        assert_eq!(limits.concurrency(ExecutionClass::Transfer), 2);
    }
}
