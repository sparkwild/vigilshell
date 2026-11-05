//! 服务器工具用例命令与结果。
use std::path::PathBuf;
use vigilshell_domain::tools::*;
#[derive(Clone, Debug)]
pub enum ToolAction {
    Edit(crate::file_editing::FileEditCommand),
    Stat {
        session: String,
        path: String,
    },
    CreateFile {
        session: String,
        path: String,
    },
    StartFileOperation {
        session: String,
        paths: Vec<String>,
        operation: String,
        recursive: bool,
        set_mask: u32,
        clear_mask: u32,
    },
    FileOperation {
        id: String,
        offset: u32,
    },
    RetryFileOperation {
        id: String,
    },
    Subscribe {
        group: String,
        visible: bool,
    },
    Refresh {
        session: String,
    },
    List {
        session: String,
        path: String,
        cursor: Option<String>,
        request: String,
    },
    ReleaseList {
        request: String,
    },
    Identities {
        session: String,
        uids: Vec<u32>,
        gids: Vec<u32>,
    },
    CancelList {
        request: String,
    },
    CreateDirectory {
        session: String,
        path: String,
    },
    Rename {
        session: String,
        from: String,
        to: String,
    },
    Remove {
        session: String,
        path: String,
        directory: bool,
    },
    Grant {
        direction: String,
        paths: Vec<PathBuf>,
    },
    Transfer {
        session: String,
        grant: String,
        paths: Vec<String>,
        overwrite: bool,
    },
    Archive {
        session: String,
        grant: String,
        paths: Vec<String>,
    },
    TransferItems {
        id: String,
        offset: u32,
    },
    CancelTransfer {
        id: String,
    },
    RetryTransfer {
        id: String,
    },
    Transfers,
    TransferPage {
        request: Option<String>,
        cursor: Option<String>,
    },
    ResolveConflict {
        id: String,
        version: u32,
        decision: String,
        batch: bool,
    },
}
#[derive(Clone, Debug)]
pub enum ToolResult {
    FileEdit(FileEditRemote),
    FileEntry(RemoteEntry),
    FileOperation(FileOperationPage),
    Snapshot(Box<ToolSnapshot>),
    Identities(Vec<IdentityName>),
    Directory(DirectoryPage),
    Grant(FileGrantView),
    TransferItems(TransferItemsPage),
    Transfers(Vec<TransferView>),
    TransferPage {
        request: String,
        tasks: Vec<TransferView>,
        next: Option<String>,
    },
    Done,
}
