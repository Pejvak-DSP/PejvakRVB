//! Bundled user manual.
//!
//! # Why this file uses a pre-spawned worker thread
//!
//! Building locally works, but the same source built by GitHub and
//! downloaded carries Windows' "Mark of the Web" attribute. AV software
//! treats such binaries far more aggressively: it scans the DLL longer
//! on load, hooks `CreateThread` to inspect new thread entry points,
//! and defers process spawns for inspection.
//!
//! Any of those can exceed a DAW's watchdog timeout when triggered
//! from the UI thread. The observed symptom is the ☰ menu freezing,
//! then the DAW being killed by its own unresponsive-plugin watchdog.
//!
//! The defense is architectural, not a band-aid:
//!
//! 1. A **worker thread is spawned once at plugin init**. Clicks
//!    never trigger `thread::spawn`, which is what AV scans.
//!
//! 2. **Clicks are pure memory operations.** `mpsc::Sender::send`
//!    is a lock + memcpy + wakeup; it cannot block on I/O, thread
//!    creation, or shell dispatch. The UI thread returns in
//!    microseconds regardless of what the worker is doing.
//!
//! 3. **File I/O targets `%TEMP%`**, which AV scans far less
//!    aggressively than `%APPDATA%` (roaming, OneDrive, network
//!    shares).
//!
//! 4. **The shell call is `explorer.exe`**, a plain process spawn.
//!    No COM, no STA, no `ShellExecuteW` deadlock.
//!
//! If the worker thread ever hangs (AV blocks a spawn, browser is
//! unresponsive, disk is full), **only the worker is stuck**. The UI
//! stays free. The next click queues silently. The DAW is never
//! at risk.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::OnceLock;

const MANUAL_HTML: &str = include_str!("../../manual/manual.html");
const LOGO_PNG: &[u8] = include_bytes!("../../manual/logo.png");

/// The worker's request channel. `OnceLock` ensures it's initialized
/// exactly once even if `init()` is called from multiple places.
static WORKER_TX: OnceLock<Sender<()>> = OnceLock::new();

/// Call once at plugin init, before any user interaction.
///
/// Idempotent. Safe to call from `Plugin::activate`, from
/// `PejvakEguiApp::build`, or both.
///
/// Spawning the worker here means clicks never pay thread-creation
/// cost, which is one of the things AV can delay on a
/// downloaded-from-the-internet binary.
pub fn init() {
    WORKER_TX.get_or_init(|| {
        let (tx, rx) = channel::<()>();

        // `thread::Builder` (not `thread::spawn`) so a failed spawn
        // is a `Result` instead of a panic. If the OS refuses to
        // create the thread, `open_in_browser` becomes a harmless
        // no-op rather than crashing.
        let _ = std::thread::Builder::new()
            .name("pejvak-manual".to_string())
            .stack_size(64 * 1024) // small, dedicated stack
            .spawn(move || {
                while rx.recv().is_ok() {
                    // Coalesce rapid clicks: if the user clicked
                    // Manual three times while the first request was
                    // still being processed, they get one tab.
                    while rx.try_recv().is_ok() {}

                    // All the slow work happens here, safely off
                    // the UI thread.
                    if let Some(path) = write_manual_to_disk() {
                        open_path(&path);
                    }
                }
            });

        tx
    });
}

/// Opens the user manual in the OS's default browser.
///
/// Returns immediately. All work happens on the worker thread
/// spawned by [`init()`]. If `init()` was never called, this is
/// a harmless no-op.
pub fn open_in_browser() {
    if let Some(tx) = WORKER_TX.get() {
        let _ = tx.send(());
    }
}

fn write_manual_to_disk() -> Option<PathBuf> {
    let dir = manual_dir()?;
    let version = env!("CARGO_PKG_VERSION");

    let logo_path = dir.join("logo.png");
    std::fs::write(&logo_path, LOGO_PNG).ok()?;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir.join(format!("Manual-{}-{}.html", version, stamp));
    std::fs::write(&path, MANUAL_HTML).ok()?;

    Some(path)
}

/// Returns `<temp>/PEJVAK RVB/`, creating it if necessary.
///
/// Uses `%TEMP%` (or the platform equivalent), not `%APPDATA%`:
///
///   * `%APPDATA%` may be a roaming profile on a dead network share
///   * OneDrive sometimes redirects `%APPDATA%` to cloud storage,
///     where file writes take seconds
///   * AV often scans profile folders far more aggressively than
///     `%TEMP%`, because profile files persist across sessions
///
/// The manual is a transient file. `%TEMP%` is the right place.
fn manual_dir() -> Option<PathBuf> {
    let mut dir = std::env::temp_dir();
    dir.push("PEJVAK RVB");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

// ============================================================================
// Per-platform browser open — process spawn only
// ============================================================================

#[cfg(target_os = "windows")]
fn open_path(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            std::ptr::null(),
            wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

#[cfg(target_os = "macos")]
fn open_path(path: &Path) {
    if let Ok(mut child) = std::process::Command::new("open").arg(path).spawn() {
        let _ = child.wait();
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_path(path: &Path) {
    if let Ok(mut child) = std::process::Command::new("xdg-open").arg(path).spawn() {
        let _ = child.wait();
    }
}
