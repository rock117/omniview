//! Windows system proxy / WinHTTP / env proxy.

use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Networking::WinHttp::{
    WinHttpGetDefaultProxyConfiguration, WinHttpSetDefaultProxyConfiguration,
    WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_PROXY_INFO,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY_CURRENT_USER, KEY_READ,
    KEY_SET_VALUE, REG_DWORD, REG_SZ,
};

use crate::domain::{EnvProxyInfo, ProbeError, SystemProxyInfo, WinHttpProxyInfo};
use crate::platform::unsupported::read_env_proxy;
use crate::platform::ProxyProbe;

pub struct WindowsProxyProbe;

impl ProxyProbe for WindowsProxyProbe {
    fn system_proxy(&self) -> Result<SystemProxyInfo, ProbeError> {
        read_internet_settings_proxy()
    }

    fn winhttp_proxy(&self) -> Result<WinHttpProxyInfo, ProbeError> {
        read_winhttp_proxy()
    }

    fn env_proxy(&self) -> Result<EnvProxyInfo, ProbeError> {
        Ok(read_env_proxy())
    }

    fn disable_system_proxy(&self) -> Result<(), ProbeError> {
        set_proxy_enable(0)
    }

    fn reset_winhttp_proxy(&self) -> Result<(), ProbeError> {
        unsafe {
            let mut info = WINHTTP_PROXY_INFO {
                dwAccessType: WINHTTP_ACCESS_TYPE_NO_PROXY,
                lpszProxy: windows::core::PWSTR::null(),
                lpszProxyBypass: windows::core::PWSTR::null(),
            };
            WinHttpSetDefaultProxyConfiguration(&mut info).map_err(|e| {
                ProbeError::msg(format!("WinHttpSetDefaultProxyConfiguration: {e}"))
            })?;
        }
        Ok(())
    }
}

fn read_internet_settings_proxy() -> Result<SystemProxyInfo, ProbeError> {
    unsafe {
        let subkey: Vec<u16> =
            "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
        let mut hkey = Default::default();
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            Some(0),
            KEY_READ,
            &mut hkey,
        );
        if status != ERROR_SUCCESS {
            return Err(ProbeError::msg(format!(
                "RegOpenKeyEx Internet Settings: {status:?}"
            )));
        }

        let enabled = reg_query_dword(hkey, "ProxyEnable").unwrap_or(0) != 0;
        let server = reg_query_string(hkey, "ProxyServer").unwrap_or_default();
        let override_list = reg_query_string(hkey, "ProxyOverride").unwrap_or_default();
        let _ = RegCloseKey(hkey);

        Ok(SystemProxyInfo {
            enabled,
            server,
            override_list,
        })
    }
}

fn set_proxy_enable(value: u32) -> Result<(), ProbeError> {
    unsafe {
        let subkey: Vec<u16> =
            "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
        let mut hkey = Default::default();
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            Some(0),
            KEY_SET_VALUE,
            &mut hkey,
        );
        if status != ERROR_SUCCESS {
            return Err(ProbeError::msg(format!(
                "RegOpenKeyEx for write: {status:?}"
            )));
        }
        let name: Vec<u16> = "ProxyEnable"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let bytes = value.to_le_bytes();
        let status = RegSetValueExW(
            hkey,
            PCWSTR(name.as_ptr()),
            Some(0),
            REG_DWORD,
            Some(&bytes),
        );
        let _ = RegCloseKey(hkey);
        if status != ERROR_SUCCESS {
            return Err(ProbeError::msg(format!(
                "RegSetValueEx ProxyEnable: {status:?}"
            )));
        }
    }
    Ok(())
}

fn read_winhttp_proxy() -> Result<WinHttpProxyInfo, ProbeError> {
    unsafe {
        let mut info = WINHTTP_PROXY_INFO::default();
        if let Err(e) = WinHttpGetDefaultProxyConfiguration(&mut info) {
            return Ok(WinHttpProxyInfo {
                available: true,
                summary: format!("unavailable ({e})"),
            });
        }

        let access = info.dwAccessType;
        let proxy = pwstr_owned(info.lpszProxy);
        let bypass = pwstr_owned(info.lpszProxyBypass);

        // Memory for strings is owned by WinHTTP; do not GlobalFree here on all builds.
        let summary = if access == WINHTTP_ACCESS_TYPE_NO_PROXY || proxy.is_empty() {
            "direct".into()
        } else {
            format!("access={access:?} proxy={proxy} bypass={bypass}")
        };

        Ok(WinHttpProxyInfo {
            available: true,
            summary,
        })
    }
}

fn reg_query_dword(
    hkey: windows::Win32::System::Registry::HKEY,
    name: &str,
) -> Option<u32> {
    unsafe {
        let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let mut ty = REG_DWORD;
        let mut data = [0u8; 4];
        let mut size = data.len() as u32;
        let status = RegQueryValueExW(
            hkey,
            PCWSTR(name_w.as_ptr()),
            None,
            Some(&mut ty),
            Some(data.as_mut_ptr()),
            Some(&mut size),
        );
        if status == ERROR_SUCCESS {
            Some(u32::from_le_bytes(data))
        } else {
            None
        }
    }
}

fn reg_query_string(
    hkey: windows::Win32::System::Registry::HKEY,
    name: &str,
) -> Option<String> {
    unsafe {
        let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let mut ty = REG_SZ;
        let mut size = 0u32;
        let status = RegQueryValueExW(
            hkey,
            PCWSTR(name_w.as_ptr()),
            None,
            Some(&mut ty),
            None,
            Some(&mut size),
        );
        if status != ERROR_SUCCESS || size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let status = RegQueryValueExW(
            hkey,
            PCWSTR(name_w.as_ptr()),
            None,
            Some(&mut ty),
            Some(buf.as_mut_ptr()),
            Some(&mut size),
        );
        if status != ERROR_SUCCESS {
            return None;
        }
        let u16s: Vec<u16> = buf
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .filter(|c| *c != 0)
            .collect();
        Some(String::from_utf16_lossy(&u16s))
    }
}

fn pwstr_owned(p: windows::core::PWSTR) -> String {
    unsafe {
        if p.0.is_null() {
            return String::new();
        }
        let mut len = 0usize;
        while *p.0.add(len) != 0 {
            len += 1;
            if len > 4096 {
                break;
            }
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p.0, len))
    }
}
