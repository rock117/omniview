use std::sync::Mutex;

use sysinfo::{ProcessesToUpdate, System, UpdateKind};

use crate::domain::{Pid, ProbeError, ProcessInfo};
use crate::platform::ProcessProbe;

/// Process probe backed by `sysinfo` (portable core; used on Windows first).
pub struct SysinfoProcessProbe {
    system: Mutex<System>,
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
            let exe = proc_
                .exe()
                .map(|p| p.to_string_lossy().into_owned());
            let user = proc_.user_id().map(|u| u.to_string());
            out.push(ProcessInfo {
                pid: pid_u,
                parent_pid: parent,
                name: proc_.name().to_string_lossy().into_owned(),
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
