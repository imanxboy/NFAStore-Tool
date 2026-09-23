//! Reading a CS2 account's Premier / Wingman rank and competitive cooldown out
//! of the HTML of Steam's own "Game Coordinator Player Data" page
//! (`steamcommunity.com/profiles/<id>/gcpd/730?tab=matchmaking`).
//!
//! This half is deliberately pure: it takes the page as a string and returns
//! numbers. It fetches nothing and never sees a token. The page it reads is the
//! one Steam renders for the account's *own* owner — the same table the account
//! holder sees on the website — so a reseller can read the state of the stock it
//! already holds. Keeping the parser standalone means it can be tested against
//! saved pages with no network and no account, and it is the piece least likely
//! to change when the way the session behind it is obtained changes.
//!
//! Ported from WareStore's `gcpd_parser.py` (GPL-3.0), matching its table shapes
//! and column heuristics field for field so a page that satisfies one satisfies
//! the other. Written without the `regex` crate to keep this tool's dependency
//! set as small as it already is.
//!
//! The fetch that produces the HTML — a non-destructive CM logon that mints a
//! read-only web cookie — lands as a separate step; until it is wired, these
//! items have no caller outside the tests, hence the module-wide allow.
#![allow(dead_code)]

use serde::Serialize;

/// A permanent competitive ban has no real expiry. A far-future unix second lets
/// "expires in the future" comparisons still read it as active, while staying
/// well under 2^31 so nothing overflows a 32-bit second count downstream.
pub(crate) const COOLDOWN_PERMANENT: i64 = 2_000_000_000;

/// What one account's matchmaking tab tells us. `-1` is "unknown / unranked",
/// kept distinct from a real `0`, so the card can show a blank rather than a
/// false "unranked" when a page simply did not carry the row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Cs2Rank {
    /// Premier CS Rating, e.g. 15750. `-1` when unknown or unranked.
    pub premier_rating: i64,
    pub premier_wins: i64,
    /// Wingman rank 1..=18. `-1` when unknown or unranked.
    pub wingman_rank: i64,
    pub wingman_wins: i64,
    /// Unix second the cooldown clears; `0` for none, or [`COOLDOWN_PERMANENT`]
    /// for a ban with no end.
    pub cooldown_expires_unix: i64,
    pub cooldown_reason: String,
}

impl Default for Cs2Rank {
    fn default() -> Self {
        Self {
            premier_rating: -1,
            premier_wins: -1,
            wingman_rank: -1,
            wingman_wins: -1,
            cooldown_expires_unix: 0,
            cooldown_reason: String::new(),
        }
    }
}

impl Cs2Rank {
    pub fn has_premier(&self) -> bool {
        self.premier_rating > 0
    }

    pub fn has_wingman(&self) -> bool {
        self.wingman_rank > 0
    }

    /// Seconds left on the cooldown at `now`, or `0` if there is none / it has
    /// already cleared. A permanent ban reports its full remaining span rather
    /// than a special value; the caller decides how to label it.
    pub fn cooldown_remaining_secs(&self, now: i64) -> i64 {
        if self.cooldown_expires_unix <= 0 {
            return 0;
        }
        (self.cooldown_expires_unix - now).max(0)
    }

    pub fn is_on_cooldown(&self, now: i64) -> bool {
        self.cooldown_remaining_secs(now) > 0
    }

    pub fn is_permanent_cooldown(&self) -> bool {
        self.cooldown_expires_unix == COOLDOWN_PERMANENT
    }
}

/// Scrape Premier / Wingman and cooldown from the matchmaking-tab HTML. `now` is
/// the current unix second, taken as a parameter so the cooldown logic is
/// testable without a clock.
pub(crate) fn parse_matchmaking(html: &str, now: i64) -> Cs2Rank {
    let mut out = Cs2Rank::default();

    for table in tables(html) {
        if table.len() < 2 {
            continue;
        }
        let header = &table[0];
        if header.is_empty() {
            continue;
        }

        if ci_contains(&header[0], "Cooldown") {
            parse_cooldown_table(&table, now, &mut out);
            continue;
        }

        if ci_contains(&header[0], "Matchmaking Mode") {
            parse_matchmaking_table(header, &table, &mut out);
            continue;
        }
    }

    out
}

/// The cooldown table lists expirations. We keep the earliest one still in the
/// future; a row whose first cell is not a timestamp but whose second cell is a
/// count means an open-ended competitive cooldown.
fn parse_cooldown_table(table: &[Vec<String>], now: i64, out: &mut Cs2Rank) {
    for row in &table[1..] {
        if row.is_empty() {
            continue;
        }
        let ts = parse_timestamp(&row[0]);
        if ts == 0 {
            if !row[0].is_empty() && row.len() > 1 && to_int(&row[1], 0) >= 1 {
                out.cooldown_expires_unix = COOLDOWN_PERMANENT;
                out.cooldown_reason = "Competitive cooldown".to_string();
            }
            continue;
        }
        if out.cooldown_expires_unix == COOLDOWN_PERMANENT {
            continue;
        }
        if ts > now && (out.cooldown_expires_unix == 0 || ts < out.cooldown_expires_unix) {
            out.cooldown_expires_unix = ts;
            out.cooldown_reason = "Competitive cooldown".to_string();
        }
    }
}

/// The matchmaking table carries a row per mode. The Premier and Wingman rows
/// give the rating/rank in a "Skill" column and a "Wins" column, whose positions
/// we read from the header when named and otherwise fall back to Steam's usual
/// layout. Per-map variant tables (which repeat "Matchmaking Mode" but add a Map
/// column) are skipped so a single map's rating never overwrites the real one.
fn parse_matchmaking_table(header: &[String], table: &[Vec<String>], out: &mut Cs2Rank) {
    let is_per_map = header.iter().any(|cell| {
        ["Map", "Mappa", "Mapa", "Carte", "Karte"]
            .iter()
            .any(|name| ci_contains(cell, name))
    });
    if is_per_map {
        return;
    }

    let mut skill_col: Option<usize> = None;
    let mut wins_col: Option<usize> = None;
    for (index, cell) in header.iter().enumerate() {
        if skill_col.is_none() && ci_contains(cell, "Skill") {
            skill_col = Some(index);
        }
        if wins_col.is_none() && ci_contains(cell, "Wins") {
            wins_col = Some(index);
        }
    }
    let skill_col = skill_col.unwrap_or(4);
    let wins_col = wins_col.unwrap_or(1);

    for row in &table[1..] {
        if row.is_empty() {
            continue;
        }
        let skill = row.get(skill_col).map_or(-1, |c| to_int(c, -1));
        let wins = row.get(wins_col).map_or(-1, |c| to_int(c, -1));
        if skill < 0 && wins < 0 {
            continue;
        }
        if ci_contains(&row[0], "Premier") {
            if skill > 0 {
                out.premier_rating = skill;
            }
            if wins >= 0 {
                out.premier_wins = wins;
            }
        } else if ci_contains(&row[0], "Wingman") {
            if skill > 0 {
                out.wingman_rank = skill;
            }
            if wins >= 0 {
                out.wingman_wins = wins;
            }
        }
    }
}

pub(crate) fn looks_like_login_page(html: &str) -> bool {
    ci_contains(html, "g_steamID = false") || ci_contains(html, "<title>Sign In")
}

pub(crate) fn looks_like_gcpd_page(html: &str) -> bool {
    ci_contains(html, "generic_kv_table") || ci_contains(html, "Personal Game Data")
}

// --- HTML scanning, no regex ------------------------------------------------

/// Every `generic_kv_table` on the page, as rows of already-cleaned cell text.
fn tables(html: &str) -> Vec<Vec<Vec<String>>> {
    let mut out = Vec::new();
    for (attrs, inner) in blocks(html, "table") {
        if !ci_contains(attrs, "generic_kv_table") {
            continue;
        }
        let mut rows = Vec::new();
        for (_row_attrs, row_inner) in blocks(inner, "tr") {
            let cells = row_cells(row_inner);
            if !cells.is_empty() {
                rows.push(cells);
            }
        }
        out.push(rows);
    }
    out
}

/// The `<td>`/`<th>` cells of one row, in order, with tags and entities removed.
fn row_cells(row: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut pos = 0;
    loop {
        let td = find_ci(row, "<td", pos);
        let th = find_ci(row, "<th", pos);
        let (start, close) = match (td, th) {
            (Some(a), Some(b)) if a <= b => (a, "</td>"),
            (Some(_), Some(b)) => (b, "</th>"),
            (Some(a), None) => (a, "</td>"),
            (None, Some(b)) => (b, "</th>"),
            (None, None) => break,
        };
        let Some(rel) = row[start..].find('>') else {
            break;
        };
        let inner_start = start + rel + 1;
        let Some(close_at) = find_ci(row, close, inner_start) else {
            break;
        };
        out.push(strip_tags(&row[inner_start..close_at]));
        pos = close_at + close.len();
    }
    out
}

/// Each `<tag ...>inner</tag>` in `hay`, returned as `(open-tag-text, inner)`.
/// GCPD's tables, rows and cells are not nested into themselves, so a first
/// matching close tag is the right one — the same assumption the source parser's
/// regexes make.
fn blocks<'a>(hay: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(start) = find_ci(hay, &open, pos) {
        let after = start + open.len();
        // Guard against `<tablefoo`: the char after the name must end it.
        let ends_name = hay
            .as_bytes()
            .get(after)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/');
        if !ends_name {
            pos = after;
            continue;
        }
        let Some(rel) = hay[after..].find('>') else {
            break;
        };
        let gt = after + rel;
        let inner_start = gt + 1;
        let Some(close_at) = find_ci(hay, &close, inner_start) else {
            break;
        };
        out.push((&hay[start..gt], &hay[inner_start..close_at]));
        pos = close_at + close.len();
    }
    out
}

/// Drop tags, then control whitespace, then decode the handful of entities Steam
/// actually emits — in that order, so an entity that decodes to `<` is never
/// mistaken for a tag.
fn strip_tags(cell: &str) -> String {
    let mut untagged = String::with_capacity(cell.len());
    let mut in_tag = false;
    for ch in cell.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => untagged.push(ch),
            _ => {}
        }
    }

    let mut text: String = untagged
        .chars()
        .filter(|c| !matches!(c, '\n' | '\r' | '\t'))
        .collect();
    for (from, to) in [
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&nbsp;", " "),
    ] {
        text = text.replace(from, to);
    }
    text.trim().to_string()
}

/// Leading signed integer after stripping commas and spaces, or `default`.
fn to_int(cell: &str, default: i64) -> i64 {
    let cleaned: String = cell.chars().filter(|c| *c != ',' && *c != ' ').collect();
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    let negative = match bytes.first() {
        Some(b'+') => {
            i = 1;
            false
        }
        Some(b'-') => {
            i = 1;
            true
        }
        _ => false,
    };
    let digits_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start {
        return default;
    }
    match cleaned[digits_start..i].parse::<i64>() {
        Ok(value) => {
            if negative {
                -value
            } else {
                value
            }
        }
        Err(_) => default,
    }
}

/// Parse a leading `YYYY-MM-DD HH:MM:SS` (UTC) into a unix second, or `0` when
/// the cell is not a timestamp. The GCPD page prints these in UTC.
fn parse_timestamp(cell: &str) -> i64 {
    let bytes = cell.trim_start().as_bytes();
    let mut i = 0;
    let mut fields = [0i64; 6];

    for (field_index, slot) in fields.iter_mut().enumerate() {
        let start = i;
        let mut value = 0i64;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            value = value * 10 + i64::from(bytes[i] - b'0');
            i += 1;
        }
        if i == start {
            return 0;
        }
        *slot = value;

        if field_index < 5 {
            if field_index == 2 {
                // Between the date and the time: one or more whitespace.
                if i >= bytes.len() || !bytes[i].is_ascii_whitespace() {
                    return 0;
                }
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
            } else {
                let expected = if field_index < 2 { b'-' } else { b':' };
                if bytes.get(i) != Some(&expected) {
                    return 0;
                }
                i += 1;
            }
        }
    }

    timegm(
        fields[0], fields[1], fields[2], fields[3], fields[4], fields[5],
    )
}

/// Civil UTC date-time to unix seconds, via Howard Hinnant's `days_from_civil`.
/// No external date crate, and no local-time surprises: the input is UTC.
fn timegm(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    days * 86_400 + hour * 3_600 + minute * 60 + second
}

/// Case-insensitive substring search from `start`, returning a byte offset.
fn find_ci(hay: &str, needle: &str, start: usize) -> Option<usize> {
    let hay = hay.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || start >= hay.len() || needle.len() > hay.len() - start {
        return None;
    }
    let mut i = start;
    while i + needle.len() <= hay.len() {
        if hay[i..i + needle.len()]
            .iter()
            .zip(needle)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn ci_contains(hay: &str, needle: &str) -> bool {
    find_ci(hay, needle, 0).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MATCHMAKING_PAGE: &str = r#"
        <html><body>
        <h1>Personal Game Data</h1>
        <table class="generic_kv_table cs_kv">
          <tr><th>Matchmaking Mode</th><th>Wins</th><th>Ties</th><th>Losses</th><th>Skill</th><th>Last</th></tr>
          <tr><td>Premier</td><td>1,234</td><td>0</td><td>57</td><td>15750</td><td>2026-09-01</td></tr>
          <tr><td>Wingman</td><td>88</td><td>1</td><td>12</td><td>11</td><td>2026-08-20</td></tr>
          <tr><td>Competitive</td><td>500</td><td>3</td><td>200</td><td>0</td><td>2026-07-10</td></tr>
        </table>
        </body></html>
    "#;

    fn page_with_cooldown(rows: &str) -> String {
        format!(
            r#"<body><table class="generic_kv_table">
                <tr><th>Cooldown Expiration</th><th>Count</th></tr>
                {rows}
               </table></body>"#
        )
    }

    #[test]
    fn reads_premier_and_wingman() {
        let rank = parse_matchmaking(MATCHMAKING_PAGE, 1_700_000_000);
        assert_eq!(rank.premier_rating, 15_750);
        assert_eq!(rank.premier_wins, 1_234);
        assert_eq!(rank.wingman_rank, 11);
        assert_eq!(rank.wingman_wins, 88);
        assert!(rank.has_premier() && rank.has_wingman());
        assert_eq!(rank.cooldown_expires_unix, 0);
    }

    #[test]
    fn timegm_matches_a_known_epoch() {
        // 2030-01-01T00:00:00Z is 1_893_456_000 — the anchor that proves the
        // date maths, not just that it returns something.
        assert_eq!(timegm(2030, 1, 1, 0, 0, 0), 1_893_456_000);
    }

    #[test]
    fn a_future_cooldown_is_the_earliest_one() {
        let page = page_with_cooldown(
            "<tr><td>2031-01-01 00:00:00</td><td>1</td></tr>\
             <tr><td>2030-01-01 00:00:00</td><td>1</td></tr>",
        );
        let rank = parse_matchmaking(&page, 1_893_000_000);
        assert_eq!(rank.cooldown_expires_unix, 1_893_456_000); // the 2030 one
        assert!(rank.is_on_cooldown(1_893_000_000));
        assert_eq!(rank.cooldown_reason, "Competitive cooldown");
    }

    #[test]
    fn a_past_cooldown_is_not_a_cooldown() {
        let page = page_with_cooldown("<tr><td>2000-01-01 00:00:00</td><td>1</td></tr>");
        let rank = parse_matchmaking(&page, 1_893_000_000);
        assert_eq!(rank.cooldown_expires_unix, 0);
        assert!(!rank.is_on_cooldown(1_893_000_000));
    }

    #[test]
    fn a_countless_text_row_is_a_permanent_ban() {
        let page = page_with_cooldown("<tr><td>Permanent</td><td>3</td></tr>");
        let rank = parse_matchmaking(&page, 1_893_000_000);
        assert!(rank.is_permanent_cooldown());
        assert!(rank.is_on_cooldown(1_893_000_000));
    }

    #[test]
    fn per_map_table_does_not_overwrite_the_real_rating() {
        let page = format!(
            "{MATCHMAKING_PAGE}
            <table class=\"generic_kv_table\">
              <tr><th>Matchmaking Mode</th><th>Map</th><th>Wins</th><th>Ties</th><th>Losses</th><th>Skill</th></tr>
              <tr><td>Premier</td><td>de_dust2</td><td>3</td><td>0</td><td>1</td><td>999</td></tr>
            </table>"
        );
        let rank = parse_matchmaking(&page, 1_700_000_000);
        assert_eq!(rank.premier_rating, 15_750); // not 999
    }

    #[test]
    fn header_named_columns_win_over_the_defaults() {
        // Skill and Wins in unusual positions; the header must still find them.
        let page = r#"<table class="generic_kv_table">
            <tr><th>Matchmaking Mode</th><th>Skill</th><th>Losses</th><th>Wins</th></tr>
            <tr><td>Premier</td><td>21000</td><td>4</td><td>640</td></tr>
        </table>"#;
        let rank = parse_matchmaking(page, 1_700_000_000);
        assert_eq!(rank.premier_rating, 21_000);
        assert_eq!(rank.premier_wins, 640);
    }

    #[test]
    fn unranked_stays_unknown_not_zero() {
        let page = r#"<table class="generic_kv_table">
            <tr><th>Matchmaking Mode</th><th>Wins</th><th>Ties</th><th>Losses</th><th>Skill</th></tr>
            <tr><td>Premier</td><td>0</td><td>0</td><td>0</td><td>0</td></tr>
        </table>"#;
        let rank = parse_matchmaking(page, 1_700_000_000);
        assert_eq!(rank.premier_rating, -1); // skill 0 is not a rating
        assert_eq!(rank.premier_wins, 0); // but zero wins is a real count
        assert!(!rank.has_premier());
    }

    #[test]
    fn a_sign_in_page_is_recognised() {
        let html =
            "<html><head><title>Sign In</title></head><body>g_steamID = false;</body></html>";
        assert!(looks_like_login_page(html));
        assert!(!looks_like_gcpd_page(html));
    }

    #[test]
    fn entities_and_tags_come_out_clean() {
        assert_eq!(strip_tags("<b>Premier</b>&nbsp;mode"), "Premier mode");
        assert_eq!(to_int("1,234", -1), 1_234);
        assert_eq!(to_int("n/a", -1), -1);
        assert_eq!(parse_timestamp("Competitive cooldown"), 0);
    }
}
