mod account;
mod config;
mod crypto;
mod downloads;
mod gcpd;
mod import;
mod paths;
mod process;
mod rank;
mod tokens;
mod vdf;

pub use account::{load_steam_accounts, SteamAccount};
pub use import::read_clipboard;
// Crate-internal, so it is re-exported as such: `pub use` on a
// `pub(crate)` item is a compile error, not a widening.
pub(crate) use gcpd::Cs2Rank;
pub(crate) use import::expiry_from_jwt;
use import::write_clipboard;

use std::path::Path;
use std::time::Duration;

use tauri::AppHandle;

use crate::settings::{load_settings, AppSettings};

/// Read an account's CS2 rank and cooldown by running the bundled sidecar.
pub fn fetch_cs2_rank(app: &AppHandle, token: &str) -> Result<Cs2Rank, String> {
    rank::fetch_rank(app, token)
}

/// Recover a login token Steam saved for an account signed in through the Steam
/// client, so an account we hold no token of our own for can still be checked.
pub fn recover_token(account_name: &str) -> Option<String> {
    config::read_connect_cache_token(account_name)
}

pub fn import_from_clipboard() -> Result<String, String> {
    let content = read_clipboard()?;
    handle_batch_import(&content)
}

pub fn handle_batch_import(content: &str) -> Result<String, String> {
    let entries = import::split_batch_payloads(content);
    if entries.is_empty() {
        return Err("Clipboard is empty.".to_string());
    }
    if entries.len() == 1 {
        return import_single_account(&entries[0]);
    }

    let steam_path_string = paths::get_steam_path()?;
    let steam_path = Path::new(&steam_path_string);
    config::check_steam_config_files(&steam_path.join("config"))?;
    process::stop_steam()?;

    let mut imported = 0usize;
    let mut errors = Vec::new();
    let mut last: Option<(String, String)> = None;

    for (idx, entry) in entries.iter().enumerate() {
        match import_account_files(entry, steam_path) {
            Ok((username, steamid)) => {
                imported += 1;
                last = Some((username, steamid));
            }
            Err(e) => errors.push(format!("#{}: {}", idx + 1, e)),
        }
    }

    if imported == 0 {
        return Err(errors.join(" | "));
    }

    if let Some((username, steamid)) = last {
        let settings = load_settings();
        apply_active_account(&username, &steamid, steam_path, &settings)?;
        relaunch_steam(&steam_path_string, &settings)?;
    }

    let mut msg = format!("Imported {imported} accounts. Starting Steam.");
    if !errors.is_empty() {
        msg.push_str(&format!(" {} failed.", errors.len()));
    }
    Ok(msg)
}

fn import_account_files(entry: &str, steam_path: &Path) -> Result<(String, String), String> {
    let (username, jwt) = import::parse_clipboard(entry)?;
    let steamid = import::extract_steamid_from_jwt(&jwt)?;
    config::write_account_files(&username, &jwt, &steamid, steam_path)?;
    tokens::save_record(&steamid, &username, &username, &jwt);
    Ok((username, steamid))
}

fn import_single_account(content: &str) -> Result<String, String> {
    let (username, jwt) = import::parse_clipboard(content)?;
    let steamid = import::extract_steamid_from_jwt(&jwt)?;

    let steam_path_string = paths::get_steam_path()?;
    let steam_path = Path::new(&steam_path_string);
    config::check_steam_config_files(&steam_path.join("config"))?;
    process::stop_steam()?;

    config::write_account_files(&username, &jwt, &steamid, steam_path)?;
    tokens::save_record(&steamid, &username, &username, &jwt);
    let settings = load_settings();
    apply_active_account(&username, &steamid, steam_path, &settings)?;
    relaunch_steam(&steam_path_string, &settings)?;

    Ok(format!("Imported {username}. Starting Steam."))
}

pub fn handle_login_account(account: &SteamAccount) -> Result<String, String> {
    let steam_path_string = paths::get_steam_path()?;
    let steam_path = Path::new(&steam_path_string);
    let settings = load_settings();

    process::stop_steam()?;

    // Re-provision the ConnectCache token + config.vdf from our stored copy on
    // every sign-in, so switching works even if Steam's cache was cleared since
    // import. Accounts imported before token persistence fall back to the flip.
    if let Some(jwt) = &account.token {
        config::check_steam_config_files(&steam_path.join("config"))?;
        config::write_account_files(&account.account_name, jwt, &account.steamid, steam_path)?;
    }

    apply_active_account(&account.account_name, &account.steamid, steam_path, &settings)?;
    relaunch_steam(&steam_path_string, &settings)?;

    Ok(format!(
        "Signed in as {}. Starting Steam.",
        account.display_name()
    ))
}

/// Copy an account's login token back to the clipboard.
///
/// The token is the account. Handing it to a friend, or keeping a copy once the
/// order page has scrolled out of reach, is an ordinary thing for the person
/// who bought it to want — and after an import this app is the only place it
/// still exists in the clear.
///
/// The value goes straight from our own store to the clipboard: it is never
/// returned to the webview, never logged, and never written anywhere else.
pub fn handle_copy_token(account: &SteamAccount) -> Result<String, String> {
    let token = account
        .token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            // Accounts imported before the app kept tokens, and accounts whose
            // sealed copy will not open on this Windows user, both land here.
            "No token is stored for this account. Import it again from your order page to keep a copy."
                .to_string()
        })?;

    // Copy as `steamid----token`, the delivery/order-line format. A bare JWT
    // signs in here (Import accepts it), but other loaders and a friend's tool
    // expect the steamid prefix, so the portable form is the one to hand out.
    write_clipboard(&format!("{}----{}", account.steamid, token))?;
    Ok("Login token copied to the clipboard.".to_string())
}

/// Remove an account from the list, and scrub it from Steam where we can.
///
/// The order matters and it used to be the wrong way round. Scrubbing Steam
/// came first and any failure aborted the whole thing, so an account Steam had
/// already dropped — deleted inside Steam, or left behind by a reinstall — could
/// never be removed here: the delete died on "not found in loginusers.vdf"
/// before it reached our own records, and Refresh could not help because the row
/// was coming from those records in the first place.
///
/// Our record is the list. Removing it is the operation, and it always happens.
/// Everything Steam-side is best-effort cleanup around it, reported but never
/// fatal, because a Steam that is uninstalled, moved, running or simply out of
/// sync is not a reason to trap a dead row in the customer's list forever.
pub fn handle_delete_account(account: &SteamAccount) -> Result<String, String> {
    let mut warning: Option<String> = None;

    match paths::get_steam_path() {
        Ok(steam_path_string) => {
            let steam_path = Path::new(&steam_path_string);

            // Steam rewrites these files on exit, so edits only stick once it is
            // stopped. If it will not stop, skip the file work rather than make
            // changes Steam is about to undo.
            if let Err(err) = process::stop_steam() {
                warning = Some(err);
            } else {
                let config_dir = steam_path.join("config");

                if let Err(err) =
                    config::remove_loginuser(&config_dir.join("loginusers.vdf"), &account.steamid)
                {
                    if warning.is_none() {
                        warning = Some(err);
                    }
                }

                let config_vdf = config_dir.join("config.vdf");
                if config_vdf.exists() {
                    if let Err(err) = config::remove_config_account(&config_vdf, &account.steamid) {
                        if warning.is_none() {
                            warning = Some(err);
                        }
                    }
                }

                if let Ok(local_dir) = paths::local_steam_cache_path() {
                    let local_vdf = local_dir.join("local.vdf");
                    if local_vdf.exists() {
                        let crc = crypto::compute_crc32(&account.account_name);
                        if let Ok(content) = std::fs::read_to_string(&local_vdf) {
                            let updated = config::remove_connect_cache_entry(&content, &crc);
                            let _ = std::fs::write(&local_vdf, updated);
                        }
                    }
                }

                process::clear_autologin_if_matches(&account.account_name);
            }
        }
        Err(err) => warning = Some(err),
    }

    // The one step that defines the operation.
    tokens::remove_record(&account.steamid);

    Ok(match warning {
        Some(err) => format!(
            "Removed {} from the list. Steam was not tidied up: {err}",
            account.display_name()
        ),
        None => format!("Removed {}.", account.display_name()),
    })
}

pub fn handle_clear_steam() -> Result<String, String> {
    // Non-destructive: wipe only the cached login tokens (local.vdf ConnectCache),
    // which signs Steam out, while leaving the rest of %LOCALAPPDATA%\Steam and the
    // account list intact. Saved accounts can be signed back in from their stored
    // token, so this is recoverable rather than a full re-import.
    process::stop_steam()?;
    let base_path = paths::local_steam_cache_path()?;
    config::clear_login_cache(&base_path)?;
    Ok("Steam login cache cleared.".to_string())
}

fn apply_active_account(
    username: &str,
    steamid: &str,
    steam_path: &Path,
    settings: &AppSettings,
) -> Result<(), String> {
    let loginusers_vdf = steam_path.join("config").join("loginusers.vdf");
    config::update_loginusers_vdf(&loginusers_vdf, username, steamid)?;
    config::apply_localconfig_settings(steamid, steam_path, settings)?;
    if settings.cancel_downloads_on_login {
        // Best-effort: defer background game updates so signing in doesn't kick
        // off downloads. Runs while Steam is dead (we just stopped it).
        downloads::defer_game_updates(steam_path);
    }
    process::write_autologin_user(username)
}

fn relaunch_steam(steam_path: &str, settings: &AppSettings) -> Result<(), String> {
    std::thread::sleep(Duration::from_millis(400));
    process::launch_steam(steam_path, settings)?;
    Ok(())
}
