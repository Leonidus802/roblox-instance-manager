# Roblox Instance Manager (Windows, Rust)

A small native Windows 10/11 x64 app for saving Roblox game shortcuts and managing visible Roblox Player processes. There is no webview, background service, telemetry, account database, or administrator requirement. One UI thread blocks on Windows messages, with a five-second monitor timer that stops while the manager is minimized.

## Compatibility status

**This is an experimental manager, not a verified way to run several current Roblox clients.** Roblox staff have stated that multiple desktop clients on one computer are not supported. The legacy mutex technique is included as an optional experiment and may have no effect on your installed client. Bloxstrap removed its own multi-instance feature in version 2.10.0 because Roblox broke compatibility. If launching a second game replaces the first, this app cannot guarantee simultaneous sessions.

References: [Roblox staff discussion](https://devforum.roblox.com/t/the-ability-to-run-2-roblox-clients-at-once/28334), [Bloxstrap 2.10.0 release notes](https://github.com/bloxstraplabs/bloxstrap/wiki/Release-notes-for-Bloxstrap-v2.10.0/01bc91cc45f73b4b7800c16880d3aae5958ff08f).

## Run

Open `dist/RobloxInstanceManager.exe`. Roblox must already be installed and its `roblox://` protocol registered.

1. If trying legacy multi-instance mode, close Roblox yourself, open this manager, and enable **Legacy multi-instance compatibility** before starting any clients. Keep the manager open. The setting intentionally resets to off at each startup.
2. Enter a game name and its numeric **Place ID** from `https://www.roblox.com/games/PLACE_ID/...`. An optional Server JobId must be a UUID. Click **Save game**; use **New** to create another shortcut.
3. For different accounts, use separate browser profiles signed into the accounts you want. Open the game page in each profile and press Play on Roblox's website. **Open browser game page** uses your default browser; choose the appropriate browser profile yourself. The manager does not create browser profiles or switch accounts.
4. **Launch current account** sends a Roblox launch link to Windows and uses the account signed into the Roblox app. Saved games are game shortcuts; they are not independent authenticated account slots. Windows accepting a launch link is not confirmation that a new client joined.
5. Select a running client to focus, minimize, request a normal window close, or explicitly change CPU priority. Roblox or Windows may deny an action; errors are shown. Closing this manager leaves Roblox clients running and releases its legacy mutex.

Launch links follow [Roblox's documented desktop link formats](https://create.roblox.com/docs/production/promotion/deeplinks). Roblox labels that public deep-link workflow deprecated; its future compatibility is outside this app's control.

## Resource use

- Release executable observed in the initial build: about **352 KiB**. Runtime RAM observed during local UI inspection: roughly **13–20 MiB working set**, varying with Windows UI/accessibility loading. These are observations, not guarantees or a proven minimum.
- No animation loop, browser engine, account refresh loop, or automatic network requests. Process enumeration and measurements happen every five seconds while visible, or when Refresh is pressed. The first CPU reading needs a second sample. CPU is normalized to total logical CPU capacity; RAM is process working set and can include shared pages.
- Process handles close after each operation. Unavailable measurements are labeled; process creation timestamps are checked before actions to avoid operating on a reused PID.
- Lower CPU priority changes how Windows schedules a client when there is competition. **It is not a CPU cap, FPS cap, or RAM/GPU reduction.** Priority changes persist until you restore Normal priority or the client exits. See [Microsoft's priority documentation](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setpriorityclass).
- The manager does not control Roblox graphics quality, render rate, or game assets. The actual clients dominate resource consumption. Use the available in-game graphics/frame-rate settings for those savings. Minimizing a client does not guarantee it will reduce rendering work.

## Local data and implementation

Game shortcuts are stored in `%LOCALAPPDATA%\RobloxInstanceManager\games.json`. Saving writes a temporary file and replaces the old file. Invalid configuration is preserved, and saving is blocked until the file is repaired or moved and the manager restarted. Only one manager is allowed per Windows session to reduce accidental conflicting edits.

Legacy mode creates and owns the historical `ROBLOX_singletonMutex` Windows object on the UI thread. It refuses to enable if Player processes or an existing mutex are detected. It does not modify Roblox's binary or files, inject code, or close Roblox-owned handles. It does not store cookies, passwords, authentication tickets, private-server access codes, or automatically authenticate accounts.

The application uses Rust's standard library, `serde`/`serde_json` for the small settings file, and `windows-sys` for direct Win32 calls. Native UI controls use a Windows message loop. There is no Tokio, Electron, Chromium, Tauri, or external runtime dependency. `Cargo.lock` pins the dependency versions.

## Build and check

On a machine with Rust and Windows linker prerequisites installed:

```powershell
cargo build --release --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

For a checked build and distributable copy:

```powershell
.\build.ps1 -Check
```

This workspace also has a local Rust GNU toolchain in ignored `.tools/`, used automatically by `build.ps1` without changing the global PATH. A copied source folder can use a normal installed Rust MSVC or GNU toolchain. See [Rust's Windows prerequisites](https://rust-lang.github.io/rustup/installation/windows.html).

## Validation and limits

Four automated tests cover unsafe URI-parameter input, settings save/replace/load/corruption handling, live process metrics, and rejection of stale process actions. Formatting and Clippy checks pass. The native interface was inspected on Windows and detected an existing Roblox Player client, including memory and CPU readings; its compatibility guard rejected enabling while that client was open.

Simultaneous multi-account joins have **not** been verified. No claims are made about FPS, GPU savings, a maximum client count, or guaranteed multi-instance compatibility. Existing Roblox sessions were left running during testing. Authentication, live launch success, and protected-process priority/window behavior require testing with your accounts and current Roblox build.
