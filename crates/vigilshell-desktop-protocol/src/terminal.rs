//! 固定头部的工作区二进制帧；正文不经过 JSON 或日志。
use std::io::{self, Read, Write};
pub const MAX_PAYLOAD: usize = 32 * 1024;
pub const HEADER_SIZE: usize = 45;
pub const INPUT: u8 = 1;
pub const CREDIT: u8 = 2;
pub const OUTPUT: u8 = 3;
pub const INPUT_ACK: u8 = 4;
pub const END: u8 = 5;
#[derive(Clone)]
pub struct TerminalFrame {
    pub kind: u8,
    pub owner: String,
    pub session: String,
    pub sequence: u64,
    pub bytes: Vec<u8>,
}
impl TerminalFrame {
    fn valid(&self) -> bool {
        self.sequence <= vigilshell_domain::MAX_REVISION
            && self.bytes.len() <= MAX_PAYLOAD
            && match self.kind {
                INPUT | OUTPUT => !self.bytes.is_empty(),
                CREDIT | INPUT_ACK => self.bytes.len() == 4,
                END => self.bytes.is_empty(),
                _ => false,
            }
    }
    pub fn write(&self, out: &mut impl Write) -> io::Result<()> {
        if !self.valid() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let owner = uuid::Uuid::parse_str(&self.owner).map_err(|_| io::ErrorKind::InvalidData)?;
        let session =
            uuid::Uuid::parse_str(&self.session).map_err(|_| io::ErrorKind::InvalidData)?;
        out.write_all(&[self.kind])?;
        out.write_all(owner.as_bytes())?;
        out.write_all(session.as_bytes())?;
        out.write_all(&self.sequence.to_be_bytes())?;
        out.write_all(&(self.bytes.len() as u32).to_be_bytes())?;
        out.write_all(&self.bytes)
    }
    pub fn read(input: &mut impl Read) -> io::Result<Option<Self>> {
        let mut header = [0; HEADER_SIZE];
        match input.read(&mut header[..1])? {
            0 => return Ok(None),
            1 => {}
            _ => unreachable!(),
        };
        input.read_exact(&mut header[1..])?;
        let length = u32::from_be_bytes(header[41..45].try_into().unwrap()) as usize;
        if length > MAX_PAYLOAD {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut frame = Self {
            kind: header[0],
            owner: uuid::Uuid::from_bytes(header[1..17].try_into().unwrap()).to_string(),
            session: uuid::Uuid::from_bytes(header[17..33].try_into().unwrap()).to_string(),
            sequence: u64::from_be_bytes(header[33..41].try_into().unwrap()),
            bytes: vec![0; length],
        };
        input.read_exact(&mut frame.bytes)?;
        if !frame.valid() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(Some(frame))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_binary_and_rejects_incomplete_or_oversize_frames() {
        let f = TerminalFrame {
            kind: OUTPUT,
            owner: "11111111-1111-4111-8111-111111111111".into(),
            session: "22222222-2222-4222-8222-222222222222".into(),
            sequence: 7,
            bytes: (0..=255).collect(),
        };
        let mut encoded = Vec::new();
        f.write(&mut encoded).unwrap();
        let decoded = TerminalFrame::read(&mut encoded.as_slice())
            .unwrap()
            .unwrap();
        assert_eq!(decoded.bytes, f.bytes);
        assert_eq!(decoded.sequence, 7);
        assert!(TerminalFrame::read(&mut &encoded[..encoded.len() - 1]).is_err());
        encoded[41..45].copy_from_slice(&((MAX_PAYLOAD + 1) as u32).to_be_bytes());
        assert!(TerminalFrame::read(&mut encoded.as_slice()).is_err());
    }
}
