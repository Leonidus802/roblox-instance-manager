#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(not(windows))]
compile_error!("This application targets Windows 10/11.");

mod model;
mod platform;
use model::{Config, Game};
use platform::{wide, Monitor, Session};
use std::{
    mem::zeroed,
    path::PathBuf,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    System::Threading::{BELOW_NORMAL_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS},
    UI::{Controls::*, HiDpi::*, Input::KeyboardAndMouse::SetFocus, WindowsAndMessaging::*},
};

const GAMES: i32 = 100;
const NAME: i32 = 101;
const PLACE: i32 = 102;
const SERVER: i32 = 103;
const SAVE: i32 = 104;
const NEW: i32 = 105;
const DELETE: i32 = 106;
const LAUNCH: i32 = 107;
const WEB: i32 = 108;
const LEGACY: i32 = 109;
const SESSIONS: i32 = 110;
const REFRESH: i32 = 111;
const FOCUS: i32 = 112;
const MINIMIZE: i32 = 113;
const LOWER: i32 = 114;
const NORMAL: i32 = 115;
const CLOSE: i32 = 116;
const STATUS: i32 = 117;
const SUMMARY: i32 = 118;

fn settings_path() -> Result<PathBuf, String> {
    let root = std::env::var_os("LOCALAPPDATA").ok_or("Windows LOCALAPPDATA is unavailable.")?;
    Ok(PathBuf::from(root)
        .join("RobloxInstanceManager")
        .join("games.json"))
}

struct App {
    hwnd: HWND,
    controls: Vec<(i32, HWND)>,
    font: HFONT,
    config: Config,
    config_path: PathBuf,
    config_error: Option<String>,
    monitor: Monitor,
    sessions: Vec<Session>,
    legacy: Option<platform::LegacyGuard>,
    scale: f64,
    dirty: bool,
    editing: Option<usize>,
}

impl App {
    fn control(&self, id: i32) -> HWND {
        self.controls
            .iter()
            .find(|(key, _)| *key == id)
            .map_or(null_mut(), |(_, h)| *h)
    }
    fn px(&self, n: i32) -> i32 {
        (n as f64 * self.scale).round() as i32
    }
    unsafe fn add(&mut self, id: i32, class: &str, text: &str, style: u32) {
        let hwnd = CreateWindowExW(
            0,
            wide(class).as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            0,
            0,
            self.hwnd,
            id as isize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        SendMessageW(hwnd, WM_SETFONT, self.font as usize, 1);
        self.controls.push((id, hwnd));
    }
    unsafe fn create_controls(&mut self) {
        self.scale = GetDpiForWindow(self.hwnd) as f64 / 96.;
        self.font = CreateFontW(
            -self.px(15),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            wide("Segoe UI").as_ptr(),
        );
        self.add(200, "STATIC", "Roblox Instance Manager", 0);
        self.add(201, "STATIC", "Saved games", 0);
        self.add(
            GAMES,
            "LISTBOX",
            "",
            WS_BORDER | WS_TABSTOP | WS_VSCROLL | LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32,
        );
        self.add(202, "STATIC", "Game name", 0);
        self.add(
            NAME,
            "EDIT",
            "",
            WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32,
        );
        self.add(203, "STATIC", "Place ID (number from the game's URL)", 0);
        self.add(
            PLACE,
            "EDIT",
            "",
            WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32,
        );
        self.add(204, "STATIC", "Server JobId (optional UUID)", 0);
        self.add(
            SERVER,
            "EDIT",
            "",
            WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32,
        );
        for (id, label) in [
            (SAVE, "Save game"),
            (NEW, "New"),
            (DELETE, "Remove"),
            (LAUNCH, "Launch current account"),
            (WEB, "Open browser game page"),
            (REFRESH, "Refresh"),
            (FOCUS, "Focus"),
            (MINIMIZE, "Minimize"),
            (LOWER, "Lower CPU priority"),
            (NORMAL, "Normal priority"),
            (CLOSE, "Close session"),
        ] {
            self.add(id, "BUTTON", label, WS_TABSTOP | BS_PUSHBUTTON as u32);
        }
        self.add(
            LEGACY,
            "BUTTON",
            "Legacy multi-instance compatibility (experimental)",
            WS_TABSTOP | BS_CHECKBOX as u32,
        );
        self.add(205,"STATIC","Sign into each account in your browser and press Play there. Direct launch uses Roblox's current account.",0);
        self.add(206,"STATIC","Legacy mode must be enabled before opening Roblox. Current Roblox builds may still reject multiple clients.",0);
        self.add(
            207,
            "STATIC",
            "Running Roblox clients  |  PID / RAM / CPU (% of whole PC) / priority / window title",
            0,
        );
        self.add(
            SESSIONS,
            "LISTBOX",
            "",
            WS_BORDER
                | WS_TABSTOP
                | WS_VSCROLL
                | WS_HSCROLL
                | LBS_NOTIFY as u32
                | LBS_NOINTEGRALHEIGHT as u32,
        );
        self.add(SUMMARY, "STATIC", "", 0);
        self.add(
            STATUS,
            "STATIC",
            "Ready. Add a game to get started. Refreshes every 5 seconds; pauses when minimized.",
            0,
        );
        SendMessageW(self.control(NAME), EM_SETLIMITTEXT, 80, 0);
        SendMessageW(self.control(PLACE), EM_SETLIMITTEXT, 20, 0);
        SendMessageW(self.control(SERVER), EM_SETLIMITTEXT, 36, 0);
        SendMessageW(
            self.control(SESSIONS),
            LB_SETHORIZONTALEXTENT,
            self.px(1400) as usize,
            0,
        );
        self.render_games(None);
        self.refresh();
        SetTimer(self.hwnd, 1, 5000, None);
    }
    unsafe fn layout(&self) {
        let mut rect: RECT = zeroed();
        GetClientRect(self.hwnd, &mut rect);
        let w = (rect.right as f64 / self.scale) as i32;
        let h = (rect.bottom as f64 / self.scale) as i32;
        let right = w - 322;
        let rows = [
            (200, 20, 14, w - 40, 26),
            (201, 20, 52, 260, 22),
            (GAMES, 20, 80, 270, 213),
            (202, 310, 52, right, 22),
            (NAME, 310, 78, right, 28),
            (203, 310, 112, right, 22),
            (PLACE, 310, 138, right, 28),
            (204, 310, 172, right, 22),
            (SERVER, 310, 198, right, 28),
            (SAVE, 310, 242, 116, 32),
            (NEW, 436, 242, 80, 32),
            (DELETE, 526, 242, 90, 32),
            (LAUNCH, 20, 306, 240, 34),
            (WEB, 270, 306, 260, 34),
            (LEGACY, 20, 354, w - 40, 26),
            (205, 20, 388, w - 40, 36),
            (206, 20, 429, w - 40, 36),
            (207, 20, 475, w - 40, 22),
            (SESSIONS, 20, 501, w - 40, h - 636),
            (REFRESH, 20, h - 123, 86, 32),
            (FOCUS, 116, h - 123, 76, 32),
            (MINIMIZE, 202, h - 123, 90, 32),
            (LOWER, 302, h - 123, 160, 32),
            (NORMAL, 472, h - 123, 140, 32),
            (CLOSE, 622, h - 123, 132, 32),
            (SUMMARY, 20, h - 80, w - 40, 24),
            (STATUS, 20, h - 50, w - 40, 42),
        ];
        for (id, x, y, cw, ch) in rows {
            MoveWindow(
                self.control(id),
                self.px(x),
                self.px(y),
                self.px(cw),
                self.px(ch),
                1,
            );
        }
    }
    unsafe fn text(&self, id: i32) -> String {
        let hwnd = self.control(id);
        let n = GetWindowTextLengthW(hwnd).max(0) as usize;
        let mut data = vec![0u16; n + 1];
        let written = GetWindowTextW(hwnd, data.as_mut_ptr(), data.len() as i32).max(0) as usize;
        String::from_utf16_lossy(&data[..written])
    }
    unsafe fn set(&self, id: i32, text: &str) {
        SetWindowTextW(self.control(id), wide(text).as_ptr());
    }
    unsafe fn status(&self, text: &str) {
        self.set(STATUS, text);
    }
    unsafe fn error(&self, text: &str) {
        MessageBoxW(
            self.hwnd,
            wide(text).as_ptr(),
            wide("Roblox Instance Manager").as_ptr(),
            MB_OK | MB_ICONWARNING,
        );
    }
    unsafe fn selection(&self, id: i32) -> Option<usize> {
        let index = SendMessageW(self.control(id), LB_GETCURSEL, 0, 0);
        (index >= 0).then_some(index as usize)
    }
    unsafe fn render_games(&self, selected: Option<usize>) {
        SendMessageW(self.control(GAMES), WM_SETREDRAW, 0, 0);
        SendMessageW(self.control(GAMES), LB_RESETCONTENT, 0, 0);
        for g in &self.config.games {
            SendMessageW(
                self.control(GAMES),
                LB_ADDSTRING,
                0,
                wide(&g.name).as_ptr() as isize,
            );
        }
        if let Some(i) = selected {
            SendMessageW(self.control(GAMES), LB_SETCURSEL, i, 0);
        }
        SendMessageW(self.control(GAMES), WM_SETREDRAW, 1, 0);
        InvalidateRect(self.control(GAMES), null(), 1);
    }
    unsafe fn clear_form(&mut self) {
        self.set(NAME, "");
        self.set(PLACE, "");
        self.set(SERVER, "");
        self.dirty = false;
        self.editing = None;
        SendMessageW(self.control(GAMES), LB_SETCURSEL, usize::MAX, 0);
    }
    unsafe fn current_game(&self) -> Result<Game, String> {
        Game::new(&self.text(NAME), &self.text(PLACE), &self.text(SERVER))
    }
    unsafe fn commit(&mut self, candidate: Config) -> bool {
        if let Some(error) = &self.config_error {
            self.error(&format!(
                "{error}\n\nMove or repair {} and restart before saving.",
                self.config_path.display()
            ));
            return false;
        }
        match candidate.save(&self.config_path) {
            Ok(()) => {
                self.config = candidate;
                self.dirty = false;
                true
            }
            Err(e) => {
                self.error(&format!("Could not save games: {e}"));
                false
            }
        }
    }
    unsafe fn refresh(&mut self) {
        let selected_pid = self
            .selection(SESSIONS)
            .and_then(|i| self.sessions.get(i))
            .map(|s| (s.pid, s.created));
        match self.monitor.refresh() {
            Ok(sessions) => self.sessions = sessions,
            Err(e) => {
                self.status(&e);
                return;
            }
        }
        SendMessageW(self.control(SESSIONS), WM_SETREDRAW, 0, 0);
        SendMessageW(self.control(SESSIONS), LB_RESETCONTENT, 0, 0);
        let mut total = 0usize;
        let mut unknown = false;
        for (i, s) in self.sessions.iter().enumerate() {
            total += s.memory.unwrap_or(0);
            unknown |= s.memory.is_none();
            let ram = s.memory.map_or_else(
                || "RAM unavailable".into(),
                |m| format!("{:.1} MiB", m as f64 / 1048576.),
            );
            let cpu = s
                .cpu
                .map_or_else(|| "CPU --".into(), |c| format!("CPU {c:.1}%"));
            let priority = match s.priority {
                BELOW_NORMAL_PRIORITY_CLASS => "Below normal",
                NORMAL_PRIORITY_CLASS => "Normal",
                0 => "Priority unavailable",
                _ => "Other priority",
            };
            let title = s.title.replace(['\r', '\n', '\t'], " ");
            let text = format!("{}  |  {ram}  |  {cpu}  |  {priority}  |  {title}", s.pid);
            SendMessageW(
                self.control(SESSIONS),
                LB_ADDSTRING,
                0,
                wide(&text).as_ptr() as isize,
            );
            if selected_pid == Some((s.pid, s.created)) {
                SendMessageW(self.control(SESSIONS), LB_SETCURSEL, i, 0);
            }
        }
        SendMessageW(self.control(SESSIONS), WM_SETREDRAW, 1, 0);
        InvalidateRect(self.control(SESSIONS), null(), 1);
        let own = platform::own_memory().map_or("unavailable".into(), |m| {
            format!("{:.1} MiB", m as f64 / 1048576.)
        });
        self.set(
            SUMMARY,
            &format!(
                "{} client(s)  |  Roblox RAM: {:.1} MiB{}  |  Manager RAM: {own}",
                self.sessions.len(),
                total as f64 / 1048576.,
                if unknown {
                    " + unavailable readings"
                } else {
                    ""
                }
            ),
        );
    }
    unsafe fn command(&mut self, id: i32, notification: u16) {
        if [NAME, PLACE, SERVER].contains(&id) {
            if notification == EN_CHANGE as u16 {
                self.dirty = true;
            }
            return;
        }
        if id == GAMES && notification == LBN_SELCHANGE as u16 {
            if !self.discard_edits() {
                SendMessageW(
                    self.control(GAMES),
                    LB_SETCURSEL,
                    self.editing.unwrap_or(usize::MAX),
                    0,
                );
                return;
            }
            self.editing = self.selection(GAMES);
            if let Some(game) = self.selection(GAMES).and_then(|i| self.config.games.get(i)) {
                self.set(NAME, &game.name);
                self.set(PLACE, &game.place_id.to_string());
                self.set(SERVER, &game.server_id);
                self.dirty = false;
            }
            return;
        }
        if notification != BN_CLICKED as u16 {
            return;
        }
        match id {
            NEW => {
                if self.discard_edits() {
                    self.clear_form();
                    SetFocus(self.control(NAME));
                }
            }
            SAVE => match self.current_game() {
                Ok(game) => {
                    let mut config = self.config.clone();
                    let i = if let Some(i) = self.selection(GAMES) {
                        config.games[i] = game;
                        i
                    } else {
                        config.games.push(game);
                        config.games.len() - 1
                    };
                    if self.commit(config) {
                        self.editing = Some(i);
                        self.render_games(Some(i));
                        self.status("Game saved.");
                    }
                }
                Err(e) => self.error(&e),
            },
            DELETE => {
                if let Some(i) = self.selection(GAMES) {
                    if MessageBoxW(
                        self.hwnd,
                        wide("Remove the selected saved game?").as_ptr(),
                        wide("Remove game").as_ptr(),
                        MB_YESNO | MB_ICONQUESTION,
                    ) == IDYES
                    {
                        let mut config = self.config.clone();
                        config.games.remove(i);
                        if self.commit(config) {
                            self.render_games(None);
                            self.clear_form();
                            self.status("Saved game removed.");
                        }
                    }
                } else {
                    self.status("Select a saved game first.");
                }
            }
            LAUNCH | WEB => match self.current_game() {
                Ok(g) => {
                    let uri = if id == LAUNCH {
                        g.launch_uri()
                    } else {
                        g.web_url()
                    };
                    match platform::launch(self.hwnd,&uri) {
                        Ok(()) => self.status(if id == LAUNCH {"Launch link sent to Windows. Check Roblox; this does not confirm that a new client joined."}
                            else {"Game page opened. Sign into the desired account in your browser, then press Play."}),
                        Err(e) => self.error(&e),
                    }
                }
                Err(e) => self.error(&e),
            },
            LEGACY => {
                if self.legacy.is_some() {
                    self.legacy = None;
                    SendMessageW(self.control(LEGACY), BM_SETCHECK, BST_UNCHECKED as usize, 0);
                    self.status("Legacy compatibility disabled. Roblox clients remain open.");
                } else {
                    match self.monitor.refresh() {
                        Ok(s) if !s.is_empty() => {
                            self.error(
                                "Close all Roblox clients before enabling legacy compatibility.",
                            );
                            return;
                        }
                        Err(e) => {
                            self.error(&e);
                            return;
                        }
                        _ => {}
                    }
                    match platform::LegacyGuard::enable() {
                        Ok(guard) => {
                            self.legacy = Some(guard);
                            SendMessageW(
                                self.control(LEGACY),
                                BM_SETCHECK,
                                BST_CHECKED as usize,
                                0,
                            );
                            self.status("Legacy mutex held. Keep this manager open. This is experimental; Roblox may still allow only one client.");
                        }
                        Err(e) => self.error(&e),
                    }
                }
            }
            REFRESH => {
                self.refresh();
                self.status("Session list refreshed. CPU readings need two samples.");
            }
            FOCUS | MINIMIZE | LOWER | NORMAL | CLOSE => {
                let Some(session) = self
                    .selection(SESSIONS)
                    .and_then(|i| self.sessions.get(i))
                    .cloned()
                else {
                    self.status("Select a running Roblox client first.");
                    return;
                };
                if id == CLOSE
                    && MessageBoxW(
                        self.hwnd,
                        wide(&format!("Ask Roblox client {} to close?", session.pid)).as_ptr(),
                        wide("Close session").as_ptr(),
                        MB_YESNO | MB_ICONQUESTION,
                    ) != IDYES
                {
                    return;
                }
                let result = match id {
                    LOWER | NORMAL => platform::priority(&session, id == LOWER),
                    FOCUS => platform::window_action(&session, platform::WindowAction::Focus),
                    MINIMIZE => platform::window_action(&session, platform::WindowAction::Minimize),
                    _ => platform::window_action(&session, platform::WindowAction::Close),
                };
                match result {
                    Ok(()) => self.status(match id {
                        LOWER => "Priority lowered. This changes CPU scheduling; it does not cap usage or reduce RAM/GPU demand.",
                        NORMAL => "Normal CPU priority restored.", CLOSE => "Close request sent to the selected Roblox window.",
                        MINIMIZE => "Selected Roblox window minimized.", _ => "Selected Roblox window focused.",
                    }), Err(e) => self.error(&e),
                }
                self.refresh();
            }
            _ => {}
        }
    }
    unsafe fn discard_edits(&self) -> bool {
        !self.dirty
            || MessageBoxW(
                self.hwnd,
                wide("Discard unsaved game edits?").as_ptr(),
                wide("Unsaved edits").as_ptr(),
                MB_YESNO | MB_ICONQUESTION,
            ) == IDYES
    }
}

thread_local! { static DISPATCHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
struct DispatchGuard;
impl DispatchGuard {
    fn enter() -> Option<Self> {
        DISPATCHING.with(|busy| if busy.replace(true) { None } else { Some(Self) })
    }
}
impl Drop for DispatchGuard {
    fn drop(&mut self) {
        DISPATCHING.with(|busy| busy.set(false));
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
    // Native control updates and modal dialogs pump messages synchronously.
    // Never create a second Rust borrow of App during a nested dispatch.
    let _guard = if matches!(
        msg,
        WM_CREATE | WM_COMMAND | WM_SIZE | WM_TIMER | WM_DPICHANGED | WM_CLOSE
    ) {
        match DispatchGuard::enter() {
            Some(g) => Some(g),
            None => return 0,
        }
    } else {
        None
    };
    match msg {
        WM_CREATE => {
            let create = &*(lp as *const CREATESTRUCTW);
            let app = &mut *(create.lpCreateParams as *mut App);
            app.hwnd = hwnd;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, app as *mut _ as isize);
            app.create_controls();
            app.layout();
            0
        }
        WM_COMMAND if !raw.is_null() => {
            (*raw).command((wp & 0xffff) as i32, ((wp >> 16) & 0xffff) as u16);
            0
        }
        WM_SIZE if !raw.is_null() => {
            if wp == SIZE_MINIMIZED as usize {
                KillTimer(hwnd, 1);
            } else {
                (*raw).layout();
                SetTimer(hwnd, 1, 5000, None);
            }
            0
        }
        WM_GETMINMAXINFO => {
            let dpi = GetDpiForWindow(hwnd).max(96) as f64 / 96.;
            let info = &mut *(lp as *mut MINMAXINFO);
            info.ptMinTrackSize = POINT {
                x: (820. * dpi) as i32,
                y: (820. * dpi) as i32,
            };
            0
        }
        WM_TIMER if !raw.is_null() => {
            if IsIconic(hwnd) == 0 {
                (*raw).refresh();
            }
            0
        }
        WM_DPICHANGED if !raw.is_null() => {
            // Windows supplies a suggested rectangle when moving between monitors.
            let rect = &*(lp as *const RECT);
            (*raw).scale = (wp & 0xffff) as f64 / 96.;
            SetWindowPos(
                hwnd,
                null_mut(),
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            let old_font = (*raw).font;
            (*raw).font = CreateFontW(
                -(*raw).px(15),
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32,
                wide("Segoe UI").as_ptr(),
            );
            for (_, control) in &(*raw).controls {
                SendMessageW(*control, WM_SETFONT, (*raw).font as usize, 1);
            }
            if !old_font.is_null() {
                DeleteObject(old_font);
            }
            (*raw).layout();
            0
        }
        WM_CLOSE if !raw.is_null() => {
            if (*raw).dirty
                && MessageBoxW(
                    hwnd,
                    wide("Discard unsaved game edits and close the manager?").as_ptr(),
                    wide("Unsaved edits").as_ptr(),
                    MB_YESNO | MB_ICONQUESTION,
                ) != IDYES
            {
                return 0;
            }
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, 1);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn run() -> Result<(), String> {
    let lock = platform::AppLock::acquire()?;
    lock.keep_alive();
    let path = settings_path()?;
    let (config, config_error) = match Config::load(&path) {
        Ok(c) => (c, None),
        Err(e) => (Config::default(), Some(e)),
    };
    let mut app = Box::new(App {
        hwnd: null_mut(),
        controls: Vec::new(),
        font: null_mut(),
        config,
        config_path: path,
        config_error,
        monitor: Monitor::default(),
        sessions: Vec::new(),
        legacy: None,
        scale: 1.,
        dirty: false,
        editing: None,
    });
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(null());
        let class = wide("RobloxInstanceManagerWindow");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
            hbrBackground: (COLOR_BTNFACE + 1) as HBRUSH,
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        if RegisterClassW(&wc) == 0 {
            return Err(platform::os_error("Cannot register window"));
        }
        let dpi = GetDpiForSystem() as f64 / 96.;
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class.as_ptr(),
            wide("Roblox Instance Manager").as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            (940. * dpi) as i32,
            (860. * dpi) as i32,
            null_mut(),
            null_mut(),
            instance,
            &mut *app as *mut _ as *const _,
        );
        if hwnd.is_null() {
            return Err(platform::os_error("Cannot create window"));
        }
        ShowWindow(hwnd, SW_SHOW);
        if let Some(error) = app.config_error.clone() {
            MessageBoxW(
                hwnd,
                wide(&error).as_ptr(),
                wide("Saved games unavailable").as_ptr(),
                MB_OK | MB_ICONWARNING,
            );
        }
        let mut msg: MSG = zeroed();
        loop {
            let result = GetMessageW(&mut msg, null_mut(), 0, 0);
            if result == 0 {
                break;
            }
            if result < 0 {
                return Err(platform::os_error("Window message loop failed"));
            }
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        if !app.font.is_null() {
            DeleteObject(app.font);
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        unsafe {
            MessageBoxW(
                null_mut(),
                wide(&error).as_ptr(),
                wide("Roblox Instance Manager").as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}
