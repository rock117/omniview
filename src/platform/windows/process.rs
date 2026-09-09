use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use sysinfo::{ProcessesToUpdate, System, UpdateKind};

use crate::domain::{Pid, ProbeError, ProcessInfo};
use crate::platform::ProcessProbe;

/// Process probe backed by `sysinfo` (portable core; used on Windows first).
pub struct SysinfoProcessProbe {
    system: Mutex<System>,
}

/// Cache PE FileDescription by exe path (None = looked up, missing).
static DISPLAY_NAME_CACHE: LazyLock<Mutex<HashMap<String, Option<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Cache service ImagePath by service key name.
static SERVICE_BIN_CACHE: LazyLock<Mutex<HashMap<String, Option<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone)]
struct RunningService {
    name: String,
    display_name: String,
}

impl SysinfoProcessProbe {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_all();
        // Prime CPU deltas.
        std::thread::sleep(std::time::Duration::from_millis(50));
        system.refresh_processes(ProcessesToUpdate::All, true);
        Self {
            system: Mutex::new(system),
        }
    }
}

impl ProcessProbe for SysinfoProcessProbe {
    fn list_processes(&self) -> Result<Vec<ProcessInfo>, ProbeError> {
        let mut system = self
            .system
            .lock()
            .map_err(|_| ProbeError::msg("process probe lock poisoned"))?;

        system.refresh_cpu_usage();
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            sysinfo::ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .with_exe(UpdateKind::OnlyIfNotSet)
                .with_user(UpdateKind::OnlyIfNotSet),
        );

        // Protected processes (e.g. MsMpEng) often hide their path from OpenProcess;
        // SCM still exposes PID + ImagePath, which Task Manager uses for FileDescription.
        let services_by_pid = enumerate_services_by_pid();

        let mut out = Vec::with_capacity(system.processes().len());
        for (pid, proc_) in system.processes() {
            let pid_u = pid.as_u32();
            let parent = proc_.parent().map(|p| p.as_u32());
            let cmd = {
                let c = proc_.cmd();
                if c.is_empty() {
                    None
                } else {
                    Some(
                        c.iter()
                            .map(|s| s.to_string_lossy())
                            .collect::<Vec<_>>()
                            .join(" "),
                    )
                }
            };
            let name = proc_.name().to_string_lossy().into_owned();
            let mut exe = proc_.exe().map(|p| p.to_string_lossy().into_owned());
            let svc_list = services_by_pid.get(&pid_u);
            if exe.is_none() {
                if let Some(svcs) = svc_list {
                    exe = resolve_exe_from_services(&name, svcs);
                }
            }
            let service_names: Vec<String> = svc_list
                .map(|svcs| {
                    svcs.iter()
                        .map(|s| s.display_name.clone())
                        .filter(|d| !d.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let display_name = exe
                .as_deref()
                .and_then(cached_file_description)
                .or_else(|| {
                    // Dedicated service process without PE description: use SCM name.
                    (service_names.len() == 1).then(|| service_names[0].clone())
                });
            let user = proc_.user_id().map(|u| u.to_string());
            out.push(ProcessInfo {
                pid: pid_u,
                parent_pid: parent,
                name,
                display_name,
                service_names,
                exe_path: exe,
                cmd_line: cmd,
                user,
                cpu_percent: proc_.cpu_usage(),
                memory_bytes: proc_.memory(),
            });
        }
        Ok(out)
    }

    fn kill(&self, pid: Pid, tree: bool) -> Result<(), ProbeError> {
        if tree {
            kill_tree(pid)
        } else {
            kill_one(pid)
        }
    }
}

fn resolve_exe_from_services(image_name: &str, svcs: &[RunningService]) -> Option<String> {
    let image = Path::new(image_name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(image_name);
    for svc in svcs {
        if let Some(path) = cached_service_binary(&svc.name) {
            let base = Path::new(&path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if base.eq_ignore_ascii_case(image)
                || base.eq_ignore_ascii_case(&format!("{image}.exe"))
                || format!("{base}.exe").eq_ignore_ascii_case(image)
            {
                return Some(path);
            }
        }
    }
    // Single dedicated service: trust its ImagePath even if name casing differs.
    if svcs.len() == 1 {
        return cached_service_binary(&svcs[0].name);
    }
    None
}

fn kill_one(pid: Pid) -> Result<(), ProbeError> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, TerminateProcess, PROCESS_TERMINATE,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, false, pid).map_err(|e| {
            ProbeError::msg(format!("OpenProcess({pid}) failed: {e}"))
        })?;
        let result = TerminateProcess(handle, 1);
        let _ = CloseHandle(handle);
        result.map_err(|e| ProbeError::msg(format!("TerminateProcess({pid}) failed: {e}")))?;
    }
    Ok(())
}

fn kill_tree(root: Pid) -> Result<(), ProbeError> {
    // Collect children via a fresh list, deepest-first-ish: kill children then root.
    let probe = SysinfoProcessProbe::new();
    let list = probe.list_processes()?;
    let mut to_kill = Vec::new();
    collect_descendants(root, &list, &mut to_kill);
    to_kill.push(root);
    // Reverse so children die before parents when possible.
    to_kill.reverse();
    let mut errors = Vec::new();
    for pid in to_kill {
        if let Err(e) = kill_one(pid) {
            errors.push(format!("{pid}: {e}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ProbeError::msg(format!(
            "some processes not terminated: {}",
            errors.join("; ")
        )))
    }
}

fn collect_descendants(parent: Pid, list: &[ProcessInfo], out: &mut Vec<Pid>) {
    for p in list {
        if p.parent_pid == Some(parent) && p.pid != parent {
            collect_descendants(p.pid, list, out);
            out.push(p.pid);
        }
    }
}

fn cached_file_description(path: &str) -> Option<String> {
    if let Ok(cache) = DISPLAY_NAME_CACHE.lock() {
        if let Some(hit) = cache.get(path) {
            return hit.clone();
        }
    }
    let value = query_file_description(path);
    if let Ok(mut cache) = DISPLAY_NAME_CACHE.lock() {
        cache.insert(path.to_string(), value.clone());
    }
    value
}

fn query_file_description(path: &str) -> Option<String> {
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };

    unsafe {
        let path_w = HSTRING::from(path);
        let mut dummy = 0u32;
        let size = GetFileVersionInfoSizeW(&path_w, Some(&mut dummy));
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        GetFileVersionInfoW(&path_w, None, size, buf.as_mut_ptr().cast()).ok()?;

        let mut trans_ptr: *mut core::ffi::c_void = core::ptr::null_mut();
        let mut trans_len = 0u32;
        if !VerQueryValueW(
            buf.as_ptr().cast(),
            &HSTRING::from(r"\VarFileInfo\Translation"),
            &mut trans_ptr,
            &mut trans_len,
        )
        .as_bool()
            || trans_ptr.is_null()
            || trans_len < 4
        {
            return None;
        }

        let lang_cp = *(trans_ptr as *const u32);
        let lang = lang_cp & 0xffff;
        let codepage = (lang_cp >> 16) & 0xffff;
        let key = format!(r"\StringFileInfo\{lang:04x}{codepage:04x}\FileDescription");

        let mut desc_ptr: *mut core::ffi::c_void = core::ptr::null_mut();
        let mut desc_len = 0u32;
        if !VerQueryValueW(
            buf.as_ptr().cast(),
            &HSTRING::from(key.as_str()),
            &mut desc_ptr,
            &mut desc_len,
        )
        .as_bool()
            || desc_ptr.is_null()
            || desc_len == 0
        {
            return None;
        }

        // `desc_len` is character count for Unicode strings (incl. NUL when present).
        let wide = std::slice::from_raw_parts(desc_ptr as *const u16, desc_len as usize);
        let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
        let s = String::from_utf16_lossy(&wide[..end]);
        let s = s.trim();
        if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    }
}

fn cached_service_binary(service_name: &str) -> Option<String> {
    if let Ok(cache) = SERVICE_BIN_CACHE.lock() {
        if let Some(hit) = cache.get(service_name) {
            return hit.clone();
        }
    }
    let value = query_service_binary(service_name);
    if let Ok(mut cache) = SERVICE_BIN_CACHE.lock() {
        cache.insert(service_name.to_string(), value.clone());
    }
    value
}

fn query_service_binary(service_name: &str) -> Option<String> {
    use windows::core::HSTRING;
    use windows::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceConfigW,
        QUERY_SERVICE_CONFIGW, SC_MANAGER_CONNECT, SERVICE_QUERY_CONFIG,
    };

    unsafe {
        let scm = OpenSCManagerW(None, None, SC_MANAGER_CONNECT).ok()?;
        let svc = match OpenServiceW(scm, &HSTRING::from(service_name), SERVICE_QUERY_CONFIG) {
            Ok(h) => h,
            Err(_) => {
                let _ = CloseServiceHandle(scm);
                return None;
            }
        };
        let mut needed = 0u32;
        let _ = QueryServiceConfigW(svc, None, 0, &mut needed);
        if needed == 0 {
            let _ = CloseServiceHandle(svc);
            let _ = CloseServiceHandle(scm);
            return None;
        }
        let mut buf = vec![0u8; needed as usize];
        let cfg = buf.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW;
        let ok = QueryServiceConfigW(svc, Some(cfg), needed, &mut needed);
        let path = if ok.is_ok() {
            let raw = (*cfg).lpBinaryPathName;
            if raw.is_null() {
                None
            } else {
                parse_image_path(&raw.to_string().unwrap_or_default())
            }
        } else {
            None
        };
        let _ = CloseServiceHandle(svc);
        let _ = CloseServiceHandle(scm);
        path
    }
}

fn parse_image_path(path_name: &str) -> Option<String> {
    let s = path_name.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(rest) = s.strip_prefix('"') {
        let end = rest.find('"')?;
        let path = &rest[..end];
        if path.is_empty() {
            None
        } else {
            Some(path.to_string())
        }
    } else {
        Some(s.split_whitespace().next()?.to_string())
    }
}

fn enumerate_services_by_pid() -> HashMap<u32, Vec<RunningService>> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Services::{
        CloseServiceHandle, EnumServicesStatusExW, OpenSCManagerW, ENUM_SERVICE_STATUS_PROCESSW,
        SC_ENUM_PROCESS_INFO, SC_MANAGER_CONNECT, SC_MANAGER_ENUMERATE_SERVICE, SERVICE_STATE_ALL,
        SERVICE_WIN32,
    };

    let mut map: HashMap<u32, Vec<RunningService>> = HashMap::new();
    unsafe {
        let Ok(scm) = OpenSCManagerW(
            None,
            None,
            SC_MANAGER_CONNECT | SC_MANAGER_ENUMERATE_SERVICE,
        ) else {
            return map;
        };

        let mut needed = 0u32;
        let mut returned = 0u32;
        let mut resume = 0u32;
        // Size probe.
        let _ = EnumServicesStatusExW(
            scm,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            None,
            &mut needed,
            &mut returned,
            Some(&mut resume),
            PCWSTR::null(),
        );
        if needed == 0 {
            let _ = CloseServiceHandle(scm);
            return map;
        }

        let mut buf = vec![0u8; needed as usize];
        resume = 0;
        if EnumServicesStatusExW(
            scm,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            Some(&mut buf),
            &mut needed,
            &mut returned,
            Some(&mut resume),
            PCWSTR::null(),
        )
        .is_err()
        {
            let _ = CloseServiceHandle(scm);
            return map;
        }

        let entries = std::slice::from_raw_parts(
            buf.as_ptr() as *const ENUM_SERVICE_STATUS_PROCESSW,
            returned as usize,
        );
        for e in entries {
            let pid = e.ServiceStatusProcess.dwProcessId;
            if pid == 0 {
                continue;
            }
            let name = pwstr_to_string(e.lpServiceName);
            let display_name = pwstr_to_string(e.lpDisplayName);
            if name.is_empty() {
                continue;
            }
            map.entry(pid).or_default().push(RunningService {
                name,
                display_name,
            });
        }
        let _ = CloseServiceHandle(scm);
    }
    map
}

unsafe fn pwstr_to_string(p: windows::core::PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { p.to_string().unwrap_or_default() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::filter_processes;

    #[test]
    fn defender_display_and_anti_search() {
        let probe = SysinfoProcessProbe::new();
        let list = probe.list_processes().expect("list");
        let ms = list.iter().find(|p| {
            p.name.eq_ignore_ascii_case("MsMpEng.exe") || p.name.eq_ignore_ascii_case("MsMpEng")
        });
        let Some(ms) = ms else {
            eprintln!("MsMpEng not running; skip assert");
            return;
        };
        eprintln!(
            "MsMpEng pid={} name={} display={:?} path={:?} services={:?}",
            ms.pid, ms.name, ms.display_name, ms.exe_path, ms.service_names
        );
        assert!(
            ms.display_name
                .as_deref()
                .is_some_and(|d| d.to_lowercase().contains("antimalware")),
            "expected Antimalware FileDescription, got {:?}",
            ms.display_name
        );
        let hits = filter_processes(&list, "anti");
        assert!(
            hits.iter().any(|p| p.pid == ms.pid),
            "search anti should include MsMpEng"
        );
    }
}
