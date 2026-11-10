use std::{
    collections::BTreeMap,
    mem::{size_of, zeroed},
    ptr::null,
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{Diagnostics::ToolHelp::*, ProcessStatus::*, Threading::*},
    UI::{Shell::ShellExecuteW, WindowsAndMessaging::*},
};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn from_wide(s: &[u16]) -> String {
    String::from_utf16_lossy(&s[..s.iter().position(|c| *c == 0).unwrap_or(s.len())])
}
pub fn os_error(context: &str) -> String {
    format!("{context}: {}", std::io::Error::last_os_error())
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn open(pid: u32, rights: u32) -> Result<Handle, String> {
    let handle = unsafe { OpenProcess(rights, 0, pid) };
    if handle.is_null() {
        Err(os_error("Cannot access process"))
    } else {
        Ok(Handle(handle))
    }
}

fn times(handle: HANDLE) -> Option<(u64, u64)> {
    unsafe {
        let (mut created, mut exit, mut kernel, mut user): (
            FILETIME,
            FILETIME,
            FILETIME,
            FILETIME,
        ) = (zeroed(), zeroed(), zeroed(), zeroed());
        if GetProcessTimes(handle, &mut created, &mut exit, &mut kernel, &mut user) == 0 {
            return None;
        }
        let value = |v: FILETIME| ((v.dwHighDateTime as u64) << 32) | v.dwLowDateTime as u64;
        Some((value(created), value(kernel) + value(user)))
    }
}

fn player_executable(handle: HANDLE) -> bool {
    let mut path = vec![0u16; 32768];
    let mut len = path.len() as u32;
    if unsafe { QueryFullProcessImageNameW(handle, 0, path.as_mut_ptr(), &mut len) } == 0 {
        return false;
    }
    from_wide(&path[..len as usize])
        .rsplit('\\')
        .next()
        .is_some_and(|s| s.eq_ignore_ascii_case("RobloxPlayerBeta.exe"))
}

fn memory(handle: HANDLE) -> Option<usize> {
    unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS = zeroed();
        counters.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        (K32GetProcessMemoryInfo(
            handle,
            &mut counters,
            size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ) != 0)
            .then_some(counters.WorkingSetSize)
    }
}

pub fn own_memory() -> Option<usize> {
    memory(unsafe { GetCurrentProcess() })
}

#[derive(Clone, Debug)]
pub struct Session {
    pub pid: u32,
    pub created: u64,
    pub title: String,
    pub memory: Option<usize>,
    pub cpu: Option<f64>,
    pub priority: u32,
}

#[derive(Default)]
pub struct Monitor {
    // Samples only; never retain handles to Roblox between refreshes.
    previous: BTreeMap<u32, (u64, u64)>,
    at: Option<Instant>,
}

impl Monitor {
    pub fn refresh(&mut self) -> Result<Vec<Session>, String> {
        let now = Instant::now();
        let elapsed = self.at.map(|at| now.duration_since(at).as_secs_f64());
        let cores = std::thread::available_parallelism().map_or(1., |n| n.get() as f64);
        let mut next = BTreeMap::new();
        let mut sessions = Vec::new();
        unsafe {
            let raw = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if raw == INVALID_HANDLE_VALUE {
                return Err(os_error("Cannot list processes"));
            }
            let snapshot = Handle(raw);
            let mut entry: PROCESSENTRY32W = zeroed();
            entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            let mut found = Process32FirstW(snapshot.0, &mut entry) != 0;
            while found {
                if from_wide(&entry.szExeFile).eq_ignore_ascii_case("RobloxPlayerBeta.exe") {
                    let pid = entry.th32ProcessID;
                    // The OS may deny memory access; retain a visible row with unknown metrics.
                    let process = open(pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ)
                        .or_else(|_| open(pid, PROCESS_QUERY_LIMITED_INFORMATION));
                    let mut session = Session {
                        pid,
                        created: 0,
                        title: String::new(),
                        memory: None,
                        cpu: None,
                        priority: 0,
                    };
                    if let Ok(process) = process {
                        if player_executable(process.0) {
                            if let Some((created, ticks)) = times(process.0) {
                                session.created = created;
                                session.cpu =
                                    self.previous
                                        .get(&pid)
                                        .and_then(|(old_created, old_ticks)| {
                                            let secs = elapsed?;
                                            (*old_created == created && secs > 0.).then(|| {
                                                ((ticks.saturating_sub(*old_ticks)) as f64
                                                    / 100_000.
                                                    / secs
                                                    / cores)
                                                    .clamp(0., 100.)
                                            })
                                        });
                                next.insert(pid, (created, ticks));
                            }
                            session.memory = memory(process.0);
                            session.priority = GetPriorityClass(process.0);
                        }
                    }
                    let windows = windows_for(pid);
                    if let Some(window) = windows.first() {
                        let mut title = [0u16; 512];
                        GetWindowTextW(*window, title.as_mut_ptr(), title.len() as i32);
                        session.title = from_wide(&title);
                    }
                    if session.title.is_empty() {
                        session.title = "Roblox (no visible window)".into();
                    }
                    sessions.push(session);
                }
                found = Process32NextW(snapshot.0, &mut entry) != 0;
            }
        }
        sessions.sort_by_key(|s| s.pid);
        self.previous = next;
        self.at = Some(now);
        Ok(sessions)
    }
}

struct WindowSearch {
    pid: u32,
    windows: Vec<HWND>,
}
unsafe extern "system" fn collect_window(hwnd: HWND, param: LPARAM) -> BOOL {
    let search = &mut *(param as *mut WindowSearch);
    let mut pid = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == search.pid && IsWindowVisible(hwnd) != 0 && GetWindow(hwnd, GW_OWNER).is_null() {
        search.windows.push(hwnd);
    }
    1
}
fn windows_for(pid: u32) -> Vec<HWND> {
    let mut search = WindowSearch {
        pid,
        windows: Vec::new(),
    };
    unsafe {
        EnumWindows(Some(collect_window), &mut search as *mut _ as isize);
    }
    search.windows
}

fn verified(session: &Session, rights: u32) -> Result<Handle, String> {
    if session.created == 0 {
        return Err("Process identity unavailable. Refresh and try again.".into());
    }
    let process = open(
        session.pid,
        rights | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
    )?;
    if unsafe { WaitForSingleObject(process.0, 0) } != WAIT_TIMEOUT
        || !player_executable(process.0)
        || times(process.0).map(|t| t.0) != Some(session.created)
    {
        return Err("This session has ended. Refresh the list.".into());
    }
    Ok(process)
}

pub fn priority(session: &Session, lower: bool) -> Result<(), String> {
    let handle = verified(session, PROCESS_SET_INFORMATION)?;
    if unsafe {
        SetPriorityClass(
            handle.0,
            if lower {
                BELOW_NORMAL_PRIORITY_CLASS
            } else {
                NORMAL_PRIORITY_CLASS
            },
        )
    } == 0
    {
        Err(os_error("Windows denied the priority change"))
    } else {
        Ok(())
    }
}

pub enum WindowAction {
    Focus,
    Minimize,
    Close,
}
pub fn window_action(session: &Session, action: WindowAction) -> Result<(), String> {
    let _handle = verified(session, 0)?;
    let windows = windows_for(session.pid);
    let window = *windows
        .first()
        .ok_or("No visible window is available for this session.")?;
    unsafe {
        match action {
            WindowAction::Focus => {
                ShowWindowAsync(window, SW_RESTORE);
                if SetForegroundWindow(window) == 0 {
                    return Err(
                        "Windows did not allow the foreground switch. Use the taskbar.".into(),
                    );
                }
            }
            WindowAction::Minimize => {
                ShowWindowAsync(window, SW_MINIMIZE);
            }
            WindowAction::Close => {
                if PostMessageW(window, WM_CLOSE, 0, 0) == 0 {
                    return Err(os_error("Cannot request window close"));
                }
            }
        }
    }
    Ok(())
}

pub fn launch(owner: HWND, uri: &str) -> Result<(), String> {
    let result = unsafe {
        ShellExecuteW(
            owner,
            wide("open").as_ptr(),
            wide(uri).as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result <= 32 {
        Err(format!("Windows could not open this link (code {result}). Check Roblox and your default browser are installed."))
    } else {
        Ok(())
    }
}

pub struct LegacyGuard {
    mutex: Handle,
}
impl LegacyGuard {
    pub fn enable() -> Result<Self, String> {
        // Historical compatibility only. No client memory, files, or foreign handles are changed.
        let raw = unsafe { CreateMutexW(null(), 1, wide("ROBLOX_singletonMutex").as_ptr()) };
        if raw.is_null() {
            return Err(os_error("Cannot create compatibility mutex"));
        }
        let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let mutex = Handle(raw);
        if existed {
            return Err("The Roblox mutex already exists. Close Roblox and other instance managers, then retry.".into());
        }
        Ok(Self { mutex })
    }
}
impl Drop for LegacyGuard {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.mutex.0);
        }
    }
}

pub struct AppLock(Handle);
impl AppLock {
    pub fn acquire() -> Result<Self, String> {
        let raw =
            unsafe { CreateMutexW(null(), 0, wide("Local\\RobloxInstanceManager-0.1").as_ptr()) };
        if raw.is_null() {
            return Err(os_error("Cannot create manager lock"));
        }
        let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let handle = Handle(raw);
        if existed {
            return Err("The manager is already open. Check your taskbar.".into());
        }
        Ok(Self(handle))
    }
    pub fn keep_alive(&self) {
        let _ = self.0 .0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_metrics_and_monitor_are_available() {
        assert!(own_memory().unwrap() > 0);
        let (created, _) = times(unsafe { GetCurrentProcess() }).unwrap();
        assert!(created > 0);
        assert!(!player_executable(unsafe { GetCurrentProcess() }));
        let mut monitor = Monitor::default();
        monitor.refresh().unwrap();
        monitor.refresh().unwrap();
    }
    #[test]
    fn stale_process_actions_are_rejected() {
        let fake = Session {
            pid: std::process::id(),
            created: 0,
            title: String::new(),
            memory: None,
            cpu: None,
            priority: 0,
        };
        assert!(priority(&fake, true).is_err());
        assert!(window_action(&fake, WindowAction::Close).is_err());
    }
}
