mod dns;
mod handle;
mod net;
mod process;
mod proxy;

use std::sync::Arc;

use crate::platform::PlatformServices;

pub fn services() -> PlatformServices {
    PlatformServices {
        processes: Arc::new(process::SysinfoProcessProbe::new()),
        net: Arc::new(net::WindowsNetProbe),
        handles: Arc::new(handle::WindowsHandleProbe),
        dns: Arc::new(dns::WindowsDnsCacheProbe),
        proxy: Arc::new(proxy::WindowsProxyProbe),
    }
}
