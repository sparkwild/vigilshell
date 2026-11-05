use std::path::PathBuf;
#[derive(Clone, Debug)]
pub enum FileEditCommand {
    Open {
        id: String,
        lease_owner: String,
        session: String,
        path: String,
        local: PathBuf,
    },
    Inspect {
        id: String,
    },
    Upload {
        id: String,
        local: PathBuf,
        expected: vigilshell_domain::tools::RemoteFileVersion,
        destination: Option<String>,
    },
    Reload {
        id: String,
        local: PathBuf,
    },
    Close {
        id: String,
    },
}
