//! Windows DNS client cache via dnsapi.dll (undocumented list API + flush).

use std::ffi::c_void;
use std::ptr;

use windows::core::{BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::FreeLibrary;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

use crate::domain::{DnsCacheEntry, ProbeError};
use crate::platform::DnsCacheProbe;

#[repr(C)]
struct DnsCacheEntryRaw {
    p_next: *mut DnsCacheEntryRaw,
    psz_name: PWSTR,
    w_type: u16,
    w_data_length: u16,
    flags: u32,
}

type DnsGetCacheDataTableFn = unsafe extern "system" fn(*mut *mut DnsCacheEntryRaw) -> BOOL;
type DnsFlushResolverCacheFn = unsafe extern "system" fn() -> BOOL;
type DnsFlushResolverCacheEntryFn = unsafe extern "system" fn(PCWSTR) -> BOOL;

pub struct WindowsDnsCacheProbe;

impl DnsCacheProbe for WindowsDnsCacheProbe {
    fn list_cache(&self) -> Result<Vec<DnsCacheEntry>, ProbeError> {
        let api = DnsApi::load()?;
        let get = api
            .get_cache
            .ok_or_else(|| ProbeError::msg("DnsGetCacheDataTable not found in dnsapi.dll"))?;

        unsafe {
            let mut table: *mut DnsCacheEntryRaw = ptr::null_mut();
            if !get(&mut table).as_bool() || table.is_null() {
                return Ok(Vec::new());
            }

            let mut out = Vec::new();
            let mut cur = table;
            while !cur.is_null() {
                let entry = &*cur;
                let name = pwstr_to_string(entry.psz_name);
                let record_type = dns_type_name(entry.w_type).to_string();
                out.push(DnsCacheEntry {
                    name,
                    record_type,
                    data: String::new(),
                });
                cur = entry.p_next;
            }

            if let Some(free) = api.dns_api_free {
                free(table as *mut c_void, 1);
            }

            Ok(out)
        }
    }

    fn flush_cache(&self) -> Result<(), ProbeError> {
        let api = DnsApi::load()?;
        let flush = api
            .flush
            .ok_or_else(|| ProbeError::msg("DnsFlushResolverCache not found"))?;
        unsafe {
            if !flush().as_bool() {
                return Err(ProbeError::msg(
                    "DnsFlushResolverCache failed (may need elevation)",
                ));
            }
        }
        Ok(())
    }

    fn remove_entry(&self, name: &str) -> Result<(), ProbeError> {
        let api = DnsApi::load()?;
        let flush_entry = api
            .flush_entry
            .ok_or_else(|| ProbeError::msg("DnsFlushResolverCacheEntry not found"))?;
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            if !flush_entry(PCWSTR(wide.as_ptr())).as_bool() {
                return Err(ProbeError::msg(format!(
                    "failed to remove DNS cache entry for {name}"
                )));
            }
        }
        Ok(())
    }

    fn supports_remove_entry(&self) -> bool {
        true
    }
}

struct DnsApi {
    _module: windows::Win32::Foundation::HMODULE,
    get_cache: Option<DnsGetCacheDataTableFn>,
    flush: Option<DnsFlushResolverCacheFn>,
    flush_entry: Option<DnsFlushResolverCacheEntryFn>,
    dns_api_free: Option<unsafe extern "system" fn(*mut c_void, u32)>,
}

impl DnsApi {
    fn load() -> Result<Self, ProbeError> {
        unsafe {
            let name: Vec<u16> = "dnsapi.dll"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let module = LoadLibraryW(PCWSTR(name.as_ptr()))
                .map_err(|e| ProbeError::msg(format!("LoadLibrary dnsapi.dll: {e}")))?;

            let get_cache = get_proc::<DnsGetCacheDataTableFn>(module, b"DnsGetCacheDataTable\0");
            let flush = get_proc::<DnsFlushResolverCacheFn>(module, b"DnsFlushResolverCache\0");
            let flush_entry =
                get_proc::<DnsFlushResolverCacheEntryFn>(module, b"DnsFlushResolverCacheEntry_W\0");
            let dns_api_free =
                get_proc::<unsafe extern "system" fn(*mut c_void, u32)>(module, b"DnsApiFree\0");

            Ok(Self {
                _module: module,
                get_cache,
                flush,
                flush_entry,
                dns_api_free,
            })
        }
    }
}

impl Drop for DnsApi {
    fn drop(&mut self) {
        unsafe {
            let _ = FreeLibrary(self._module);
        }
    }
}

unsafe fn get_proc<T>(
    module: windows::Win32::Foundation::HMODULE,
    name: &[u8],
) -> Option<T> {
    unsafe {
        let p = GetProcAddress(module, windows::core::PCSTR(name.as_ptr()));
        p.map(|f| std::mem::transmute_copy(&f))
    }
}

fn pwstr_to_string(p: PWSTR) -> String {
    unsafe {
        if p.0.is_null() {
            return String::new();
        }
        let mut len = 0usize;
        while *p.0.add(len) != 0 {
            len += 1;
            if len > 2048 {
                break;
            }
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p.0, len))
    }
}

fn dns_type_name(t: u16) -> &'static str {
    match t {
        1 => "A",
        2 => "NS",
        5 => "CNAME",
        6 => "SOA",
        12 => "PTR",
        15 => "MX",
        16 => "TXT",
        28 => "AAAA",
        33 => "SRV",
        255 => "ANY",
        _ => "OTHER",
    }
}
