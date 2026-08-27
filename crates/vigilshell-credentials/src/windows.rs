use std::{
    ffi::c_void,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::windows::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    ptr,
};

use sha2::{Digest, Sha256};
use vigilshell_core::connection::CredentialRef;
use windows_sys::Win32::{
    Foundation::{ERROR_CANCELLED, LocalFree, NO_ERROR},
    Security::{
        Credentials::{
            CREDUI_FLAGS_ALWAYS_SHOW_UI, CREDUI_FLAGS_DO_NOT_PERSIST,
            CREDUI_FLAGS_GENERIC_CREDENTIALS, CREDUI_FLAGS_PASSWORD_ONLY_OK, CREDUI_INFOW,
            CREDUI_MAX_USERNAME_LENGTH, CredUIPromptForCredentialsW,
        },
        Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    },
    Storage::FileSystem::{
        MOVEFILE_WRITE_THROUGH, MoveFileExW, REPLACEFILE_WRITE_THROUGH, ReplaceFileW,
    },
    UI::WindowsAndMessaging::{IDYES, MB_DEFBUTTON2, MB_ICONWARNING, MB_YESNO, MessageBoxW},
};
use zeroize::{Zeroize, Zeroizing};

use crate::{
    CredentialProviderError, MAX_PASSWORD_BYTES, NativePasswordPrompt, PasswordConfigurationStatus,
    PasswordPromptSettlement, PasswordStore,
};

const ENVELOPE_MAGIC: &[u8; 4] = b"VDP1";
const INNER_MAGIC: &[u8; 4] = b"VSI1";
const FORMAT_VERSION: u8 = 1;
const MATERIAL_KIND_PASSWORD: u8 = 1;
const OUTER_HEADER_BYTES: usize = 4 + 1 + 1 + 32 + 4;
const INNER_HEADER_BYTES: usize = 4 + 1 + 1 + 32 + 4;
const INNER_CHECKSUM_BYTES: usize = 32;
const MAX_ENVELOPE_BYTES: usize = 128 * 1024;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// Current-User DPAPI 密文文件支持的 Human 密码存储。
#[derive(Debug)]
pub(crate) struct WindowsDpapiPasswordStore {
    root: PathBuf,
}

impl WindowsDpapiPasswordStore {
    pub(crate) fn new(root: PathBuf) -> Result<Self, CredentialProviderError> {
        validate_directory(&root)?;
        Ok(Self { root })
    }

    fn path(&self, credential_ref: &CredentialRef) -> PathBuf {
        self.root.join(format!(
            "{}.vdp",
            hex(&locator_digest(credential_ref.as_str().as_bytes()))
        ))
    }
}

impl PasswordStore for WindowsDpapiPasswordStore {
    fn password_configuration_status(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<PasswordConfigurationStatus, CredentialProviderError> {
        match validate_regular_file(&self.path(credential_ref)) {
            Ok(()) => Ok(PasswordConfigurationStatus::Configured),
            Err(CredentialProviderError::Missing) => Ok(PasswordConfigurationStatus::NotConfigured),
            Err(error) => Err(error),
        }
    }

    fn set_password(
        &self,
        credential_ref: &CredentialRef,
        password: &[u8],
    ) -> Result<(), CredentialProviderError> {
        if password.is_empty() || password.len() > MAX_PASSWORD_BYTES {
            return Err(CredentialProviderError::InvalidSecret);
        }
        validate_directory(&self.root)?;
        let digest = locator_digest(credential_ref.as_str().as_bytes());
        let inner = encode_inner(password, &digest)?;
        let ciphertext = protect(&inner, &digest)?;
        let envelope = encode_envelope(&ciphertext, &digest)?;
        atomic_replace(&self.root, &self.path(credential_ref), &envelope)
    }

    fn password(&self, credential_ref: &CredentialRef) -> Result<Vec<u8>, CredentialProviderError> {
        let digest = locator_digest(credential_ref.as_str().as_bytes());
        let path = self.path(credential_ref);
        validate_regular_file(&path)?;
        let file = File::open(path).map_err(|_| CredentialProviderError::Unavailable)?;
        let metadata = file
            .metadata()
            .map_err(|_| CredentialProviderError::Unavailable)?;
        if metadata.len() == 0 || metadata.len() > MAX_ENVELOPE_BYTES as u64 {
            return Err(CredentialProviderError::InvalidSecret);
        }
        let mut envelope = Vec::with_capacity(metadata.len() as usize);
        file.take((MAX_ENVELOPE_BYTES + 1) as u64)
            .read_to_end(&mut envelope)
            .map_err(|_| CredentialProviderError::Unavailable)?;
        if envelope.len() > MAX_ENVELOPE_BYTES {
            return Err(CredentialProviderError::InvalidSecret);
        }
        let ciphertext = decode_envelope(&envelope, &digest)?;
        let inner = unprotect(ciphertext, &digest)?;
        Ok(decode_inner(&inner, &digest)?.to_vec())
    }

    fn remove_password(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<(), CredentialProviderError> {
        let path = self.path(credential_ref);
        validate_regular_file(&path)?;
        fs::remove_file(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                CredentialProviderError::Missing
            } else {
                CredentialProviderError::Unavailable
            }
        })
    }
}

pub(crate) struct WindowsPasswordPrompt;

impl NativePasswordPrompt for WindowsPasswordPrompt {
    fn capture_password_on_main_thread(
        &self,
    ) -> Result<PasswordPromptSettlement, CredentialProviderError> {
        let target = wide("VigilShell SSH password");
        let caption = wide("Configure SSH password");
        let message = wide(
            "The password is protected for the current Windows user and is never sent to the app interface.",
        );
        let mut username = Zeroizing::new(vec![0_u16; CREDUI_MAX_USERNAME_LENGTH as usize + 1]);
        let mut password = Zeroizing::new(vec![0_u16; MAX_PASSWORD_BYTES + 1]);
        let mut save = 0;
        let info = CREDUI_INFOW {
            cbSize: std::mem::size_of::<CREDUI_INFOW>() as u32,
            hwndParent: ptr::null_mut(),
            pszMessageText: message.as_ptr(),
            pszCaptionText: caption.as_ptr(),
            hbmBanner: ptr::null_mut(),
        };
        let flags = CREDUI_FLAGS_ALWAYS_SHOW_UI
            | CREDUI_FLAGS_DO_NOT_PERSIST
            | CREDUI_FLAGS_GENERIC_CREDENTIALS
            | CREDUI_FLAGS_PASSWORD_ONLY_OK;
        // SAFETY: 所有 UTF-16 缓冲区在调用期间保持有效并包含终止 NUL；输出缓冲区
        // 长度与传入容量一致，函数返回后由 Zeroizing 清零。
        let result = unsafe {
            CredUIPromptForCredentialsW(
                &info,
                target.as_ptr(),
                ptr::null(),
                0,
                username.as_mut_ptr(),
                u32::try_from(username.len()).map_err(|_| CredentialProviderError::Unavailable)?,
                password.as_mut_ptr(),
                u32::try_from(password.len()).map_err(|_| CredentialProviderError::Unavailable)?,
                &mut save,
                flags,
            )
        };
        if result == ERROR_CANCELLED {
            return Ok(PasswordPromptSettlement::Cancelled);
        }
        if result != NO_ERROR {
            return Err(CredentialProviderError::Unavailable);
        }
        let length = password
            .iter()
            .position(|unit| *unit == 0)
            .ok_or(CredentialProviderError::InvalidSecret)?;
        if length == 0 {
            return Err(CredentialProviderError::InvalidSecret);
        }
        let value = String::from_utf16(&password[..length])
            .map_err(|_| CredentialProviderError::InvalidSecret)?;
        let bytes = Zeroizing::new(value.into_bytes());
        if bytes.len() > MAX_PASSWORD_BYTES {
            return Err(CredentialProviderError::InvalidSecret);
        }
        Ok(PasswordPromptSettlement::Submitted(bytes))
    }

    fn confirm_removal_on_main_thread(&self) -> Result<bool, CredentialProviderError> {
        let caption = wide("Remove saved SSH password?");
        let message = wide("VigilShell will remove this password from its protected local store.");
        // SAFETY: 两个 UTF-16 字符串都以 NUL 结尾，并在同步调用期间保持有效。
        let result = unsafe {
            MessageBoxW(
                ptr::null_mut(),
                message.as_ptr(),
                caption.as_ptr(),
                MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
            )
        };
        Ok(result == IDYES)
    }
}

fn validate_directory(path: &Path) -> Result<(), CredentialProviderError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| CredentialProviderError::Unavailable)?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(CredentialProviderError::Unavailable);
    }
    Ok(())
}

fn validate_regular_file(path: &Path) -> Result<(), CredentialProviderError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CredentialProviderError::Missing
        } else {
            CredentialProviderError::Unavailable
        }
    })?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(CredentialProviderError::InvalidSecret);
    }
    Ok(())
}

fn atomic_replace(root: &Path, target: &Path, bytes: &[u8]) -> Result<(), CredentialProviderError> {
    let (temporary, mut file) = create_temporary_file(root)?;
    let result = (|| {
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| CredentialProviderError::Unavailable)?;
        drop(file);
        let temporary_wide = path_wide(&temporary)?;
        let target_wide = path_wide(target)?;
        match fs::symlink_metadata(target) {
            Ok(_) => {
                validate_regular_file(target)?;
                // SAFETY: 路径均以 NUL 结尾；目标已验证为普通非 reparse 文件，替换文件
                // 位于同一受保护目录，调用期间两个缓冲区保持有效。
                let success = unsafe {
                    ReplaceFileW(
                        target_wide.as_ptr(),
                        temporary_wide.as_ptr(),
                        ptr::null(),
                        REPLACEFILE_WRITE_THROUGH,
                        ptr::null(),
                        ptr::null(),
                    )
                };
                if success == 0 {
                    return Err(CredentialProviderError::Unavailable);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // SAFETY: 路径均以 NUL 结尾，临时文件位于目标目录且已同步落盘。
                let success = unsafe {
                    MoveFileExW(
                        temporary_wide.as_ptr(),
                        target_wide.as_ptr(),
                        MOVEFILE_WRITE_THROUGH,
                    )
                };
                if success == 0 {
                    return Err(CredentialProviderError::Unavailable);
                }
            }
            Err(_) => return Err(CredentialProviderError::Unavailable),
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn create_temporary_file(root: &Path) -> Result<(PathBuf, File), CredentialProviderError> {
    for _ in 0..32 {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).map_err(|_| CredentialProviderError::Unavailable)?;
        let path = root.join(format!(".{}.tmp", hex(&random)));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(CredentialProviderError::Unavailable),
        }
    }
    Err(CredentialProviderError::Unavailable)
}

fn protect(
    material: &[u8],
    digest: &[u8; 32],
) -> Result<Zeroizing<Vec<u8>>, CredentialProviderError> {
    let entropy = entropy(digest);
    let input = blob(material)?;
    let entropy = blob(&entropy)?;
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: 输入缓冲区在同步调用期间保持有效；输出由 DPAPI 分配并在复制清零后
    // 通过 LocalFree 释放；禁止任何 DPAPI UI。
    let success = unsafe {
        CryptProtectData(
            &input,
            ptr::null(),
            &entropy,
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if success == 0 {
        return Err(CredentialProviderError::Unavailable);
    }
    copy_and_free_blob(output)
}

fn unprotect(
    ciphertext: &[u8],
    digest: &[u8; 32],
) -> Result<Zeroizing<Vec<u8>>, CredentialProviderError> {
    let entropy = entropy(digest);
    let input = blob(ciphertext)?;
    let entropy = blob(&entropy)?;
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: 输入缓冲区在同步调用期间保持有效；禁止 UI；成功输出按 DPAPI
    // 所有权规则在复制清零后释放。
    let success = unsafe {
        CryptUnprotectData(
            &input,
            ptr::null_mut(),
            &entropy,
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if success == 0 {
        return Err(CredentialProviderError::InvalidSecret);
    }
    copy_and_free_blob(output)
}

fn blob(bytes: &[u8]) -> Result<CRYPT_INTEGER_BLOB, CredentialProviderError> {
    Ok(CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len()).map_err(|_| CredentialProviderError::InvalidSecret)?,
        pbData: bytes.as_ptr().cast_mut(),
    })
}

fn copy_and_free_blob(
    blob: CRYPT_INTEGER_BLOB,
) -> Result<Zeroizing<Vec<u8>>, CredentialProviderError> {
    if blob.pbData.is_null() || blob.cbData == 0 {
        if !blob.pbData.is_null() {
            // SAFETY: 指针来自 DPAPI 分配器，并且只在此释放一次。
            let _ = unsafe { LocalFree(blob.pbData.cast::<c_void>()) };
        }
        return Err(CredentialProviderError::InvalidSecret);
    }
    let length =
        usize::try_from(blob.cbData).map_err(|_| CredentialProviderError::InvalidSecret)?;
    // SAFETY: DPAPI 成功返回的指针保证至少包含 cbData 字节。
    let bytes = unsafe { std::slice::from_raw_parts_mut(blob.pbData, length) };
    let output = Zeroizing::new(bytes.to_vec());
    bytes.zeroize();
    // SAFETY: 指针来自 DPAPI 分配器，并且只在此释放一次。
    let _ = unsafe { LocalFree(blob.pbData.cast::<c_void>()) };
    Ok(output)
}

fn encode_inner(
    password: &[u8],
    digest: &[u8; 32],
) -> Result<Zeroizing<Vec<u8>>, CredentialProviderError> {
    if password.is_empty() || password.len() > MAX_PASSWORD_BYTES {
        return Err(CredentialProviderError::InvalidSecret);
    }
    let length =
        u32::try_from(password.len()).map_err(|_| CredentialProviderError::InvalidSecret)?;
    let mut value = Zeroizing::new(Vec::with_capacity(
        INNER_HEADER_BYTES + password.len() + INNER_CHECKSUM_BYTES,
    ));
    value.extend_from_slice(INNER_MAGIC);
    value.push(FORMAT_VERSION);
    value.push(MATERIAL_KIND_PASSWORD);
    value.extend_from_slice(digest);
    value.extend_from_slice(&length.to_be_bytes());
    value.extend_from_slice(password);
    let checksum = Sha256::digest(value.as_slice());
    value.extend_from_slice(&checksum);
    Ok(value)
}

fn decode_inner<'a>(
    value: &'a [u8],
    digest: &[u8; 32],
) -> Result<&'a [u8], CredentialProviderError> {
    if value.len() < INNER_HEADER_BYTES + INNER_CHECKSUM_BYTES
        || &value[..4] != INNER_MAGIC
        || value[4] != FORMAT_VERSION
        || value[5] != MATERIAL_KIND_PASSWORD
        || !constant_time_equal(&value[6..38], digest)
    {
        return Err(CredentialProviderError::InvalidSecret);
    }
    let length = u32::from_be_bytes(
        value[38..42]
            .try_into()
            .map_err(|_| CredentialProviderError::InvalidSecret)?,
    ) as usize;
    if length == 0
        || length > MAX_PASSWORD_BYTES
        || value.len() != INNER_HEADER_BYTES + length + INNER_CHECKSUM_BYTES
    {
        return Err(CredentialProviderError::InvalidSecret);
    }
    let checksum_offset = INNER_HEADER_BYTES + length;
    let expected = Sha256::digest(&value[..checksum_offset]);
    if !constant_time_equal(&value[checksum_offset..], &expected) {
        return Err(CredentialProviderError::InvalidSecret);
    }
    Ok(&value[INNER_HEADER_BYTES..checksum_offset])
}

fn encode_envelope(
    ciphertext: &[u8],
    digest: &[u8; 32],
) -> Result<Vec<u8>, CredentialProviderError> {
    let length =
        u32::try_from(ciphertext.len()).map_err(|_| CredentialProviderError::InvalidSecret)?;
    if ciphertext.is_empty() || OUTER_HEADER_BYTES + ciphertext.len() > MAX_ENVELOPE_BYTES {
        return Err(CredentialProviderError::InvalidSecret);
    }
    let mut value = Vec::with_capacity(OUTER_HEADER_BYTES + ciphertext.len());
    value.extend_from_slice(ENVELOPE_MAGIC);
    value.push(FORMAT_VERSION);
    value.push(MATERIAL_KIND_PASSWORD);
    value.extend_from_slice(digest);
    value.extend_from_slice(&length.to_be_bytes());
    value.extend_from_slice(ciphertext);
    Ok(value)
}

fn decode_envelope<'a>(
    value: &'a [u8],
    digest: &[u8; 32],
) -> Result<&'a [u8], CredentialProviderError> {
    if value.len() < OUTER_HEADER_BYTES
        || &value[..4] != ENVELOPE_MAGIC
        || value[4] != FORMAT_VERSION
        || value[5] != MATERIAL_KIND_PASSWORD
        || !constant_time_equal(&value[6..38], digest)
    {
        return Err(CredentialProviderError::InvalidSecret);
    }
    let length = u32::from_be_bytes(
        value[38..42]
            .try_into()
            .map_err(|_| CredentialProviderError::InvalidSecret)?,
    ) as usize;
    if length == 0 || value.len() != OUTER_HEADER_BYTES + length {
        return Err(CredentialProviderError::InvalidSecret);
    }
    Ok(&value[OUTER_HEADER_BYTES..])
}

fn locator_digest(value: &[u8]) -> [u8; 32] {
    Sha256::digest(value).into()
}

fn entropy(digest: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"vigilshell.dpapi.current-user.password.v1");
    hasher.update(digest);
    hasher.finalize().into()
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn path_wide(path: &Path) -> Result<Vec<u16>, CredentialProviderError> {
    let value: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if value[..value.len().saturating_sub(1)].contains(&0) {
        return Err(CredentialProviderError::Unavailable);
    }
    Ok(value)
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_and_inner_payload_reject_tampering_and_wrong_locator() {
        let digest = locator_digest(b"credential-a");
        let other = locator_digest(b"credential-b");
        let inner = encode_inner(b"password", &digest).expect("inner");
        assert_eq!(decode_inner(&inner, &digest).expect("decode"), b"password");
        assert!(decode_inner(&inner, &other).is_err());

        let mut tampered = inner.to_vec();
        tampered[INNER_HEADER_BYTES] ^= 0x01;
        assert!(decode_inner(&tampered, &digest).is_err());

        let envelope = encode_envelope(b"ciphertext", &digest).expect("envelope");
        assert_eq!(
            decode_envelope(&envelope, &digest).expect("decode envelope"),
            b"ciphertext"
        );
        assert!(decode_envelope(&envelope, &other).is_err());
    }

    #[test]
    fn password_payload_enforces_public_size_bounds() {
        let digest = locator_digest(b"credential");
        assert!(encode_inner(&[], &digest).is_err());
        assert!(encode_inner(&vec![7; MAX_PASSWORD_BYTES], &digest).is_ok());
        assert!(encode_inner(&vec![7; MAX_PASSWORD_BYTES + 1], &digest).is_err());
    }
}
