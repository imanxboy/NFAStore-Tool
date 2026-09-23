//! Checking for a new release, and installing it without leaving the app.
//!
//! This used to hand the download to the browser. The reasoning was measured
//! and real — from an Iranian connection with no VPN, `api.github.com` answers
//! in about nine seconds while the host the installer actually lives on times
//! out with zero bytes — but the result was a button that did a third of a job:
//! open a browser, find the file, run it, then answer a prompt about removing
//! the old version.
//!
//! So the download happens here now. Anyone GitHub will not serve directly is
//! using a VPN for the rest of GitHub anyway, and that VPN covers this request
//! the same as it covers the browser's. When it does fail, the browser is still
//! offered as the fallback rather than being the only route.
//!
//! The version check itself still runs in the webview with `fetch`: it is one
//! small JSON call, and keeping it there means the UI can describe the new
//! version before anything is downloaded.

use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use tauri::{AppHandle, Emitter};

/// Prevents a console window flashing behind the installer.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The only place a link or a download is allowed to point.
///
/// Both `open_link` and the updater hand a URL to the OS — one to the shell,
/// one to an installer we are about to run. Pinning the prefix keeps them
/// buttons that fetch our own releases and nothing else, whatever the webview
/// passes in.
const ALLOWED_PREFIX: &str = "https://github.com/imanxboy/NFAStore-Tool/";

/// Anything far outside the real installer's size is not the installer.
///
/// The build is about 1.9 MB. The floor catches an error page served with a
/// 200; the ceiling stops a hostile or broken response from being read into
/// memory until the machine gives up.
const MIN_INSTALLER_BYTES: u64 = 500 * 1024;
const MAX_INSTALLER_BYTES: u64 = 200 * 1024 * 1024;

pub(crate) fn current_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub(crate) fn open_link(url: &str) -> Result<(), String> {
    if !url.starts_with(ALLOWED_PREFIX) {
        return Err("That link is not part of NFAStore Tool.".to_string());
    }

    // `start` treats its first quoted argument as a window title, so the empty
    // string is load-bearing: without it a URL containing spaces would be
    // swallowed as the title and nothing would open.
    Command::new("cmd")
        .args(["/C", "start", "", url])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|_| "Could not open your browser.".to_string())?;

    Ok(())
}

/// Compare two dotted versions numerically.
///
/// String comparison gets this wrong in the one case that matters: "1.10.0" is
/// older than "1.9.0" alphabetically, so a customer on 1.9.0 would never be
/// told about 1.10.0. Anything unparsable sorts as 0 rather than panicking on a
/// tag somebody typed by hand.
pub(crate) fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.trim()
            .trim_start_matches(['v', 'V'])
            .split(['.', '-', '+'])
            .take_while(|part| part.chars().all(|c| c.is_ascii_digit()) && !part.is_empty())
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };

    // Pad both to the same length and let Vec's lexicographic ordering do the
    // rest — [1, 10, 0] > [1, 9, 0] falls out of it, which is the whole point.
    let (mut a, mut b) = (parse(latest), parse(current));
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    a > b
}

/// Where the downloaded installer is put.
///
/// Its own folder under TEMP, so a half-written file from a failed attempt is
/// overwritten by the next one rather than accumulating, and so nothing else in
/// TEMP is at risk of being mistaken for it.
fn installer_path() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("NFAStore-Tool-update");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not prepare a download folder: {e}"))?;
    Ok(dir.join("nfastore-tool-setup.exe"))
}

/// Fetch the installer, reporting progress as it goes.
///
/// Read in chunks rather than in one call so the UI can show something moving:
/// on a slow connection this is the part that takes the time, and a button that
/// says nothing for a minute reads as a button that did nothing.
fn download(app: &AppHandle, url: &str) -> Result<Vec<u8>, String> {
    let response = ureq::get(url)
        .set("User-Agent", "NFAStore-Tool")
        .call()
        .map_err(|e| format!("Could not reach the download: {e}"))?;

    // Content-Length is advisory — it is missing on some proxies, and the read
    // loop below enforces the real ceiling either way.
    let expected: Option<u64> = response
        .header("Content-Length")
        .and_then(|value| value.parse().ok())
        .filter(|size| *size <= MAX_INSTALLER_BYTES);

    let mut reader = response.into_reader().take(MAX_INSTALLER_BYTES + 1);
    let mut bytes: Vec<u8> = Vec::with_capacity(expected.unwrap_or(2 * 1024 * 1024) as usize);
    let mut chunk = vec![0u8; 64 * 1024];
    let mut last_percent = u8::MAX;

    loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|e| format!("The download stopped early: {e}"))?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);

        if bytes.len() as u64 > MAX_INSTALLER_BYTES {
            return Err("That download is far larger than the installer.".to_string());
        }

        if let Some(total) = expected {
            let percent = ((bytes.len() as u64 * 100) / total.max(1)).min(100) as u8;
            // Only on a change, so a fast connection does not emit hundreds of
            // identical events at the webview.
            if percent != last_percent {
                last_percent = percent;
                let _ = app.emit("update-progress", percent);
            }
        }
    }

    Ok(bytes)
}

/// Download the installer and start it.
///
/// The app exits once the installer is running: NSIS has to replace the very
/// executable this code lives in, and it cannot do that while we hold it open.
/// The installer is given NSIS's passive flags so it puts up a progress bar and
/// asks nothing — the prompt about removing the previous version is exactly
/// what made the old route tiresome.
pub(crate) fn download_and_install(app: &AppHandle, url: &str) -> Result<String, String> {
    if !url.starts_with(ALLOWED_PREFIX) {
        return Err("That download is not part of NFAStore Tool.".to_string());
    }

    let bytes = download(app, url)?;

    // A captive portal or a proxy error page arrives as a perfectly good 200.
    // An installer starts "MZ" and is not tiny; anything else is not the file
    // we asked for and must not be executed.
    if (bytes.len() as u64) < MIN_INSTALLER_BYTES {
        return Err("The download came back too small to be the installer.".to_string());
    }
    if !bytes.starts_with(b"MZ") {
        return Err("The download is not a Windows installer.".to_string());
    }

    let path = installer_path()?;
    std::fs::write(&path, &bytes).map_err(|e| format!("Could not save the installer: {e}"))?;

    // /P is NSIS passive mode: a progress bar and no questions, which is what
    // removes the "uninstall the old version?" step. /R restarts the app once
    // it is done, so the customer lands back where they started.
    Command::new(&path)
        .args(["/P", "/R"])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Could not start the installer: {e}"))?;

    let handle = app.clone();
    std::thread::spawn(move || {
        // Long enough for the webview to paint the closing message, short
        // enough that it does not look stuck. The installer is a separate
        // process by now and outlives us either way.
        std::thread::sleep(std::time::Duration::from_millis(900));
        handle.exit(0);
    });

    Ok("Installing. NFAStore Tool will close and reopen on its own.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_versions_are_detected() {
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(is_newer("v1.1.0", "1.0.9"));
        assert!(is_newer("2.0.0", "1.9.9"));
    }

    #[test]
    fn double_digit_segments_beat_string_order() {
        // The whole reason this is not a string compare.
        assert!(is_newer("1.10.0", "1.9.0"));
        assert!(!is_newer("1.9.0", "1.10.0"));
    }

    #[test]
    fn same_or_older_is_not_an_update() {
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("v1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.0", "1.0.0"));
    }

    #[test]
    fn short_and_ragged_tags_do_not_panic() {
        assert!(is_newer("1.1", "1.0.9"));
        assert!(!is_newer("", "1.0.0"));
        assert!(!is_newer("not-a-version", "1.0.0"));
        assert!(is_newer("1.0.1-beta", "1.0.0"));
    }

    #[test]
    fn links_outside_the_repo_are_refused() {
        assert!(open_link("https://example.com/evil.exe").is_err());
        assert!(open_link("https://github.com/someone-else/tool/releases").is_err());
    }

    #[test]
    fn the_installer_lands_in_its_own_folder() {
        let path = installer_path().expect("temp dir should be writable");
        assert!(path.ends_with("nfastore-tool-setup.exe"));
        assert!(path
            .parent()
            .and_then(|p| p.file_name())
            .is_some_and(|name| name == "NFAStore-Tool-update"));
    }
}
