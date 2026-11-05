use super::{RecordingError, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
};
use vigilshell_domain::agent::{ActivityRecord, HistoricalActivity, RecordingPosition};

const MAGIC: &[u8; 8] = b"VSRLOG01";
const HEADER: usize = 8 + 4 + 32 + 32;
const MAX_RECORD: usize = 2 * 1024 * 1024;

pub(super) struct Frame {
    pub bytes: Vec<u8>,
    pub digest: [u8; 32],
    pub previous: [u8; 32],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FrameIdentity {
    pub session_id: String,
    pub sequence: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryEnvelope {
    session_id: String,
    sequence: u64,
    task_id: Option<String>,
    parent_task_id: Option<String>,
    timestamp_ms: u64,
    actor: Option<String>,
    policy_revision: Option<u64>,
    body: HistoryBody,
}

#[derive(Deserialize)]
struct HistoryBody {
    kind: String,
}

impl Frame {
    pub fn encode(record: &ActivityRecord, previous: &[u8; 32]) -> Result<Self> {
        let body = serde_json::to_vec(record).map_err(|_| RecordingError::InvalidRequest)?;
        if body.len() > MAX_RECORD {
            return Err(RecordingError::InvalidRequest);
        }
        let mut hash = Sha256::new();
        hash.update(MAGIC);
        hash.update(previous);
        hash.update(&body);
        let digest: [u8; 32] = hash.finalize().into();
        let mut bytes = Vec::with_capacity(HEADER + body.len());
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
        bytes.extend_from_slice(previous);
        bytes.extend_from_slice(&digest);
        bytes.extend_from_slice(&body);
        Ok(Self {
            bytes,
            digest,
            previous: *previous,
        })
    }

    pub fn record(&self) -> Result<ActivityRecord> {
        let bytes = &self.bytes[HEADER..];
        if let Ok(record) = serde_json::from_slice(bytes) {
            return Ok(record);
        }
        let mut value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| RecordingError::Corrupt)?;
        let kind = value
            .pointer("/body/kind")
            .and_then(|v| v.as_str())
            .ok_or(RecordingError::Corrupt)?
            .to_owned();
        value["body"] = serde_json::json!({"kind":"historical","originalKind":kind,"raw":std::str::from_utf8(bytes).map_err(|_|RecordingError::Corrupt)?});
        serde_json::from_value(value).map_err(|_| RecordingError::Corrupt)
    }

    pub fn identity(&self) -> Result<FrameIdentity> {
        serde_json::from_slice(&self.bytes[HEADER..]).map_err(|_| RecordingError::Corrupt)
    }

    pub fn history(&self, position: &RecordingPosition) -> Result<HistoricalActivity> {
        let raw_json =
            std::str::from_utf8(&self.bytes[HEADER..]).map_err(|_| RecordingError::Corrupt)?;
        let envelope: HistoryEnvelope =
            serde_json::from_str(raw_json).map_err(|_| RecordingError::Corrupt)?;
        Ok(HistoricalActivity {
            session_id: envelope.session_id,
            sequence: envelope.sequence,
            task_id: envelope.task_id,
            parent_task_id: envelope.parent_task_id,
            timestamp_ms: envelope.timestamp_ms,
            actor: envelope.actor,
            policy_revision: envelope.policy_revision,
            kind: envelope.body.kind,
            position: position.clone(),
            previous_digest: format!("{:x}", sha2::digest::Output::<Sha256>::from(self.previous)),
            digest: format!("{:x}", sha2::digest::Output::<Sha256>::from(self.digest)),
            raw_json: raw_json.into(),
        })
    }
}

pub(super) fn write_frame(file: &mut File, frame: &Frame) -> Result<()> {
    file.write_all(&frame.bytes)
        .and_then(|_| file.sync_data())
        .map_err(|_| RecordingError::Unavailable)
}

pub(super) fn read_frame(file: &mut File) -> Result<Option<Frame>> {
    let mut header = [0; HEADER];
    let first = loop {
        match file.read(&mut header[..1]) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => break result.map_err(|_| RecordingError::Unavailable)?,
        }
    };
    if first == 0 {
        return Ok(None);
    }
    file.read_exact(&mut header[1..])
        .map_err(|_| RecordingError::Gap)?;
    if &header[..8] != MAGIC {
        return Err(RecordingError::Corrupt);
    }
    let length = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if length > MAX_RECORD {
        return Err(RecordingError::Corrupt);
    }
    let previous = header[12..44].try_into().unwrap();
    let digest = header[44..76].try_into().unwrap();
    let mut bytes = header.to_vec();
    bytes.resize(HEADER + length, 0);
    file.read_exact(&mut bytes[HEADER..])
        .map_err(|_| RecordingError::Gap)?;
    let frame = Frame {
        bytes,
        previous,
        digest,
    };
    // 对原始字节计算摘要，避免重编码时字段次序变化掩盖损坏。
    let mut hash = Sha256::new();
    hash.update(MAGIC);
    hash.update(previous);
    hash.update(&frame.bytes[HEADER..]);
    let actual: [u8; 32] = hash.finalize().into();
    if actual != digest {
        return Err(RecordingError::Corrupt);
    }
    // 完整性只依赖稳定信封；未知历史事件不能被误判为段文件损坏。
    let identity = frame.identity()?;
    if identity.sequence == 0 || identity.session_id.is_empty() {
        return Err(RecordingError::Corrupt);
    }
    Ok(Some(frame))
}
