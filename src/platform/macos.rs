use std::sync::Arc;

use crate::platform::{
    unsupported::{UnsupportedDns, UnsupportedHandle, UnsupportedNet, UnsupportedProcess, UnsupportedProxy},
    PlatformServices,
};

pub fn services() -> PlatformServices {
    PlatformServices {
        processes: Arc::new(UnsupportedProcess {
            capability: "process enumeration (macOS)",
        }),
        net: Arc::new(UnsupportedNet {
            capability: "socket enumeration (macOS)",
        }),
        handles: Arc::new(UnsupportedHandle {
            capability: "file handle enumeration (macOS)",
        }),
        dns: Arc::new(UnsupportedDns {
            capability: "DNS cache (macOS)",
        }),
        proxy: Arc::new(UnsupportedProxy {
            capability: "system proxy control (macOS)",
        }),
    }
}
