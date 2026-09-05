//! OS-agnostic domain types and helpers.

mod dns;
mod handle;
mod net;
mod process;
mod proxy;
mod settings;

pub use dns::*;
pub use handle::*;
pub use net::*;
pub use process::*;
pub use proxy::*;
pub use settings::*;

/// Process identifier (host OS PID).
pub type Pid = u32;

#[derive(Debug, Clone, thiserror::Error)]
pub enum ProbeError {
    #[error("{capability} is not supported on this platform yet")]
    Unsupported { capability: &'static str },
    #[error("administrator / elevated privileges required: {0}")]
    PrivilegeRequired(String),
    #[error("{0}")]
    Message(String),
}

impl ProbeError {
    pub fn unsupported(capability: &'static str) -> Self {
        Self::Unsupported { capability }
    }

    pub fn msg(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }
}
