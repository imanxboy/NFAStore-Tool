// Persistent, app-owned account records (keyed by SteamID64).
//
// Steam's own config files (loginusers.vdf / local.vdf) are volatile — a cache
// reset or a Steam update can wipe the encrypted ConnectCache token, after which
// an account can no longer be signed in without re-importing. Keeping our own copy
// of the token lets every sign-in re-provision Steam from scratch (idempotent and
// recoverable).
//
// The copy is sealed with DPAPI before it touches the disk, scoped to the current
// Windows user. Anything that reads %APPDATA% gets a blob it cannot open, and the
// file is worthless on another machine.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::crypto::{protect_for_user, unprotect_for_user};

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct StoredRecord {
    pub account_name: String,
    #[serde(default)]
    pub persona_name: String,
    /// DPAPI-sealed token, base64. What actually goes to disk.
    #[serde(default)]
    pub token_sealed: String,
    /// Plain token, written by older builds. Read once, then migrated away.
    #[serde(default)]
    pub token: String,
}

/// What the rest of the app works with — the token in the clear, in memory only.
#[derive(Clone, Default)]
pub(crate) struct AccountRecord {
    pub account_name: String,
    pub persona_name: String,
    pub token: String,
}

fn store_path() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base)
        .join("ir.nfastore.tool")
        .join("accounts.json")
}

pub(crate) fn load_records() -> BTreeMap<String, AccountRecord> {
    let Ok(raw) = fs::read_to_string(store_path()) else {
        return BTreeMap::new();
    };
    let stored: BTreeMap<String, StoredRecord> = serde_json::from_str(&raw).unwrap_or_default();

    // A record whose token will not unseal is dropped rather than surfaced with an
    // empty token — that would show a signed-in-able account that silently fails.
    // It happens when the store is copied to another PC or user, where DPAPI is
    // meant to refuse.
    stored
        .into_iter()
        .filter_map(|(steamid, rec)| {
            let token = if !rec.token_sealed.is_empty() {
                unprotect_for_user(&rec.token_sealed).ok()?
            } else if !rec.token.is_empty() {
                rec.token
            } else {
                return None;
            };
            Some((
                steamid,
                AccountRecord {
                    account_name: rec.account_name,
                    persona_name: rec.persona_name,
                    token,
                },
            ))
        })
        .collect()
}

fn write_records(records: &BTreeMap<String, AccountRecord>) -> Result<(), String> {
    let path = store_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create data dir: {e}"))?;
    }

    // Every token is sealed on the way out, so `token` is always empty on disk —
    // rewriting the file is also what migrates a store left over from an older
    // build that wrote tokens in the clear.
    let mut stored: BTreeMap<String, StoredRecord> = BTreeMap::new();
    for (steamid, rec) in records {
        stored.insert(
            steamid.clone(),
            StoredRecord {
                account_name: rec.account_name.clone(),
                persona_name: rec.persona_name.clone(),
                token_sealed: protect_for_user(&rec.token)?,
                token: String::new(),
            },
        );
    }

    let json = serde_json::to_string_pretty(&stored)
        .map_err(|e| format!("Failed to encode accounts store: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write accounts store: {e}"))
}

// Best-effort upsert — a failure to persist must never block the actual login.
pub(crate) fn save_record(steamid: &str, account_name: &str, persona_name: &str, token: &str) {
    let mut records = load_records();
    records.insert(
        steamid.to_string(),
        AccountRecord {
            account_name: account_name.to_string(),
            persona_name: persona_name.to_string(),
            token: token.to_string(),
        },
    );
    let _ = write_records(&records);
}

pub(crate) fn remove_record(steamid: &str) {
    let mut records = load_records();
    if records.remove(steamid).is_some() {
        let _ = write_records(&records);
    }
}
