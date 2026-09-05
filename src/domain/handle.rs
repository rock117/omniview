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
