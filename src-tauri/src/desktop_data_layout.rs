use std::{
    env, fmt, fs, io,
    path::{Path, PathBuf},
};

pub(crate) const DESKTOP_DATA_DIR_ENV: &str = "VIGILSHELL_DESKTOP_DATA_DIR";
pub(crate) const LEGACY_DEBUG_DATABASE_PATH_ENV: &str = "VIGILSHELL_DEBUG_DATABASE_PATH";
pub(crate) const SINGLE_INSTANCE_LOCK_FILE_NAME: &str = "vigilshell-single-instance.lock";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesktopEnvironment {
    Production,
    Development,
}

impl DesktopEnvironment {
    const fn database_file_name(self) -> &'static str {
        match self {
            Self::Production => "vigilshell.sqlite3",
            Self::Development => "vigilshell-debug.sqlite3",
        }
    }

    const fn secure_namespace(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Development => "development",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesktopDataLayoutSource {
    DefaultAppLocalData,
    TestOverride,
}

/// Desktop 启动阶段一次性解析并验证的私有数据布局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DesktopDataLayout {
    authority_root: PathBuf,
    database_path: PathBuf,
    secure_credentials_root: Option<PathBuf>,
    environment: DesktopEnvironment,
    source: DesktopDataLayoutSource,
}

impl DesktopDataLayout {
    pub(crate) fn resolve(default_root: PathBuf) -> Result<Self, DesktopDataLayoutError> {
        if env::var_os(LEGACY_DEBUG_DATABASE_PATH_ENV).is_some() {
            return Err(DesktopDataLayoutError::LegacyOverrideRejected);
        }
        let environment = if cfg!(debug_assertions) || cfg!(feature = "desktop-test-support") {
            DesktopEnvironment::Development
        } else {
            DesktopEnvironment::Production
        };
        let (root, source) = match env::var_os(DESKTOP_DATA_DIR_ENV) {
            Some(value) if value.is_empty() => {
                return Err(DesktopDataLayoutError::InvalidOverride);
            }
            Some(value) if environment == DesktopEnvironment::Development => {
                (PathBuf::from(value), DesktopDataLayoutSource::TestOverride)
            }
            Some(_) => return Err(DesktopDataLayoutError::ProductionOverrideRejected),
            None => (default_root, DesktopDataLayoutSource::DefaultAppLocalData),
        };
        Self::from_root(root, environment, source)
    }

    fn from_root(
        root: PathBuf,
        environment: DesktopEnvironment,
        source: DesktopDataLayoutSource,
    ) -> Result<Self, DesktopDataLayoutError> {
        if !root.is_absolute() {
            return Err(DesktopDataLayoutError::RootNotAbsolute);
        }
        let secure_credentials_root = cfg!(windows).then(|| {
            root.join("secure")
                .join(environment.secure_namespace())
                .join("credentials")
                .join("v1")
        });
        let layout = Self {
            database_path: root.join(environment.database_file_name()),
            authority_root: root,
            secure_credentials_root,
            environment,
            source,
        };
        layout.prepare_authority_root()?;
        Ok(layout)
    }

    fn prepare_authority_root(&self) -> Result<(), DesktopDataLayoutError> {
        create_owned_directory(&self.authority_root)?;
        validate_existing_regular_file(&self.database_path)
    }

    pub(crate) fn prepare_primary(&self) -> Result<(), DesktopDataLayoutError> {
        if let Some(root) = &self.secure_credentials_root {
            create_owned_directory(root)?;
        }
        prepare_owned_regular_file(&self.database_path)
    }

    pub(crate) fn harden_sqlite_sidecars(&self) -> Result<(), DesktopDataLayoutError> {
        for suffix in ["-wal", "-shm"] {
            let path = PathBuf::from(format!("{}{}", self.database_path.display(), suffix));
            harden_existing_regular_file(&path)?;
        }
        Ok(())
    }

    pub(crate) fn authority_root(&self) -> &Path {
        &self.authority_root
    }

    pub(crate) fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub(crate) fn secure_credentials_root(&self) -> Option<&Path> {
        self.secure_credentials_root.as_deref()
    }

    #[cfg(test)]
    pub(crate) const fn environment(&self) -> DesktopEnvironment {
        self.environment
    }

    #[cfg(test)]
    pub(crate) const fn source(&self) -> DesktopDataLayoutSource {
        self.source
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesktopDataLayoutError {
    LegacyOverrideRejected,
    ProductionOverrideRejected,
    InvalidOverride,
    RootNotAbsolute,
    UnsafeFilesystemEntry,
    FilesystemUnavailable,
}

impl fmt::Display for DesktopDataLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::LegacyOverrideRejected => "desktop_legacy_database_override_rejected",
            Self::ProductionOverrideRejected => "desktop_data_override_rejected",
            Self::InvalidOverride => "desktop_data_override_invalid",
            Self::RootNotAbsolute => "desktop_data_root_not_absolute",
            Self::UnsafeFilesystemEntry => "desktop_data_filesystem_entry_unsafe",
            Self::FilesystemUnavailable => "desktop_data_filesystem_unavailable",
        })
    }
}

impl std::error::Error for DesktopDataLayoutError {}

fn create_owned_directory(path: &Path) -> Result<(), DesktopDataLayoutError> {
    fs::create_dir_all(path).map_err(map_filesystem_error)?;
    let metadata = fs::symlink_metadata(path).map_err(map_filesystem_error)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() || is_reparse(&metadata)
    {
        return Err(DesktopDataLayoutError::UnsafeFilesystemEntry);
    }
    set_private_permissions(path, 0o700)
}

fn prepare_owned_regular_file(path: &Path) -> Result<(), DesktopDataLayoutError> {
    match fs::symlink_metadata(path) {
        Ok(_) => harden_existing_regular_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(map_filesystem_error)?;
            harden_existing_regular_file(path)
        }
        Err(error) => Err(map_filesystem_error(error)),
    }
}

fn validate_existing_regular_file(path: &Path) -> Result<(), DesktopDataLayoutError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(map_filesystem_error(error)),
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() || is_reparse(&metadata)
    {
        return Err(DesktopDataLayoutError::UnsafeFilesystemEntry);
    }
    Ok(())
}

fn harden_existing_regular_file(path: &Path) -> Result<(), DesktopDataLayoutError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(map_filesystem_error(error)),
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() || is_reparse(&metadata)
    {
        return Err(DesktopDataLayoutError::UnsafeFilesystemEntry);
    }
    set_private_permissions(path, 0o600)
}

#[cfg(unix)]
fn set_private_permissions(path: &Path, mode: u32) -> Result<(), DesktopDataLayoutError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(map_filesystem_error)
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path, _mode: u32) -> Result<(), DesktopDataLayoutError> {
    Ok(())
}

#[cfg(windows)]
fn is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
const fn is_reparse(_metadata: &fs::Metadata) -> bool {
    false
}

fn map_filesystem_error(_error: io::Error) -> DesktopDataLayoutError {
    DesktopDataLayoutError::FilesystemUnavailable
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("vigilshell-layout-{name}-{}", std::process::id()))
    }

    #[test]
    fn production_and_development_use_distinct_database_and_secure_namespaces() {
        let production_root = root("production");
        let production = DesktopDataLayout::from_root(
            production_root.clone(),
            DesktopEnvironment::Production,
            DesktopDataLayoutSource::TestOverride,
        )
        .expect("production layout");
        assert_eq!(
            production.database_path(),
            production_root.join("vigilshell.sqlite3")
        );
        assert!(!production.database_path().exists());
        production.prepare_primary().expect("prepare production");
        assert!(production.database_path().is_file());

        let development_root = root("development");
        let development = DesktopDataLayout::from_root(
            development_root.clone(),
            DesktopEnvironment::Development,
            DesktopDataLayoutSource::TestOverride,
        )
        .expect("development layout");
        assert_eq!(
            development.database_path(),
            development_root.join("vigilshell-debug.sqlite3")
        );
        assert_eq!(development.environment(), DesktopEnvironment::Development);
        assert_eq!(development.source(), DesktopDataLayoutSource::TestOverride);
        development.prepare_primary().expect("prepare development");

        let _ = fs::remove_dir_all(production_root);
        let _ = fs::remove_dir_all(development_root);
    }

    #[cfg(unix)]
    #[test]
    fn primary_layout_applies_private_unix_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let root = root("permissions");
        let layout = DesktopDataLayout::from_root(
            root.clone(),
            DesktopEnvironment::Development,
            DesktopDataLayoutSource::TestOverride,
        )
        .expect("layout");
        layout.prepare_primary().expect("prepare primary");
        assert_eq!(
            fs::metadata(&root)
                .expect("root metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(layout.database_path())
                .expect("database metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn relative_root_and_existing_database_directory_are_rejected() {
        assert_eq!(
            DesktopDataLayout::from_root(
                PathBuf::from("relative"),
                DesktopEnvironment::Development,
                DesktopDataLayoutSource::TestOverride,
            ),
            Err(DesktopDataLayoutError::RootNotAbsolute)
        );

        let root = root("database-directory");
        fs::create_dir_all(root.join("vigilshell-debug.sqlite3")).expect("database directory");
        assert_eq!(
            DesktopDataLayout::from_root(
                root.clone(),
                DesktopEnvironment::Development,
                DesktopDataLayoutSource::TestOverride,
            ),
            Err(DesktopDataLayoutError::UnsafeFilesystemEntry)
        );
        let _ = fs::remove_dir_all(root);
    }
}
