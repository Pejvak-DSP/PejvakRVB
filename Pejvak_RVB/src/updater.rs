//! Self-update checker for PEJVAK RVB.
//!
//! # Flow
//!
//!   1. `check_now()` — hits the GitHub Releases API and compares the
//!      remote tag against `CARGO_PKG_VERSION`.
//!   2. If newer, returns `Available` with the installer URL, the
//!      asset name, and the SHA-256 digest GitHub computed at upload
//!      time.
//!   3. `download_installer()` — downloads the asset, verifies the
//!      SHA-256 hash **while streaming**, and refuses to keep a file
//!      whose hash does not match.
//!   4. `launch_installer()` — opens the verified installer with the
//!      OS default handler. The user runs it after closing the DAW.
//!
//! # Security model
//!
//! * **HTTPS-only transport.** `AgentBuilder::https_only(true)`
//!   refuses any redirect from HTTPS to HTTP. ureq's default
//!   (`https_only = false`) would silently allow a downgrade to
//!   plaintext if the server or an intermediate proxy sent one.
//!
//! * **SHA-256 verification.** GitHub's Releases API returns an
//!   immutable `digest` per asset, computed at upload time. We hash
//!   the download while it streams and abort on mismatch. This
//!   catches CDN corruption, mirror tampering, and DNS-level
//!   redirection to a malicious host.
//!
//! * **No digest → no install.** Releases that predate the `digest`
//!   field (or GitHub Enterprise instances that have not backfilled
//!   it) are rejected with an explicit error rather than falling
//!   back to "install whatever we downloaded".
//!
//! * **Explicit user confirmation** (in the UI layer) is required
//!   before `launch_installer()` is called. The plugin never runs
//!   a downloaded binary without the user clicking through a
//!   checkbox that says "I understand this will run an installer
//!   downloaded from the internet."
//!
//! None of this replaces code signing. For Windows SmartScreen and
//! macOS Gatekeeper to stop warning users, the installer must be
//! Authenticode- and Developer-ID-signed respectively. That is a
//! build-pipeline task, not a runtime one.

use std::io::{Read, Write};
use std::path::PathBuf;

pub const GITHUB_OWNER: &str = "Pejvak-DSP";
pub const GITHUB_REPO: &str = "PejvakRVB";

/// User-Agent header sent to GitHub. Their API rejects requests
/// without one.
const USER_AGENT: &str = concat!("PEJVAK-RVB/", env!("CARGO_PKG_VERSION"));

// ============================================================================
// HTTP agent
// ============================================================================

/// Single agent used for every HTTP call in this module.
///
/// `https_only(true)` is the primary security-relevant choice: ureq's
/// default would follow an HTTPS→HTTP redirect issued by the server
/// (or by an on-path attacker). With `https_only(true)` the request
/// fails instead, so a malicious mirror cannot downgrade the
/// connection to plaintext where it could serve a tampered file.
///
/// We also set a fixed User-Agent here so every request carries the
/// same identifier without repeating the header at each call site.
fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .https_only(true)
        .user_agent(USER_AGENT)
        .build()
}

// ============================================================================
// Update phase (shared with the UI)
// ============================================================================

#[derive(Clone, Debug)]
pub enum UpdatePhase {
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
        notes: String,
        download_url: Option<String>,
        asset_name: Option<String>,
        /// Format: `"sha256:<64 lowercase hex>"`. Comes straight from
        /// GitHub's Releases API and is immutable for a given asset.
        digest: Option<String>,
    },
    Downloading {
        version: String,
    },
    Downloaded {
        version: String,
        path: PathBuf,
    },
    Error(String),
}

/// Return payload of `check_inner` when a newer version is found:
///
/// * `version`       — remote tag, e.g. `"1.0.3"`
/// * `notes`         — the release body (markdown)
/// * `download_url`  — asset URL for the current platform
/// * `asset_name`    — the filename (`installer.exe`, `*.pkg`, …)
/// * `digest`        — `"sha256:<hex>"` from GitHub's Releases API
///
/// Factored into an alias so `check_inner`'s signature does not trip
/// `clippy::type_complexity`.
type CheckResult = Result<
    Option<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )>,
    String,
>;

// ============================================================================
// Check for a new version
// ============================================================================

/// Blocking. Call from a background thread.
pub fn check_now() -> UpdatePhase {
    match check_inner() {
        Ok(Some((version, notes, url, name, digest))) => UpdatePhase::Available {
            version,
            notes,
            download_url: url,
            asset_name: name,
            digest,
        },
        Ok(None) => UpdatePhase::UpToDate,
        Err(e) => UpdatePhase::Error(e),
    }
}

/// Returns `Ok(Some(..))` when a newer version exists, `Ok(None)`
/// when the current version is already the latest, and `Err(..)`
/// for any network or parse failure.
///
/// The tuple layout is `(version, notes, download_url, asset_name,
/// digest)` — the last three are `None` if no platform-matching
/// asset was found in the release.
fn check_inner() -> CheckResult {
    let url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        GITHUB_OWNER, GITHUB_REPO
    );
    let resp = http_agent()
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("HTTP error: {e}"))?;

    let json: serde_json::Value = resp.into_json().map_err(|e| format!("JSON parse: {e}"))?;

    let tag = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing tag_name".to_string())?;

    let remote_ver = tag.trim_start_matches('v');
    let current_ver = env!("CARGO_PKG_VERSION");

    if !is_newer(remote_ver, current_ver) {
        return Ok(None);
    }

    let notes = json
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Find the platform-appropriate asset by substring match on the
    // asset name. GitHub does not expose a structured platform field,
    // so the release workflow must name assets consistently.
    let os = std::env::consts::OS;
    let asset = json
        .get("assets")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|a| {
                let name = a.get("name")?.as_str()?;
                let lower = name.to_lowercase();
                let matches = match os {
                    "windows" => lower.contains("windows") || lower.contains("win"),
                    "macos" => lower.contains("macos") || lower.contains("darwin"),
                    "linux" => lower.contains("linux"),
                    _ => false,
                };
                if matches {
                    let url = a.get("browser_download_url")?.as_str()?.to_string();
                    // GitHub computes this at upload time and never
                    // changes it. Format: "sha256:<64 hex>".
                    let digest = a
                        .get("digest")
                        .and_then(|d| d.as_str())
                        .map(|s| s.to_string());
                    Some((url, name.to_string(), digest))
                } else {
                    None
                }
            })
        });

    let (url, name, digest) = match asset {
        Some((u, n, d)) => (Some(u), Some(n), d),
        None => (None, None, None),
    };

    Ok(Some((remote_ver.to_string(), notes, url, name, digest)))
}

/// Returns `true` if `remote` is a strictly newer semantic version
/// than `current`.
///
/// The comparison is component-wise on numeric segments only. Any
/// suffix (`-rc1`, `-beta.2`, etc.) is ignored because this function
/// is only used to decide whether to *offer* an update — it is never
/// used to auto-install.
fn is_newer(remote: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.split('.')
            .filter_map(|p| p.split(|c: char| !c.is_ascii_digit()).next())
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let r = parse(remote);
    let c = parse(current);
    for i in 0..r.len().max(c.len()) {
        let rv = r.get(i).copied().unwrap_or(0);
        let cv = c.get(i).copied().unwrap_or(0);
        if rv > cv {
            return true;
        }
        if rv < cv {
            return false;
        }
    }
    false
}

// ============================================================================
// Download + verify
// ============================================================================

/// Blocking. Call from a background thread.
///
/// Streams the installer to `%TEMP%/pejvak-rvb-updates/<name>` while
/// computing its SHA-256. On any mismatch (or missing digest) the
/// partial file is deleted and an error is returned, so a corrupted
/// or tampered download never reaches the install step.
///
/// # Errors
///
/// * `expected_digest` is `None` or malformed
/// * The HTTP request fails, times out, or is redirected to HTTP
/// * The downloaded bytes do not match the expected SHA-256
pub fn download_installer(
    url: &str,
    asset_name: &str,
    expected_digest: Option<&str>,
) -> Result<PathBuf, String> {
    // Reject early — never trust the network for a missing digest.
    let Some(expected) = expected_digest else {
        return Err("No SHA-256 digest available for this release. \
             Refusing to install without verification."
            .to_string());
    };

    let expected_hex = expected
        .strip_prefix("sha256:")
        .unwrap_or(expected)
        .to_lowercase();

    if expected_hex.len() != 64 || !expected_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("Malformed SHA-256 digest: {expected}"));
    }

    let resp = http_agent()
        .get(url)
        .call()
        .map_err(|e| format!("HTTP error: {e}"))?;

    let dir = std::env::temp_dir().join("pejvak-rvb-updates");
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
    let path = dir.join(asset_name);

    // Stream + hash in one pass: no second read of the file, no full
    // buffer in memory. Installer files are small, but this keeps
    // memory usage constant regardless of the asset size.
    let mut file = std::fs::File::create(&path).map_err(|e| format!("create: {e}"))?;
    let mut reader = resp.into_reader();
    let mut hasher = sha2::Sha256::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = reader.read(&mut buf).map_err(|e| {
            let _ = std::fs::remove_file(&path);
            format!("read: {e}")
        })?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| {
            let _ = std::fs::remove_file(&path);
            format!("write: {e}")
        })?;
        use sha2::Digest;
        hasher.update(&buf[..n]);
    }
    drop(file);

    use sha2::Digest;
    let actual_hex = format!("{:x}", hasher.finalize());

    if actual_hex != expected_hex {
        let _ = std::fs::remove_file(&path);
        return Err(format!(
            "SHA-256 mismatch — download may be corrupted or tampered with.\n\
             Expected: {}\n\
             Got:      {}",
            expected_hex, actual_hex
        ));
    }

    Ok(path)
}

// ============================================================================
// Launch (user-driven, last step)
// ============================================================================

/// Opens the verified installer with the OS default handler.
///
/// The caller is responsible for having shown a confirmation UI
/// before this is invoked. This function does not verify the hash
/// again — `download_installer` already did — but it also does not
/// accept a path from the network or from the user's text input, so
/// the only way to reach it is through the verified flow above.
pub fn launch_installer(path: &std::path::Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("Installer not found: {}", path.display()));
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new(path)
            .spawn()
            .map_err(|e| format!("launch: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("launch: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("launch: {e}"))?;
    }
    Ok(())
}

/// Opens the project's "latest release" page in the user's browser.
/// Used as a fallback when no platform-matching asset was found in
/// the release (or when the user wants to install manually).
pub fn open_releases_page() {
    let url = format!(
        "https://github.com/{}/{}/releases/latest",
        GITHUB_OWNER, GITHUB_REPO
    );
    let _ = open_url(&url);
}

fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // Same fix as manual.rs::open_path — avoid `cmd /c start`.
        // Currently the URL is a hardcoded GitHub link with no
        // metacharacters, but the pattern is a footgun that will
        // bite if a future URL includes a query string (`?a=1&b=2`).
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let wide: Vec<u16> = std::ffi::OsStr::new(url)
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
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("open: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("open: {e}"))?;
    }
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("1.0.3", "1.0.2"));
        assert!(is_newer("1.1.0", "1.0.9"));
        assert!(is_newer("2.0.0", "1.99.99"));
        assert!(!is_newer("1.0.2", "1.0.2"));
        assert!(!is_newer("1.0.1", "1.0.2"));
        assert!(!is_newer("0.9.9", "1.0.0"));
        // Trailing zero components are equivalent.
        assert!(!is_newer("1.0.0", "1"));
        assert!(!is_newer("1", "1.0.0"));
    }

    #[test]
    fn digest_prefix_parsing() {
        let d = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let stripped = d.strip_prefix("sha256:").unwrap_or(d);
        assert_eq!(stripped.len(), 64);
        assert!(stripped.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn rejects_missing_digest() {
        let err = download_installer("https://example.com/x", "x", None).unwrap_err();
        assert!(err.contains("Refusing to install"));
    }

    #[test]
    fn rejects_malformed_digest() {
        let err = download_installer("https://example.com/x", "x", Some("sha256:not-a-hash"))
            .unwrap_err();
        assert!(err.contains("Malformed SHA-256"));
    }
}
