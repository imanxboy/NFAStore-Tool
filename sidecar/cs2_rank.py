# SPDX-License-Identifier: GPL-3.0-or-later
#
# NFAStore Tool — CS2 rank/cooldown fetch sidecar.
#
# Given a refresh token for an account we hold, this fetches that account's OWN
# "Game Coordinator Player Data" page from Steam and hands the HTML back to the
# Rust side, which parses it (see src-tauri/src/steam/gcpd.rs). The page is the
# one Steam renders for the account's own owner — the same rank/cooldown table
# the account holder sees on the website — so the tool can show the state of the
# stock it already holds.
#
# It is bundled as a standalone .exe by PyInstaller and invoked by the app with
# the token on STDIN (never argv — argv is readable by other processes). Its only
# output on STDOUT is one JSON line; all logging goes to STDERR.
#
# SAFETY, load-bearing and matching the rule already enforced elsewhere in this
# tool: the refresh token is used ONLY for a CM logon and to mint a *web* access
# token with renewal disabled. It is NEVER rotated or consumed — this never sets
# a renewal flag and never calls /jwt/finalizelogin or any token-killing HTTP
# endpoint. The mint below asserts that the CM did not hand back a rotated token
# and aborts if it somehow did. This logic is ported from WareStore's
# cs2_cm_mint.py / gcpd_scrape_gateway.py (GPL-3.0).

from __future__ import annotations

import base64
import hashlib
import json
import logging
import re
import secrets
import sys
import urllib.error
import urllib.parse
import urllib.request

logger = logging.getLogger("cs2_rank")

_VAC_RE = re.compile(r"<vacBanned>([01])</vacBanned>", re.IGNORECASE)

_UM_METHOD = "Authentication.GenerateAccessTokenForApp#1"
_PROTOCOL_VERSION = 65580
_CM_ATTEMPTS = 3
_GCPD_TIMEOUT = 20

# CM logon results that mean the refresh token itself is bad/revoked — the caller
# can treat the account as dead. Everything else (TryAnotherCM, ServiceUnavailable,
# timeouts, "no response") is transient and must NEVER flag an account.
_REJECTED_ERESULTS = frozenset({"InvalidPassword", "Expired", "Revoked", "AccessDenied"})


class TokenRejected(Exception):
    """The CM logon rejected the refresh token (bad / revoked / expired)."""

    def __init__(self, eresult: str):
        super().__init__(eresult)
        self.eresult = eresult


def _clean_token(raw: str) -> str:
    """Bare JWT from the app's ``username----<JWT>`` line (or a plain JWT). With
    the prefix left on, the CM rejects the logon as InvalidPassword."""
    raw = (raw or "").strip()
    if "----" in raw:
        raw = raw.rsplit("----", 1)[-1]
    return raw.strip()


def _jwt_sub(token: str) -> int:
    payload = token.split(".")[1]
    payload += "=" * (-len(payload) % 4)
    return int(json.loads(base64.urlsafe_b64decode(payload))["sub"])


def _machine_id(seed: str) -> bytes:
    """Steam machine-id KV MessageObject with three sha1 hashes (values arbitrary
    but stable per account, so a switch does not look like a new machine)."""

    def cstr(text: str) -> bytes:
        return text.encode("utf-8") + b"\x00"

    def sha(tag: str) -> bytes:
        return cstr(hashlib.sha1((tag + seed).encode()).hexdigest())

    return (
        b"\x00" + cstr("MessageObject")
        + b"\x01" + cstr("BB3") + sha("BB3")
        + b"\x01" + cstr("FF2") + sha("FF2")
        + b"\x01" + cstr("3B3") + sha("3B3")
        + b"\x08\x08"
    )


def _token_logon(client, refresh_token: str, steamid: int):
    """Secure the channel, then send a ClientLogon carrying the refresh token in
    access_token (field 108). Returns the ClientLogOnResponse or None."""
    from steam.core.msg import MsgProto
    from steam.enums import EResult
    from steam.enums.emsg import EMsg
    from steam.steamid import SteamID

    if client._pre_login() != EResult.OK:  # waits for the channel to be secured
        return None
    msg = MsgProto(EMsg.ClientLogon)
    msg.header.steamid = SteamID(steamid).as_64
    body = msg.body
    body.protocol_version = _PROTOCOL_VERSION
    body.client_os_type = 20  # Windows 10
    body.client_language = "english"
    body.should_remember_password = True
    body.supports_rate_limit_response = True
    body.chat_mode = 2
    body.machine_name = ""
    try:
        body.obfuscated_private_ip.v4 = 0
    except Exception:  # noqa: BLE001 - field shape varies by proto build
        pass
    body.machine_id = _machine_id(str(steamid))
    body.access_token = refresh_token
    client.send(msg)
    return client.wait_msg(EMsg.ClientLogOnResponse, timeout=30)


def mint_web_cookies(refresh_token: str) -> dict | None:
    """Return ``{"steamLoginSecure": ..., "sessionid": ...}`` or None on a
    transient failure. Raises :class:`TokenRejected` when the token is dead.

    Non-destructive: the refresh token is used only to log on and to mint a web
    access token with renewal disabled; it is never rotated.
    """
    token = _clean_token(refresh_token)
    if not token:
        return None
    try:
        steamid = _jwt_sub(token)
    except Exception:  # noqa: BLE001
        logger.warning("could not decode steamid from the refresh token")
        return None

    from steam.client import SteamClient
    from steam.enums import EResult

    client = SteamClient()
    try:
        resp = None
        rejected = None
        for attempt in range(1, _CM_ATTEMPTS + 1):
            if not client.connected and client.connect() is None:
                logger.info("attempt %d: could not connect to a CM", attempt)
                continue
            reply = _token_logon(client, token, steamid)
            if reply is not None and reply.body.eresult == EResult.OK:
                resp = reply
                break
            reason = (
                EResult(reply.body.eresult).name
                if reply is not None and reply.body.eresult in EResult._value2member_map_
                else "no response"
            )
            logger.info("attempt %d: logon failed (%s)", attempt, reason)
            try:
                client.disconnect()
            except Exception:  # noqa: BLE001
                pass
            if reason in _REJECTED_ERESULTS:
                rejected = reason  # a bad token will not recover on retry
                break

        if resp is None:
            if rejected is not None:
                raise TokenRejected(rejected)
            logger.warning("CM logon failed after %d attempts", _CM_ATTEMPTS)
            return None

        # Mint the web access token over the AUTHENTICATED session. Only
        # refresh_token + steamid are sent — no renewal_type, so the CM leaves
        # the refresh token untouched.
        um = client.send_um_and_wait(
            _UM_METHOD, {"refresh_token": token, "steamid": steamid}, timeout=15
        )
        if um is None or um.header.eresult != EResult.OK:
            reason = (
                EResult(um.header.eresult).name
                if um is not None and um.header.eresult in EResult._value2member_map_
                else "no response"
            )
            logger.warning("GenerateAccessTokenForApp failed (%s)", reason)
            return None

        if getattr(um.body, "refresh_token", ""):  # must never happen
            logger.error("CM returned a rotated refresh token — aborting to be safe")
            return None
        access_token = getattr(um.body, "access_token", "") or ""
        if not access_token:
            logger.warning("mint returned no access token")
            return None

        logger.info("web cookie minted for %s (non-destructive)", steamid)
        return {
            "steamLoginSecure": urllib.parse.quote(f"{steamid}||{access_token}", safe=""),
            "sessionid": secrets.token_hex(12),
        }
    finally:
        try:
            client.disconnect()
        except Exception:  # noqa: BLE001
            pass


def fetch_gcpd_html(steamid: int, cookies: dict) -> str | None:
    """Read-only GET of the account's own matchmaking GCPD page. Returns the HTML,
    or None if the page did not authenticate (so the caller can retry)."""
    url = (
        f"https://steamcommunity.com/profiles/{steamid}"
        "/gcpd/730?tab=matchmaking&l=english"
    )
    header = "; ".join(f"{k}={v}" for k, v in cookies.items())
    request = urllib.request.Request(
        url, headers={"Cookie": header, "User-Agent": "Mozilla/5.0"}
    )
    try:
        with urllib.request.urlopen(request, timeout=_GCPD_TIMEOUT) as response:
            html = response.read().decode("utf-8", "replace")
    except Exception as exc:  # noqa: BLE001
        logger.warning("GCPD fetch failed: %s", exc)
        return None
    # The Rust parser makes the same two checks; failing fast here keeps a login
    # redirect from being reported as "unranked".
    if "g_steamID = false" in html or "<title>Sign In" in html:
        logger.warning("GCPD page came back as the sign-in page")
        return None
    if "generic_kv_table" not in html and "Personal Game Data" not in html:
        logger.warning("response was not the GCPD page")
        return None
    return html


def fetch_vac(steamid: int) -> bool | None:
    """VAC ban flag from the public community profile XML.

    VAC status is public — it needs no login and is not on the GCPD page, so it
    is read separately here from `/profiles/<id>/?xml=1`, which every account
    exposes whether or not the profile is private. None means the lookup did not
    resolve (kept apart from a confirmed clean account).
    """
    url = f"https://steamcommunity.com/profiles/{steamid}/?xml=1"
    request = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    try:
        with urllib.request.urlopen(request, timeout=_GCPD_TIMEOUT) as response:
            body = response.read().decode("utf-8", "replace")
    except Exception as exc:  # noqa: BLE001
        logger.warning("VAC lookup failed: %s", exc)
        return None
    match = _VAC_RE.search(body)
    if match is None:
        logger.warning("VAC field not present in the profile XML")
        return None
    return match.group(1) == "1"


def run(refresh_token: str) -> dict:
    """The whole flow, as a JSON-able envelope. Never raises."""
    try:
        cookies = mint_web_cookies(refresh_token)
    except TokenRejected as rejected:
        return {"status": "dead", "error": rejected.eresult}
    except Exception:  # noqa: BLE001
        logger.exception("unexpected error during cookie mint")
        return {"status": "error", "error": "mint failed"}

    if not cookies or not cookies.get("steamLoginSecure"):
        return {"status": "error", "error": "cookie mint failed"}

    try:
        steamid = _jwt_sub(_clean_token(refresh_token))
    except Exception:  # noqa: BLE001
        return {"status": "error", "error": "bad token"}

    html = fetch_gcpd_html(steamid, cookies)
    if html is None:
        return {"status": "error", "error": "gcpd fetch failed"}
    return {"status": "ok", "html": html, "vacBanned": fetch_vac(steamid)}


def main(argv: list[str]) -> int:
    logging.basicConfig(
        level=logging.INFO,
        stream=sys.stderr,
        format="cs2-rank: %(message)s",
    )

    # A network-free check that the frozen exe runs and its imports resolve; used
    # by CI so a broken build fails there rather than on a customer's machine.
    if "--selftest" in argv:
        from steam.client import SteamClient  # noqa: F401

        sys.stdout.write(json.dumps({"status": "selftest-ok"}))
        return 0

    token = sys.stdin.read().strip()
    if not token:
        sys.stdout.write(json.dumps({"status": "error", "error": "no token on stdin"}))
        return 0

    result = run(token)
    sys.stdout.write(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
