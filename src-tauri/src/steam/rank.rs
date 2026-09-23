//! Fetching one account's CS2 rank and cooldown by running the bundled sidecar.
//!
//! The sidecar (`sidecar/cs2_rank.py`, frozen to `cs2-rank-sidecar.exe`) owns
//! the part Rust does not: a non-destructive CM logon that mints a read-only web
//! cookie and reads the account's own GCPD page. It is handed the token on
//! stdin — never a command-line argument, which other processes can read — and
//! answers with one JSON line. This module resolves it, runs it, and turns that
//! line into the [`Cs2Rank`] the parser already knows how to fill.

use std::io::Write;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};

use super::gcpd::{self, Cs2Rank};

/// Keeps a console window from flashing behind the helper, the same flag the
/// updater and the Steam process calls use.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Where the frozen sidecar sits inside the install, declared under
/// `bundle.resources` in `tauri.conf.json`.
const SIDECAR_RESOURCE: &str = "binaries/cs2-rank-sidecar.exe";

/// The one JSON line the sidecar prints. `html` is present on `ok`; `error`
/// carries the reason on `dead` / `error`.
#[derive(Deserialize)]
struct Envelope {
    status: String,
    html: Option<String>,
    error: Option<String>,
    /// VAC flag from the public profile, alongside the GCPD html. Absent or
    /// null (lookup did not resolve) deserializes to None.
    #[serde(rename = "vacBanned")]
    vac_banned: Option<bool>,
}

pub(crate) fn fetch_rank(app: &AppHandle, token: &str) -> Result<Cs2Rank, String> {
    let exe = app
        .path()
        .resolve(SIDECAR_RESOURCE, BaseDirectory::Resource)
        .map_err(|_| missing())?;
    if !exe.exists() {
        return Err(missing());
    }

    let mut child = Command::new(&exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Could not start the rank helper: {e}"))?;

    // Write the token, then let the handle drop so the child sees end-of-input
    // and starts work; only then collect its output, so a large page cannot
    // deadlock against an unread stdin.
    child
        .stdin
        .take()
        .ok_or_else(|| "Could not reach the rank helper.".to_string())?
        .write_all(token.as_bytes())
        .map_err(|e| format!("Could not hand the token to the rank helper: {e}"))?;

    let output = child
        .wait_with_output()
        .map_err(|e| format!("The rank helper did not finish: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_sidecar_output(&stdout, now_unix())
}

/// Turn the sidecar's JSON line into a rank, or a message. Split out from the
/// process handling so the status handling is testable without a sidecar.
fn parse_sidecar_output(stdout: &str, now: i64) -> Result<Cs2Rank, String> {
    let envelope: Envelope = serde_json::from_str(stdout.trim())
        .map_err(|_| "The rank helper returned something unreadable.".to_string())?;

    match envelope.status.as_str() {
        "ok" => {
            // Rank/cooldown come out of the page; VAC rides alongside it in the
            // envelope, so it is merged in after the pure parse.
            let mut rank = gcpd::parse_matchmaking(&envelope.html.unwrap_or_default(), now);
            rank.vac_banned = envelope.vac_banned;
            Ok(rank)
        }
        // A dead token is worth telling apart from a hiccup: the account's token
        // is genuinely gone, so the reason is surfaced rather than a "try again".
        "dead" => Err(format!(
            "This account's login token is no longer valid ({}). Import it again from your order page.",
            envelope.error.as_deref().unwrap_or("revoked")
        )),
        _ => Err("Could not read this account's rank right now. Try again in a moment.".to_string()),
    }
}

fn missing() -> String {
    "The rank helper is missing from this install. Reinstall NFAStore Tool.".to_string()
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_envelope_is_parsed_into_a_rank() {
        let page = r#"{"status":"ok","vacBanned":false,"html":"<table class=\"generic_kv_table\"><tr><th>Matchmaking Mode</th><th>Wins</th><th>Ties</th><th>Losses</th><th>Skill</th></tr><tr><td>Premier</td><td>640</td><td>0</td><td>4</td><td>21000</td></tr></table>"}"#;
        let rank = parse_sidecar_output(page, 1_700_000_000).expect("ok status should parse");
        assert_eq!(rank.premier_rating, 21_000);
        assert_eq!(rank.premier_wins, 640);
        assert_eq!(rank.vac_banned, Some(false));
    }

    #[test]
    fn vac_ban_and_a_missing_vac_field_are_told_apart() {
        let banned = parse_sidecar_output(r#"{"status":"ok","vacBanned":true,"html":""}"#, 0)
            .expect("ok parses");
        assert_eq!(banned.vac_banned, Some(true));

        // No vacBanned key at all → unknown, not "clean".
        let unknown = parse_sidecar_output(r#"{"status":"ok","html":""}"#, 0).expect("ok parses");
        assert_eq!(unknown.vac_banned, None);
    }

    #[test]
    fn a_dead_token_is_an_error_that_names_itself() {
        let out = parse_sidecar_output(r#"{"status":"dead","error":"Revoked"}"#, 0);
        let message = out.expect_err("dead status is an error");
        assert!(message.contains("Revoked"));
        assert!(message.contains("no longer valid"));
    }

    #[test]
    fn a_transient_error_asks_to_retry() {
        let out = parse_sidecar_output(r#"{"status":"error","error":"gcpd fetch failed"}"#, 0);
        assert!(out.unwrap_err().contains("Try again"));
    }

    #[test]
    fn garbage_output_is_reported_not_panicked() {
        assert!(parse_sidecar_output("not json at all", 0).is_err());
    }
}
