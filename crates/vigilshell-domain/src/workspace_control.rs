//! 标签控制权的运行期状态；调用者必须在同一串行执行边界内校验并下发操作。
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentControl {
    pub entry_id: String,
    pub context_id: String,
    pub lease_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlError {
    Busy,
    Released,
    RequestConflict,
    Capacity,
    InvalidRequest,
}

#[derive(Clone, Debug, Default)]
pub struct WorkspaceControl {
    active: Option<AgentControl>,
    generation: u64,
    releasing: bool,
    requests: HashMap<(String, String), AgentControl>,
}

impl WorkspaceControl {
    pub fn active(&self) -> Option<&AgentControl> {
        self.active.as_ref()
    }

    pub fn manual_input_allowed(&self) -> bool {
        self.active.is_none() && !self.releasing
    }

    pub fn manual_generation(&self) -> Option<u64> {
        self.manual_input_allowed().then_some(self.generation)
    }

    /// entry/context 必须先经过业务授权。重复请求只返回仍有效的原控制权。
    pub fn acquire(
        &mut self,
        entry_id: &str,
        context_id: &str,
        request_key: &str,
    ) -> Result<AgentControl, ControlError> {
        if [entry_id, context_id, request_key]
            .iter()
            .any(|value| value.is_empty() || value.len() > 256)
        {
            return Err(ControlError::InvalidRequest);
        }
        let key = (entry_id.to_owned(), request_key.to_owned());
        if let Some(previous) = self.requests.get(&key) {
            if previous.context_id != context_id {
                return Err(ControlError::RequestConflict);
            }
            return if !self.releasing && self.active.as_ref() == Some(previous) {
                Ok(previous.clone())
            } else {
                Err(ControlError::Released)
            };
        }
        if self.active.is_some() || self.releasing {
            return Err(ControlError::Busy);
        }
        // 不淘汰旧回执，否则旧的重试可能重新获得用户已经解除的控制权。
        if self.requests.len() >= 256 {
            return Err(ControlError::Capacity);
        }
        let control = AgentControl {
            entry_id: entry_id.to_owned(),
            context_id: context_id.to_owned(),
            lease_id: uuid::Uuid::new_v4().to_string(),
        };
        self.requests.insert(key, control.clone());
        self.generation += 1;
        self.active = Some(control.clone());
        Ok(control)
    }

    pub fn check(&self, control: &AgentControl) -> Result<(), ControlError> {
        if !self.releasing && self.active.as_ref() == Some(control) {
            Ok(())
        } else {
            Err(ControlError::Released)
        }
    }

    /// 先撤销 Agent 的后续操作，保持人工输入关闭，直到调用者完成在途操作交接。
    pub fn begin_release(&mut self, lease_id: &str) -> Result<(), ControlError> {
        if self.active.as_ref().is_none_or(|c| c.lease_id != lease_id) {
            return Err(ControlError::Released);
        }
        self.releasing = true;
        Ok(())
    }

    /// 只允许当前交接完成；迟到的完成消息不能解除新一轮控制权。
    pub fn finish_release(&mut self, lease_id: &str) -> Result<(), ControlError> {
        if !self.releasing || self.active.as_ref().is_none_or(|c| c.lease_id != lease_id) {
            return Err(ControlError::Released);
        }
        self.generation += 1;
        self.active = None;
        self.releasing = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handover_revokes_old_operations_before_enabling_manual_input() {
        let mut state = WorkspaceControl::default();
        assert!(state.manual_input_allowed());
        let first = state.acquire("agent", "session", "first").unwrap();
        assert!(!state.manual_input_allowed());
        assert_eq!(
            state.acquire("agent", "session", "first"),
            Ok(first.clone())
        );
        state.begin_release(&first.lease_id).unwrap();
        assert_eq!(state.check(&first), Err(ControlError::Released));
        assert!(!state.manual_input_allowed());
        assert_eq!(
            state.acquire("agent", "session", "next"),
            Err(ControlError::Busy)
        );
        state.finish_release(&first.lease_id).unwrap();
        assert!(state.manual_input_allowed());
        assert_eq!(
            state.acquire("agent", "session", "first"),
            Err(ControlError::Released)
        );
        let next = state.acquire("agent", "session", "next").unwrap();
        assert_ne!(next.lease_id, first.lease_id);
        assert_eq!(state.check(&first), Err(ControlError::Released));
        assert_eq!(
            state.finish_release(&first.lease_id),
            Err(ControlError::Released)
        );
        assert_eq!(state.check(&next), Ok(()));
    }

    #[test]
    fn identities_and_tabs_do_not_share_control() {
        let mut one = WorkspaceControl::default();
        let mut two = WorkspaceControl::default();
        let control = one.acquire("agent-a", "session-a", "same-key").unwrap();
        assert_eq!(
            one.acquire("agent-b", "session-b", "same-key"),
            Err(ControlError::Busy)
        );
        assert_eq!(
            one.acquire("agent-a", "session-b", "same-key"),
            Err(ControlError::RequestConflict)
        );
        assert!(two.manual_input_allowed());
        assert_eq!(two.check(&control), Err(ControlError::Released));
        let another = two.acquire("agent-a", "session-a", "same-key").unwrap();
        assert_eq!(one.check(&another), Err(ControlError::Released));
    }

    #[test]
    fn stale_receipts_are_never_evicted_to_allow_reacquisition() {
        let mut state = WorkspaceControl::default();
        for index in 0..256 {
            let control = state
                .acquire("agent", "session", &index.to_string())
                .unwrap();
            state.begin_release(&control.lease_id).unwrap();
            state.finish_release(&control.lease_id).unwrap();
        }
        assert_eq!(
            state.acquire("agent", "session", "0"),
            Err(ControlError::Released)
        );
        assert_eq!(
            state.acquire("agent", "session", "new"),
            Err(ControlError::Capacity)
        );
        assert!(state.manual_input_allowed());
    }
}
