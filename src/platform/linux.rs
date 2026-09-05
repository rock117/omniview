use std::sync::Arc;

use crate::platform::{
    unsupported::{UnsupportedDns, UnsupportedHandle, UnsupportedNet, UnsupportedProcess, UnsupportedProxy},
    PlatformServices,
};

pub fn services() -> PlatformServices {
    PlatformServices {
        processes: Arc::new(UnsupportedProcess {
            capability: "process enumeration (Linux)",
        }),
        net: Arc::new(UnsupportedNet {
            capability: "socket enumeration (Linux)",
        }),
        handles: Arc::new(UnsupportedHandle {
            capability: "file handle enumeration (Linux)",
        }),
        dns: Arc::new(UnsupportedDns {
            capability: "DNS cache (Linux)",
        }),
        proxy: Arc::new(UnsupportedProxy {
            capability: "system proxy control (Linux)",
        }),
    }
}
