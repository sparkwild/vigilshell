//! 原生输入进程的私有二进制响应。版本不匹配必须失败，不能错位解释秘密正文。
use std::io::{self, Write};
use tokio::io::{AsyncRead, AsyncReadExt};
use vigilshell_domain::workspace::WorkspaceError;
use zeroize::Zeroizing;
const MAGIC: &[u8; 4] = b"VCP2";

pub fn write(
    out: &mut impl Write,
    status: u8,
    input_consistent: Option<bool>,
    label: &str,
    value: &[u8],
) -> io::Result<()> {
    if status > 2 || label.len() > 1024 || value.len() > crate::MAX_SECRET {
        return Err(io::Error::other("invalid native response"));
    }
    out.write_all(MAGIC)?;
    out.write_all(&[
        status,
        match input_consistent {
            None => 0,
            Some(true) => 1,
            Some(false) => 2,
        },
    ])?;
    out.write_all(&(label.len() as u16).to_be_bytes())?;
    out.write_all(&(value.len() as u32).to_be_bytes())?;
    out.write_all(label.as_bytes())?;
    out.write_all(value)?;
    out.flush()
}
pub(crate) async fn read(
    source: &mut (impl AsyncRead + Unpin),
) -> Result<(Zeroizing<Vec<u8>>, String, Option<bool>), WorkspaceError> {
    let fail = |_| WorkspaceError::NativeUnavailable;
    let mut magic = [0; 4];
    source.read_exact(&mut magic).await.map_err(fail)?;
    if magic != *MAGIC {
        return Err(WorkspaceError::NativeUnavailable);
    }
    let status = source.read_u8().await.map_err(fail)?;
    let input_consistent = match source.read_u8().await.map_err(fail)? {
        0 => None,
        1 => Some(true),
        2 => Some(false),
        _ => return Err(WorkspaceError::NativeUnavailable),
    };
    let label_len = source.read_u16().await.map_err(fail)? as usize;
    let len = source.read_u32().await.map_err(fail)? as usize;
    if status > 2 || label_len > 1024 || len > crate::MAX_SECRET {
        return Err(WorkspaceError::NativeUnavailable);
    }
    let mut label = vec![0; label_len];
    let mut bytes = Zeroizing::new(vec![0; len]);
    source.read_exact(&mut label).await.map_err(fail)?;
    source.read_exact(&mut bytes).await.map_err(fail)?;
    match status {
        0 => Err(WorkspaceError::Cancelled),
        1 => Ok((
            bytes,
            String::from_utf8(label).map_err(|_| WorkspaceError::NativeUnavailable)?,
            input_consistent,
        )),
        _ => Err(WorkspaceError::InvalidKey),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn exact_utf8_bytes_and_input_check_survive_private_framing() {
        let input = "  fixture-密码\n\0end ".as_bytes();
        let mut frame = Zeroizing::new(Vec::new());
        write(&mut *frame, 1, Some(false), "", input).unwrap();
        let (bytes, label, same) = read(&mut frame.as_slice()).await.unwrap();
        assert!(bytes.as_slice() == input);
        assert!(label.is_empty());
        assert_eq!(same, Some(false));
        assert!(read(&mut &frame[..frame.len() - 1]).await.is_err());
        assert!(
            read(&mut b"\x01\x00\x00\x00\x00\x00\x08password".as_slice())
                .await
                .is_err()
        );
    }
}
