//! Agent exchanges have a separate bounded budget; ordinary RPC stays at 64 KiB.
use serde_json::Value;
use std::io;
use vigilshell_desktop_protocol::{MAX_AGENT_FRAME_BYTES, MAX_FRAME_BYTES};

pub(crate) fn validate_input(bytes: &[u8]) -> io::Result<()> {
    if bytes.len() <= MAX_FRAME_BYTES {
        return Ok(());
    }
    let agent = serde_json::from_slice::<Value>(bytes)
        .ok()
        .is_some_and(|v| v.get("method").and_then(Value::as_str) == Some("agent.request"));
    if bytes.len() > MAX_AGENT_FRAME_BYTES || !agent {
        return Err(io::Error::other("frame_too_large"));
    }
    Ok(())
}

pub(crate) fn encode_output(value: &Value) -> io::Result<Vec<u8>> {
    let agent = value.get("method").and_then(Value::as_str) == Some("agent.changed")
        || value.pointer("/result/result/kind").and_then(Value::as_str) == Some("agent");
    let limit = if agent {
        MAX_AGENT_FRAME_BYTES
    } else {
        MAX_FRAME_BYTES
    };
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > limit {
        tracing::error!(
            application = "vigilshell",
            stage = "rpc_output",
            code = "output_too_large",
            frame_bytes = bytes.len(),
            limit_bytes = limit
        );
        return Err(io::Error::other("output_too_large"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn large_agent_snapshots_and_requests_do_not_expand_other_rpc_limits() {
        let payload = "x".repeat(MAX_FRAME_BYTES + 1);
        let event = json!({"jsonrpc":"2.0","method":"agent.changed","params":{"snapshot":payload}});
        assert!(encode_output(&event).unwrap().len() > MAX_FRAME_BYTES);
        let response = json!({"jsonrpc":"2.0","id":"1","result":{"kind":"workspace","result":{"kind":"agent","payload":payload}}});
        assert!(encode_output(&response).is_ok());
        let request =
            serde_json::to_vec(&json!({"method":"agent.request","params":{"payload":payload}}))
                .unwrap();
        assert!(validate_input(&request).is_ok());
        let ordinary = json!({"method":"preferences.changed","params":{"payload":payload,"method":"agent.request"}});
        assert!(encode_output(&ordinary).is_err());
        assert!(validate_input(&serde_json::to_vec(&ordinary).unwrap()).is_err());
        let oversized = json!({"method":"agent.changed","params":{"payload":"x".repeat(MAX_AGENT_FRAME_BYTES)}});
        assert!(encode_output(&oversized).is_err());
        assert!(validate_input(&serde_json::to_vec(&json!({"method":"agent.request","params":{"payload":"x".repeat(MAX_AGENT_FRAME_BYTES)}})).unwrap()).is_err());
    }
}
