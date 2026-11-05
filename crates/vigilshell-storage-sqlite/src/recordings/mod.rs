//! 完整活动记录的分段文件；由现有 SQLite 工作线程串行调用，不受显示缓存额度影响。
mod repository;
mod segment;
pub(crate) use repository::execute;
#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod repository_tests;
#[cfg(test)]
mod tests;

use segment::{Frame, read_frame, write_frame};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom},
    path::{Path, PathBuf},
};
use vigilshell_domain::agent::{ActivityRecord, HistoricalActivity, RecordingPosition};

pub type Result<T> = std::result::Result<T, RecordingError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingError {
    InvalidRequest,
    Unavailable,
    Corrupt,
    Gap,
    Quota,
}

/// 索引提交失败或进程中断后，完整帧也不等于操作已执行；恢复由 SQLite 状态核对。
#[derive(Clone, Debug)]
pub struct RecoveredRecording {
    pub positions: Vec<RecordingPosition>,
    pub bytes: u64,
    pub incomplete: bool,
}

struct Writer {
    file: File,
    segment: u32,
    offset: u64,
    sequence: u64,
    digest: [u8; 32],
    failed: bool,
}

pub struct RecordingStore {
    root: PathBuf,
    writers: HashMap<String, Writer>,
    segment_bytes: u64,
}

impl RecordingStore {
    pub fn probe(&self) -> Result<()> {
        use std::io::Write;
        let path = self
            .root
            .join(format!(".probe-{}", vigilshell_domain::workspace::new_id()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|_| RecordingError::Unavailable)?;
            file.write_all(b"VSRLOG01 storage probe")
                .and_then(|_| file.sync_all())
                .map_err(|_| RecordingError::Unavailable)?;
            sync_directory(&self.root)
        })();
        let removed = fs::remove_file(path);
        result?;
        removed.map_err(|_| RecordingError::Unavailable)?;
        sync_directory(&self.root)
    }
    pub fn frame_bytes(record: &ActivityRecord) -> Result<u64> {
        Ok(Frame::encode(record, &[0; 32])?.bytes.len() as u64)
    }
    pub fn open(data_root: &Path) -> Result<Self> {
        if !data_root.is_absolute() {
            return Err(RecordingError::InvalidRequest);
        }
        let root = data_root.join("agent-recordings");
        private_directory(&root)?;
        Ok(Self {
            root,
            writers: HashMap::new(),
            segment_bytes: 8 * 1024 * 1024,
        })
    }

    pub(crate) fn open_read_only(data_root: &Path) -> Result<Self> {
        if !data_root.is_absolute() {
            return Err(RecordingError::InvalidRequest);
        }
        let root = data_root.join("agent-recordings");
        ordinary_directory(&root)?;
        Ok(Self {
            root,
            writers: HashMap::new(),
            segment_bytes: 8 * 1024 * 1024,
        })
    }

    fn directory(&self, session: &str) -> Result<PathBuf> {
        if session.is_empty()
            || session.len() > 128
            || !session
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        {
            return Err(RecordingError::InvalidRequest);
        }
        Ok(self.root.join(session))
    }

    /// 新会话仅能创建一次，重开历史记录不会恢复写入、更不会重放操作。
    pub fn create(&mut self, session: &str) -> Result<()> {
        let directory = self.directory(session)?;
        if directory
            .try_exists()
            .map_err(|_| RecordingError::Unavailable)?
        {
            return Err(RecordingError::InvalidRequest);
        }
        private_directory(&directory)?;
        let file = segment_file(&directory, 0)?;
        sync_directory(&directory)?;
        sync_directory(&self.root)?;
        self.writers.insert(
            session.into(),
            Writer {
                file,
                segment: 0,
                offset: 0,
                sequence: 0,
                digest: [0; 32],
                failed: false,
            },
        );
        Ok(())
    }

    /// 在交付执行输入之前调用。序号由唯一存储所有者分配，调用方不能跳号。
    pub fn append(
        &mut self,
        record: &mut ActivityRecord,
        available_bytes: u64,
    ) -> Result<RecordingPosition> {
        let directory = self.directory(&record.session_id)?;
        let writer = self
            .writers
            .get_mut(&record.session_id)
            .ok_or(RecordingError::InvalidRequest)?;
        if writer.failed {
            return Err(RecordingError::Gap);
        }
        record.sequence = writer
            .sequence
            .checked_add(1)
            .ok_or(RecordingError::InvalidRequest)?;
        let frame = Frame::encode(record, &writer.digest)?;
        if frame.bytes.len() as u64 > available_bytes {
            writer.failed = true;
            return Err(RecordingError::Quota);
        }
        let result = (|| {
            if writer.offset > 0 && writer.offset + frame.bytes.len() as u64 > self.segment_bytes {
                writer.segment = writer
                    .segment
                    .checked_add(1)
                    .ok_or(RecordingError::Unavailable)?;
                writer.file = segment_file(&directory, writer.segment)?;
                writer.offset = 0;
                sync_directory(&directory)?;
            }
            write_frame(&mut writer.file, &frame)?;
            let position = RecordingPosition {
                session_id: record.session_id.clone(),
                segment: writer.segment,
                offset: writer.offset,
                length: frame.bytes.len() as u32,
                sequence: record.sequence,
            };
            writer.offset += frame.bytes.len() as u64;
            writer.sequence = record.sequence;
            writer.digest = frame.digest;
            Ok(position)
        })();
        if result.is_err() {
            writer.failed = true;
        }
        result
    }

    pub fn close(&mut self, session: &str) {
        self.writers.remove(session);
    }

    fn read_verified_frame(&self, position: &RecordingPosition) -> Result<Frame> {
        let directory = self.directory(&position.session_id)?;
        ordinary_directory(&directory)?;
        let path = directory.join(segment_name(position.segment));
        ordinary_file(&path)?;
        let mut file = File::open(path).map_err(|_| RecordingError::Unavailable)?;
        file.seek(SeekFrom::Start(position.offset))
            .map_err(|_| RecordingError::Unavailable)?;
        let frame = read_frame(&mut file)?.ok_or(RecordingError::Gap)?;
        if frame.bytes.len() != position.length as usize {
            return Err(RecordingError::Corrupt);
        }
        let identity = frame.identity()?;
        if identity.sequence != position.sequence || identity.session_id != position.session_id {
            return Err(RecordingError::Corrupt);
        }
        Ok(frame)
    }

    pub fn read(&self, position: &RecordingPosition) -> Result<ActivityRecord> {
        self.read_verified_frame(position)?.record()
    }

    /// 读取已验证的原帧及展示字段，不写段文件、索引或不完整标记。
    ///
    /// # Errors
    /// 文件、原字节摘要、JSON 信封或索引位置无效时返回错误。
    pub fn read_history(&self, position: &RecordingPosition) -> Result<HistoricalActivity> {
        self.read_verified_frame(position)?.history(position)
    }

    /// 顺序核对分段、事件序号和哈希链，保留损坏尾段供诊断，绝不静默截断。
    pub fn recover(&self, session: &str) -> Result<RecoveredRecording> {
        let directory = self.directory(session)?;
        ordinary_directory(&directory)?;
        let mut names = fs::read_dir(&directory)
            .map_err(|_| RecordingError::Unavailable)?
            .map(|entry| {
                entry
                    .map(|entry| entry.file_name())
                    .map_err(|_| RecordingError::Unavailable)
            })
            .collect::<Result<Vec<_>>>()?;
        names.sort();
        let mut result = RecoveredRecording {
            positions: Vec::new(),
            bytes: 0,
            incomplete: names.is_empty(),
        };
        let mut previous = [0; 32];
        let mut sequence = 0;
        for (index, name) in names.iter().enumerate() {
            if name != &std::ffi::OsString::from(segment_name(index as u32)) {
                result.incomplete = true;
                break;
            }
            let path = directory.join(name);
            ordinary_file(&path)?;
            let mut file = File::open(path).map_err(|_| RecordingError::Unavailable)?;
            let mut offset = 0;
            loop {
                let frame = match read_frame(&mut file) {
                    Ok(Some(frame)) => frame,
                    Ok(None) => break,
                    Err(_) => {
                        result.incomplete = true;
                        break;
                    }
                };
                let identity = frame.identity()?;
                if frame.previous != previous
                    || identity.sequence != sequence + 1
                    || identity.session_id != session
                {
                    result.incomplete = true;
                    break;
                }
                let length = frame.bytes.len() as u32;
                result.positions.push(RecordingPosition {
                    session_id: session.into(),
                    segment: index as u32,
                    offset,
                    length,
                    sequence: identity.sequence,
                });
                offset += u64::from(length);
                previous = frame.digest;
                sequence = identity.sequence;
            }
            result.bytes += file
                .metadata()
                .map_err(|_| RecordingError::Unavailable)?
                .len();
            if result.incomplete {
                break;
            }
        }
        Ok(result)
    }

    /// 调用方必须先在 SQLite 将已结束会话标为待清理；活动会话不允许删除。
    pub fn remove_closed(&mut self, session: &str) -> Result<()> {
        if self.writers.contains_key(session) {
            return Err(RecordingError::InvalidRequest);
        }
        let path = self.directory(session)?;
        if !path.try_exists().map_err(|_| RecordingError::Unavailable)? {
            return Ok(());
        }
        ordinary_directory(&path)?;
        fs::remove_dir_all(path).map_err(|_| RecordingError::Unavailable)?;
        sync_directory(&self.root)
    }
}

fn segment_name(segment: u32) -> String {
    format!("{segment:08}.vsr")
}

fn segment_file(directory: &Path, segment: u32) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(directory.join(segment_name(segment)))
        .map_err(|_| RecordingError::Unavailable)
}

fn ordinary_file(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path)
        .map_err(|_| RecordingError::Unavailable)?
        .is_file()
    {
        Ok(())
    } else {
        Err(RecordingError::Corrupt)
    }
}
fn ordinary_directory(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path)
        .map_err(|_| RecordingError::Unavailable)?
        .is_dir()
    {
        Ok(())
    } else {
        Err(RecordingError::Corrupt)
    }
}
fn private_directory(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => ordinary_directory(path),
        Err(_) => Err(RecordingError::Unavailable),
    }
}
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| RecordingError::Unavailable)?;
    Ok(())
}
