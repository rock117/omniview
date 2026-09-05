use crate::domain::Pid;

#[derive(Debug, Clone)]
pub struct OpenFile {
    pub path: String,
    pub access: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PathHolder {
    pub pid: Pid,
    pub path: String,
    pub access: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FileSearchSnapshot {
    pub query: String,
    pub holders: Vec<PathHolder>,
    pub error: Option<String>,
    pub busy: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ProcessFilesSnapshot {
    pub pid: Option<Pid>,
    pub files: Vec<OpenFile>,
    pub error: Option<String>,
    pub busy: bool,
}
