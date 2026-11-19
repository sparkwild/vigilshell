//! 限制按真实执行类型计算；传输、终端和普通任务不共享默认时限或连接名额。
use super::*;

pub const CONCURRENCY_MIN: u16 = 1;
pub const CONCURRENCY_MAX: u16 = 16;
const DURATION_MIN: u32 = 1;

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
#[cfg_attr(feature = "schema", schemars(extend("x-vigil-defaults" = ExecutionLimits::default())))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionLimits {
    #[cfg_attr(feature = "schema", schemars(range(min = DURATION_MIN, max = u32::MAX)))]
    pub max_task_seconds: Option<u32>,
    #[cfg_attr(feature = "schema", schemars(range(min = DURATION_MIN, max = u32::MAX)))]
    pub terminal_seconds: Option<u32>,
    #[cfg_attr(feature = "schema", schemars(range(min = DURATION_MIN, max = u32::MAX)))]
    pub terminal_idle_seconds: Option<u32>,
    #[cfg_attr(feature = "schema", schemars(range(min = DURATION_MIN, max = u32::MAX)))]
    pub transfer_seconds: Option<u32>,
    #[cfg_attr(feature = "schema", schemars(range(min = DURATION_MIN, max = u32::MAX)))]
    pub transfer_stall_seconds: Option<u32>,
    #[cfg_attr(feature = "schema", schemars(range(min = CONCURRENCY_MIN, max = CONCURRENCY_MAX)))]
    pub concurrent_tasks: u16,
    #[cfg_attr(feature = "schema", schemars(range(min = CONCURRENCY_MIN, max = CONCURRENCY_MAX)))]
    pub concurrent_transfers: u16,
    #[cfg_attr(feature = "schema", schemars(range(min = CONCURRENCY_MIN, max = CONCURRENCY_MAX)))]
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
        .into_iter()
        .flatten()
        .any(|v| v < DURATION_MIN)
            || [
                self.concurrent_tasks,
                self.concurrent_transfers,
                self.concurrent_terminals,
            ]
            .iter()
            .any(|v| !(CONCURRENCY_MIN..=CONCURRENCY_MAX).contains(v))
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

    #[test]
    fn full_policies_and_limit_patches_agree_on_boundaries() {
        for value in [
            serde_json::Value::Null,
            serde_json::json!(0),
            serde_json::json!(1),
            serde_json::json!(2_147_483_648_u64),
            serde_json::json!(u32::MAX),
            serde_json::json!(u64::from(u32::MAX) + 1),
        ] {
            let expected = value.is_null()
                || value
                    .as_u64()
                    .is_some_and(|v| (1..=u64::from(u32::MAX)).contains(&v));
            for field in [
                "maxTaskSeconds",
                "terminalSeconds",
                "terminalIdleSeconds",
                "transferSeconds",
                "transferStallSeconds",
            ] {
                let base = ConnectionPolicy {
                    connection_id: "fixture".into(),
                    ..Default::default()
                };
                let mut policy = serde_json::to_value(base.clone()).unwrap();
                policy["execution"]["limits"][field] = value.clone();
                assert_eq!(
                    serde_json::from_value::<ConnectionPolicy>(policy).is_ok(),
                    expected
                );
                let patch = serde_json::from_value::<PolicySettingsPatch>(serde_json::json!({
                    "field":"limit","value":{"key":field,"value":value}
                }));
                assert_eq!(
                    patch.is_ok_and(|patch| patch.apply(&base).is_ok()),
                    expected
                );
            }
        }
        let base = ConnectionPolicy {
            connection_id: "fixture".into(),
            ..Default::default()
        };
        let limits = PolicySettingsPatch::Limit(PolicyLimitPatch {
            key: PolicyLimitKey::MaxTaskSeconds,
            value: Some(u32::MAX),
        })
        .apply(&base)
        .unwrap()
        .execution
        .limits;
        assert_eq!(
            limits.timeout(ExecutionClass::Task, None),
            Ok(Some(u32::MAX))
        );
        assert_eq!(
            limits.timeout(ExecutionClass::Terminal, None),
            Ok(Some(7200))
        );
        assert_eq!(limits.timeout(ExecutionClass::Transfer, None), Ok(None));
        for v in [1, 16, 17] {
            let patched = PolicySettingsPatch::Limit(PolicyLimitPatch {
                key: PolicyLimitKey::ConcurrentTasks,
                value: Some(v),
            })
            .apply(&base);
            assert_eq!(patched.is_ok(), v <= 16);
        }
    }
}
