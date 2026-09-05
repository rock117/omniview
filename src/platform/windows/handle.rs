//! Windows file-handle occupancy via `NtQuerySystemInformation` + timed `NtQueryObject`.
//!
//! `NtQueryObject` can hang on pipes/devices; name queries run on a worker with a short timeout.

use std::collections::HashMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use windows::core::{BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE,
};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, SE_DEBUG_NAME, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY, LUID_AND_ATTRIBUTES,
};
use windows::Win32::Storage::FileSystem::QueryDosDeviceW;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_DUP_HANDLE,
    PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::domain::{OpenFile, PathHolder, Pid, ProbeError};
use crate::platform::HandleProbe;

const SYSTEM_EXTENDED_HANDLE_INFORMATION: i32 = 64;
const OBJECT_NAME_INFORMATION: i32 = 1;
const OBJECT_TYPE_INFORMATION: i32 = 2;
const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC0000004u32 as i32;
const STATUS_SUCCESS: i32 = 0;
const NAME_QUERY_TIMEOUT_MS: u64 = 80;
const MAX_RESOLVE_PER_SCAN: usize = 8_000;

#[repr(C)]
struct SystemHandleTableEntryInfoEx {
    object: *mut c_void,
    unique_process_id: usize,
    handle_value: usize,
    granted_access: u32,
    creator_back_trace_index: u16,
    object_type_index: u16,
    handle_attributes: u32,
    reserved: u32,
}

#[repr(C)]
struct SystemHandleInformationEx {
    number_of_handles: usize,
    reserved: usize,
    // followed by handles[]
}

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}

#[repr(C)]
struct ObjectNameInformation {
    name: UnicodeString,
}

type NtQuerySystemInformationFn = unsafe extern "system" fn(
    system_information_class: i32,
    system_information: *mut c_void,
    system_information_length: u32,
    return_length: *mut u32,
) -> i32;

type NtQueryObjectFn = unsafe extern "system" fn(
    handle: HANDLE,
    object_information_class: i32,
    object_information: *mut c_void,
    object_information_length: u32,
    return_length: *mut u32,
) -> i32;

struct Ntdll {
    query_system: NtQuerySystemInformationFn,
    query_object: NtQueryObjectFn,
}

fn ntdll() -> Result<&'static Ntdll, ProbeError> {
    static NTDLL: OnceLock<Result<Ntdll, String>> = OnceLock::new();
    match NTDLL.get_or_init(|| unsafe {
        let name: Vec<u16> = "ntdll.dll"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let module = LoadLibraryW(PCWSTR(name.as_ptr())).map_err(|e| e.to_string())?;
        let qs = GetProcAddress(module, windows::core::PCSTR(b"NtQuerySystemInformation\0".as_ptr()))
            .ok_or_else(|| "NtQuerySystemInformation missing".to_string())?;
        let qo = GetProcAddress(module, windows::core::PCSTR(b"NtQueryObject\0".as_ptr()))
            .ok_or_else(|| "NtQueryObject missing".to_string())?;
        Ok(Ntdll {
            query_system: std::mem::transmute(qs),
            query_object: std::mem::transmute(qo),
        })
    }) {
        Ok(api) => Ok(api),
        Err(e) => Err(ProbeError::msg(e.clone())),
    }
}

pub struct WindowsHandleProbe;

impl HandleProbe for WindowsHandleProbe {
    fn open_files(&self, pid: Pid) -> Result<Vec<OpenFile>, ProbeError> {
        enable_debug_privilege();
        let api = ntdll()?;
        let table = query_handle_table(api)?;
        let mut out = Vec::new();
        let mut resolved = 0usize;

        for entry in table {
            if entry.unique_process_id as u32 != pid {
                continue;
            }
            if resolved >= MAX_RESOLVE_PER_SCAN {
                break;
            }
            resolved += 1;
            if let Some(path) = resolve_file_path(api, pid, entry.handle_value as u16) {
                out.push(OpenFile {
                    path,
                    access: Some(format!("0x{:x}", entry.granted_access)),
                });
            }
        }

        out.sort_by(|a, b| a.path.cmp(&b.path));
        out.dedup_by(|a, b| a.path == b.path);
        Ok(out)
    }

    fn holders_of_path(&self, path: &Path) -> Result<Vec<PathHolder>, ProbeError> {
        enable_debug_privilege();
        let api = ntdll()?;
        let query = normalize_query_path(path);
        if query.is_empty() {
            return Err(ProbeError::msg("empty path"));
        }

        let table = query_handle_table(api)?;
        let mut out = Vec::new();
        let mut resolved = 0usize;
        let mut open_cache: HashMap<u32, HANDLE> = HashMap::new();

        for entry in table {
            if resolved >= MAX_RESOLVE_PER_SCAN {
                break;
            }
            let pid = entry.unique_process_id as u32;
            if pid == 0 {
                continue;
            }
            resolved += 1;

            let Some(nt_path) =
                resolve_file_path_cached(api, pid, entry.handle_value as u16, &mut open_cache)
            else {
                continue;
            };
            let win_path = nt_to_win32(&nt_path).unwrap_or_else(|| nt_path.clone());
            if path_matches(&win_path, &nt_path, &query) {
                out.push(PathHolder {
                    pid,
                    path: win_path,
                    access: Some(format!("0x{:x}", entry.granted_access)),
                });
            }
        }

        for (_, h) in open_cache {
            unsafe {
                let _ = CloseHandle(h);
            }
        }

        out.sort_by(|a, b| a.pid.cmp(&b.pid).then(a.path.cmp(&b.path)));
        out.dedup_by(|a, b| a.pid == b.pid && a.path == b.path);
        Ok(out)
    }
}

fn query_handle_table(api: &Ntdll) -> Result<Vec<SystemHandleTableEntryInfoEx>, ProbeError> {
    let mut size: u32 = 1 << 20;
    for _ in 0..12 {
        let mut buf = vec![0u8; size as usize];
        let mut ret = 0u32;
        let status = unsafe {
            (api.query_system)(
                SYSTEM_EXTENDED_HANDLE_INFORMATION,
                buf.as_mut_ptr().cast(),
                size,
                &mut ret,
            )
        };
        if status == STATUS_INFO_LENGTH_MISMATCH {
            size = ret.max(size.saturating_mul(2)).max(size + (1 << 20));
            continue;
        }
        if status != STATUS_SUCCESS {
            return Err(ProbeError::msg(format!(
                "NtQuerySystemInformation failed: 0x{status:08x} (try Run as administrator)"
            )));
        }
        if buf.len() < std::mem::size_of::<SystemHandleInformationEx>() {
            return Ok(Vec::new());
        }
        let header = unsafe { &*(buf.as_ptr() as *const SystemHandleInformationEx) };
        let count = header.number_of_handles;
        let entry_size = std::mem::size_of::<SystemHandleTableEntryInfoEx>();
        let max = (buf.len() - std::mem::size_of::<SystemHandleInformationEx>()) / entry_size;
        let count = count.min(max);
        let base = unsafe {
            buf.as_ptr()
                .add(std::mem::size_of::<SystemHandleInformationEx>())
                as *const SystemHandleTableEntryInfoEx
        };
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            out.push(unsafe { std::ptr::read(base.add(i)) });
        }
        return Ok(out);
    }
    Err(ProbeError::msg(
        "handle table buffer grew too large; aborting scan",
    ))
}

fn resolve_file_path(api: &Ntdll, pid: u32, handle_value: u16) -> Option<String> {
    let mut cache = HashMap::new();
    let path = resolve_file_path_cached(api, pid, handle_value, &mut cache);
    for (_, h) in cache {
        unsafe {
            let _ = CloseHandle(h);
        }
    }
    path.and_then(|nt| nt_to_win32(&nt).or(Some(nt)))
}

fn resolve_file_path_cached(
    api: &Ntdll,
    pid: u32,
    handle_value: u16,
    open_cache: &mut HashMap<u32, HANDLE>,
) -> Option<String> {
    let process = if let Some(h) = open_cache.get(&pid) {
        *h
    } else {
        let h = open_process_for_dup(pid)?;
        open_cache.insert(pid, h);
        h
    };

    let mut dup = HANDLE::default();
    let ok = unsafe {
        DuplicateHandle(
            process,
            HANDLE(handle_value as *mut c_void),
            GetCurrentProcess(),
            &mut dup,
            0,
            false,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if ok.is_err() {
        return None;
    }

    let type_name = query_object_type_timed(api, dup)?;
    if !type_name.eq_ignore_ascii_case("File") {
        unsafe {
            let _ = CloseHandle(dup);
        }
        return None;
    }

    let name = query_object_name_timed(api, dup);
    unsafe {
        let _ = CloseHandle(dup);
    }
    let name = name?;
    if name.is_empty() {
        return None;
    }
    Some(name)
}

fn open_process_for_dup(pid: u32) -> Option<HANDLE> {
    unsafe {
        OpenProcess(
            PROCESS_DUP_HANDLE | PROCESS_QUERY_INFORMATION,
            false,
            pid,
        )
        .or_else(|_| OpenProcess(PROCESS_DUP_HANDLE | PROCESS_QUERY_LIMITED_INFORMATION, false, pid))
        .ok()
    }
}

fn query_object_name_timed(api: &Ntdll, handle: HANDLE) -> Option<String> {
    query_object_string_timed(api, handle, OBJECT_NAME_INFORMATION)
}

fn query_object_type_timed(api: &Ntdll, handle: HANDLE) -> Option<String> {
    // ObjectTypeInformation starts with UNICODE_STRING TypeName
    query_object_string_timed(api, handle, OBJECT_TYPE_INFORMATION)
}

fn query_object_string_timed(api: &Ntdll, handle: HANDLE, class: i32) -> Option<String> {
    let (tx, rx) = flume::bounded::<Option<String>>(1);
    let handle_bits = handle.0 as usize;
    // SAFETY: `api` is 'static via OnceLock
    let query_object = api.query_object;
    std::thread::spawn(move || {
        let h = HANDLE(handle_bits as *mut c_void);
        let _ = tx.send(query_object_string(query_object, h, class));
    });
    rx.recv_timeout(Duration::from_millis(NAME_QUERY_TIMEOUT_MS))
        .ok()
        .flatten()
}

fn query_object_string(
    query_object: NtQueryObjectFn,
    handle: HANDLE,
    class: i32,
) -> Option<String> {
    unsafe {
        let mut size = 0u32;
        let mut buf = vec![0u8; 1024];
        let mut status = query_object(
            handle,
            class,
            buf.as_mut_ptr().cast(),
            buf.len() as u32,
            &mut size,
        );
        if status == STATUS_INFO_LENGTH_MISMATCH && size > 0 {
            buf.resize(size as usize, 0);
            status = query_object(
                handle,
                class,
                buf.as_mut_ptr().cast(),
                buf.len() as u32,
                &mut size,
            );
        }
        if status != STATUS_SUCCESS {
            return None;
        }
        // Both ObjectNameInformation and ObjectTypeInformation begin with UNICODE_STRING
        let uni = &*(buf.as_ptr() as *const UnicodeString);
        if uni.buffer.is_null() || uni.length == 0 {
            return Some(String::new());
        }
        let len = (uni.length / 2) as usize;
        let slice = std::slice::from_raw_parts(uni.buffer, len);
        Some(String::from_utf16_lossy(slice))
    }
}

fn normalize_query_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    let s = s.trim().trim_matches('"');
    let mut s = s.replace('/', "\\");
    while s.ends_with('\\') && s.len() > 3 {
        s.pop();
    }
    s.to_lowercase()
}

fn path_matches(win_path: &str, nt_path: &str, query: &str) -> bool {
    let w = win_path.replace('/', "\\").to_lowercase();
    let n = nt_path.replace('/', "\\").to_lowercase();
    let q = query;
    if q.is_empty() {
        return false;
    }
    w == q
        || w.starts_with(&(q.to_string() + "\\"))
        || n.ends_with(q)
        || n.contains(&(q.replace(':', "")))
        || (q.len() > 3 && (w.contains(q) || n.to_lowercase().contains(q)))
}

fn nt_to_win32(nt: &str) -> Option<String> {
    let nt = nt.trim();
    if nt.len() >= 2 && nt.as_bytes().get(1) == Some(&b':') {
        return Some(nt.to_string());
    }
    if !nt.starts_with("\\Device\\") {
        return None;
    }
    let nt_lower = nt.to_lowercase();
    for (device, letter) in drive_device_map() {
        let device_lower = device.to_lowercase();
        if nt_lower == device_lower {
            return Some(format!("{letter}:\\"));
        }
        let prefix = format!("{device_lower}\\");
        if let Some(rest) = nt_lower.strip_prefix(&prefix) {
            // Preserve original casing from nt after prefix length
            let rest_orig = &nt[prefix.len()..];
            let _ = rest;
            return Some(format!("{letter}:\\{rest_orig}"));
        }
    }
    Some(nt.to_string())
}

fn drive_device_map() -> Vec<(String, char)> {
    let mut out = Vec::new();
    for letter in b'A'..=b'Z' {
        let drive = format!("{}:", letter as char);
        let wide: Vec<u16> = drive.encode_utf16().chain(std::iter::once(0)).collect();
        let mut buf = vec![0u16; 512];
        let n = unsafe { QueryDosDeviceW(PCWSTR(wide.as_ptr()), Some(&mut buf)) };
        if n == 0 {
            continue;
        }
        let device = {
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            String::from_utf16_lossy(&buf[..len])
        };
        if !device.is_empty() {
            out.push((device, letter as char));
        }
    }
    out
}

fn enable_debug_privilege() {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
        .is_err()
        {
            return;
        }
        let mut luid = Default::default();
        if LookupPrivilegeValueW(PCWSTR::null(), SE_DEBUG_NAME, &mut luid).is_err() {
            let _ = CloseHandle(token);
            return;
        }
        let mut tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        let _ = AdjustTokenPrivileges(token, false, Some(&mut tp), 0, None, None);
        let _ = CloseHandle(token);
    }
}

#[allow(dead_code)]
fn _unused() -> (BOOL, PWSTR) {
    (BOOL(0), PWSTR::null())
}
