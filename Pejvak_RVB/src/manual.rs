//! Bundled user manual.
//!
//! The manual is stored as a single self-contained HTML file inside
//! the plugin binary. When the user picks "Manual" from the ☰ menu we
//! write it to a version-tagged file in the user's config directory
//! and hand it to the OS to open in the default browser.
//!
//! # Why HTML, not PDF
//!
//! HTML gives us three things a bundled PDF cannot:
//!   * zero binary dependencies (no `printpdf` / `genpdf` crate),
//!   * full CSS styling that matches the plugin's own dark theme,
//!   * free upgrades — updating the manual is just editing a text
//!     file, with no build-time toolchain for PDF generation.
//!
//! If a user needs a PDF, the browser's own "Print → Save as PDF"
//! produces one from the same source.

use std::path::PathBuf;

/// The manual source, embedded at compile time.
const MANUAL_HTML: &str = include_str!("../../manual/manual.html");

/// The PEJVAK DSP logo, embedded at compile time. Referenced by the
/// manual HTML as `<img src="logo.png">`, so the file must be written
/// alongside the HTML on disk.
const LOGO_PNG: &[u8] = include_bytes!("../../manual/logo.png");

/// Opens the user manual in the OS's default browser.
///
/// The file is written to a version-tagged path in the plugin's
/// config directory, so a plugin update automatically produces a new
/// file (and a browser that was caching the old one picks up the new
/// content the next time the user opens it).
///
/// Silent on failure — if writing or opening the file fails, the
/// plugin keeps running. There is no meaningful recovery path on the
/// audio thread's behalf and no useful UI to show given how rare a
/// failure here is.
pub fn open_in_browser() {
    let Some(path) = write_manual_to_disk() else {
        return;
    };
    open_path(&path);
}

/// Writes the manual and its companion logo to
/// `<config>/PEJVAK RVB/`.
///
/// A fresh HTML file is written on every open, with a nanosecond
/// timestamp in its name, so the browser never serves a cached
/// copy of an older manual. The logo keeps a fixed filename
/// (`logo.png`) because the HTML references it as a sibling file;
/// its bytes are rewritten every time so a plugin update that
/// carries a new logo propagates without needing a manual cleanup.
///
/// Old timestamped `Manual-*.html` files accumulate in the folder.
/// They are harmless (a few dozen KB each) and the user can delete
/// them from the ☰ → Manual folder at any time.
fn write_manual_to_disk() -> Option<PathBuf> {
    let dir = manual_dir()?;
    let version = env!("CARGO_PKG_VERSION");

    // Fixed-filename logo, referenced by the HTML as `logo.png`.
    // Small enough that rewriting it on every open is negligible.
    let logo_path = dir.join("logo.png");
    std::fs::write(&logo_path, LOGO_PNG).ok()?;

    // Version- and time-tagged HTML, so the browser always sees a
    // fresh URL and any edit to `manual.html` is picked up on the
    // next open without a version bump.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir.join(format!("Manual-{}-{}.html", version, stamp));
    std::fs::write(&path, MANUAL_HTML).ok()?;

    Some(path)
}

/// Returns `<config>/PEJVAK RVB/`, creating it if necessary.
///
/// Follows each platform's convention:
///   Windows: `%APPDATA%\PEJVAK RVB\`
///   macOS:   `~/Library/Application Support/PEJVAK RVB/`
///   Linux:   `~/.config/PEJVAK RVB/`
fn manual_dir() -> Option<PathBuf> {
    // Follow each platform's own convention:
    //   Windows: %APPDATA%\PEJVAK RVB\
    //   macOS:   ~/Library/Application Support/PEJVAK RVB/
    //   Linux:   ~/.config/PEJVAK RVB/
    let mut dir = if cfg!(windows) {
        PathBuf::from(std::env::var("APPDATA").ok()?)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var("HOME").ok()?)
            .join("Library")
            .join("Application Support")
    } else {
        PathBuf::from(std::env::var("HOME").ok()?).join(".config")
    };
    dir.push("PEJVAK RVB");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Opens `path` with the OS's default handler.
#[cfg(target_os = "windows")]
fn open_path(path: &std::path::Path) {
    // Use ShellExecuteW directly instead of `cmd /c start`.
    //
    // The previous implementation called:
    //     Command::new("cmd").args(["/c", "start", "", path_str])
    //
    // `cmd.exe` parses the entire command line for metacharacters
    // (`&`, `|`, `^`, `>`, `<`, `%`, `!`) BEFORE `start` ever sees
    // them. A path containing any of those — e.g., a Windows
    // username with `&` in it, or a plugin installed under
    // `C:\Program Files & Data\` — would be interpreted as a
    // separate command and executed. This is a local command
    // injection vulnerability.
    //
    // ShellExecuteW takes a wide (UTF-16) string directly and does
    // NOT go through the shell, so no metacharacter interpretation
    // can happen.
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

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
fn open_path(path: &std::path::Path) {
    let _ = std::process::Command::new("open").arg(path).spawn();
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_path(path: &std::path::Path) {
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}
