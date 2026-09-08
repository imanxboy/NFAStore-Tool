//! Checking for a new release, and handing the download to the browser.
//!
//! The split is not arbitrary. Measured from an Iranian connection with no VPN,
//! which is the audience this tool is built for:
//!
//! * `api.github.com` answers in ~9s — slow, but it answers.
//! * `release-assets.githubusercontent.com`, where the installer actually
//!   lives, times out with zero bytes. Through a VPN the same range request
//!   returns 64KB in 1.5s.
//!
//! So the app can find out whether an update exists, and cannot reliably fetch
//! it. Downloading in-process would give most customers a progress bar that
//! never moves. The browser already has whatever VPN or proxy they use for the
//! rest of GitHub, so the download is handed to it — one click either way, and
//! the one that works.
//!
//! The version check itself runs in the webview with `fetch`, so this module is
//! only the parts that need the OS: reporting our own version, and opening a
//! link.

use std::os::windows::process::CommandExt;
use std::process::Command;

/// Prevents a console window flashing behind the browser.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The only place a link is allowed to point.
///
/// `open_link` hands a URL to the shell, which would otherwise be a way to
/// launch anything at all from the webview. Pinning the prefix keeps it a
/// button that opens our own releases and nothing else.
const ALLOWED_PREFIX: &str = "https://github.com/imanxboy/NFAStore-Tool/";

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
}
