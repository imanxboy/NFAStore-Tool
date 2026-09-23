# SPDX-License-Identifier: GPL-3.0-or-later
#
# NFAStore Tool — CS2 rank/cooldown fetch sidecar.
#
# Given a refresh token for an account we hold, this reads that account's OWN
# "Game Coordinator Player Data" page from Steam and hands the HTML back to the
# Rust side, which parses it (see src-tauri/src/steam/gcpd.rs). The page is the
# one Steam renders for the account's own owner, so it shows the same rank and
# cooldown the account holder sees on the website.
#
# HOW IT REACHES STEAM. To read that page it first needs a web session, which is
# minted from the refresh token — and that mint (Authentication.
# GenerateAccessTokenForApp) only works over an authenticated Steam CM session,
# not a bare HTTPS call. Steam's CM over TCP is filtered on some connections
# (e.g. from Iran) while its WebSocket CM — which looks like ordinary HTTPS — is
# not. So this connects to the WebSocket CM (wss://.../cmsocket/), the way the
# nfa.pub loader does, and lets ValvePython's `steam` library do the protocol.
#
# The library only speaks TCP, so this supplies a WebSocket transport for it
# (WsConnection) and, because the WebSocket is already TLS-encrypted, tells the
# library the channel is secure without the AES handshake (channel_secured=True,
# no channel_key → messages travel plain inside the TLS frame). The logon fields
# match the loader's exactly; Steam rejects the logon otherwise.
#
# SAFETY, matching the rule held elsewhere: the refresh token is used only to log
# on and to mint a *web* access token with renewal not requested, so it is never
# rotated or consumed; the mint is abandoned if it ever returns a new refresh
# token. The token is read on STDIN (never argv). One JSON line goes to STDOUT.

from __future__ import annotations

# gevent must patch the standard library before anything imports socket/ssl, so
# websocket-client's blocking calls cooperate with the library's greenlets.
from gevent import monkey

monkey.patch_all()

import base64
import hashlib
import json
import logging
import os
import re
import secrets
import struct
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

import gevent
import websocket
from steam.client import SteamClient
from steam.core.connection import Connection
from steam.core.msg import MsgProto
from steam.enums.emsg import EMsg
from steam.steamid import SteamID

logger = logging.getLogger("cs2_rank")

_START = time.monotonic()


def _elapsed() -> str:
    return f"+{time.monotonic() - _START:5.1f}s"


_VAC_RE = re.compile(r"<vacBanned>([01])</vacBanned>", re.IGNORECASE)
_UM_METHOD = "Authentication.GenerateAccessTokenForApp#1"
_HTTP_TIMEOUT = 20
# Keep the fan-out small: retrying many CMs only matters when one does not
# answer, and hammering makes a rate limit worse. A short logon wait keeps a
# non-answering CM from stalling the whole check.
_CM_TAKE = 3
_LOGON_TIMEOUT = 8

# CM logon results that mean the refresh token itself is bad — the account can be
# treated as dead: 5 InvalidPassword, 26 Revoked, 27 Expired, 63 AccountLogonDenied.
# AccessDenied (15) is deliberately NOT here: it also comes up for an account that
# is signed in right now or has been logged on repeatedly, so it is treated as
# transient ("try again") rather than falsely calling a good token dead.
_DEAD_ERESULTS = frozenset({5, 26, 27, 63})


class TokenRejected(Exception):
    def __init__(self, eresult: int):
        super().__init__(str(eresult))
        self.eresult = eresult


class WsConnection(Connection):
    """A WebSocket transport for ValvePython's CMClient. Each WebSocket binary
    frame is exactly one Steam message, so the VT01+length framing the TCP
    transport adds is not used here — the library's serialized message is sent
    as-is and each received frame is one message."""

    def connect(self, server_addr):
        host, port = server_addr
        try:
            self.ws = websocket.create_connection(
                f"wss://{host}:{port}/cmsocket/", timeout=10, enable_multithread=True
            )
        except Exception:  # noqa: BLE001
            return False
        self.server_addr = server_addr
        self.recv_queue.queue.clear()
        self._reader = gevent.spawn(self._reader_loop)
        self._writer = gevent.spawn(self._writer_loop)
        self.event_connected.set()
        return True

    def disconnect(self):
        if not self.event_connected.is_set():
            return
        self.event_connected.clear()
        self.server_addr = None
        for greenlet in (self._reader, self._writer):
            if greenlet:
                greenlet.kill(block=False)
        self._reader = self._writer = None
        self.send_queue.queue.clear()
        self.recv_queue.queue.clear()
        self.recv_queue.put(StopIteration)
        try:
            self.ws.close()
        except Exception:  # noqa: BLE001
            pass

    def _writer_loop(self):
        while True:
            message = self.send_queue.get()
            try:
                self.ws.send_binary(message)
            except Exception:  # noqa: BLE001
                self.disconnect()
                return

    def _reader_loop(self):
        while True:
            try:
                frame = self.ws.recv()
            except Exception:  # noqa: BLE001
                self.disconnect()
                return
            if not frame:
                self.disconnect()
                return
            if isinstance(frame, str):
                frame = frame.encode()
            self.recv_queue.put(frame)


def _clean_token(raw: str) -> str:
    raw = (raw or "").strip()
    if "----" in raw:
        raw = raw.rsplit("----", 1)[-1]
    return raw.strip()


def _jwt_sub(token: str) -> int:
    payload = token.split(".")[1]
    payload += "=" * (-len(payload) % 4)
    return int(json.loads(base64.urlsafe_b64decode(payload))["sub"])


def _machine_id(account_id: str) -> bytes:
    """Steam machine-id KV MessageObject, hashed the way the loader hashes it —
    Steam ties a token to a machine id, and a mismatch is refused (AccessDenied),
    so the exact input strings matter."""

    def cstr(text: str) -> bytes:
        return text.encode("utf-8") + b"\x00"

    def sha(tag: str) -> bytes:
        return cstr(hashlib.sha1(f"SteamUser Hash {tag} {account_id}".encode()).hexdigest())

    return (
        b"\x00" + cstr("MessageObject")
        + b"\x01" + cstr("BB3") + sha("BB3")
        + b"\x01" + cstr("FF2") + sha("FF2")
        + b"\x01" + cstr("3B3") + sha("3B3")
        + b"\x08\x08"
    )


def _cm_servers() -> list[tuple[str, int]]:
    url = (
        "https://api.steampowered.com/ISteamDirectory/GetCMListForConnect/v0001/"
        "?format=json&cellid=0&cmtype=websockets"
    )
    data = json.loads(urllib.request.urlopen(url, timeout=_HTTP_TIMEOUT).read())
    out = []
    for entry in data["response"]["serverlist"]:
        endpoint = entry.get("endpoint", "")
        if ":" in endpoint:
            host, port = endpoint.rsplit(":", 1)
            out.append((host, int(port)))
    return out


def _logon(client: SteamClient, token: str, steamid: int) -> int:
    """Send a ClientLogon carrying the token, return its eresult. The fields
    mirror the nfa.pub loader's; Steam refuses the logon if they differ."""
    message = MsgProto(EMsg.ClientLogon)
    message.header.steamid = SteamID(steamid).as_64
    body = message.body
    body.protocol_version = 65580
    body.client_language = "english"
    body.client_os_type = 16  # Windows 10
    body.should_remember_password = True
    body.obfuscated_private_ip.v4 = 0xA1A2A3A4
    body.machine_id = _machine_id(str(steamid))
    body.chat_mode = 2
    body.machine_name = "unsub-all"
    body.supports_rate_limit_response = True
    body.access_token = token
    client.send(message)
    reply = client.wait_msg(EMsg.ClientLogOnResponse, timeout=_LOGON_TIMEOUT)
    return reply.body.eresult if reply else 0


# ---- CS2 Game Coordinator: the account's Profile Rank (player_level) ----
#
# The GCPD web page carries Premier, Wingman and cooldown, but NOT the profile
# rank (the "Major General Rank 37" the game shows). That number only comes from
# the CS2 Game Coordinator, so — while the same logged-on CM session is open — we
# ask the GC for the account's own profile and read player_level out of it.
#
# GC messages ride inside ordinary client messages (ClientToGC / ClientFromGC).
# Each is [4B msgtype|proto-flag LE][4B header_len LE][proto header][proto body].
# The message ids and the profile field numbers match the nfa.pub loader's, which
# is the reference that this is known to work against a live CS2 GC.
_GC_APP = 730
_GC_HELLO = 4006  # k_EMsgGCClientHello
_GC_WELCOME = 4004  # k_EMsgGCClientWelcome
_MM_CLIENT2GC_HELLO = 9109
_MM_GC2CLIENT_HELLO = 9110
_REQ_PLAYERS_PROFILE = 9127
_PLAYERS_PROFILE = 9128
_CS_CLIENT_VERSION = 2_000_244
_PROTO_MASK = 0x80000000
_GC_DEADLINE = 15  # seconds; the reply usually lands within three


def _wv(n: int) -> bytes:
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def _pv(field: int, value: int) -> bytes:
    return _wv((field << 3) | 0) + _wv(value)


def _pf64(field: int, value: int) -> bytes:
    return _wv((field << 3) | 1) + struct.pack("<Q", value)


def _read_varint(buf: bytes, i: int) -> tuple[int, int]:
    shift = result = 0
    while True:
        b = buf[i]
        i += 1
        result |= (b & 0x7F) << shift
        if not (b & 0x80):
            return result, i
        shift += 7


def _iter_fields(buf: bytes):
    i, n = 0, len(buf)
    while i < n:
        tag, i = _read_varint(buf, i)
        field, wire = tag >> 3, tag & 7
        if wire == 0:
            val, i = _read_varint(buf, i)
            yield field, val
        elif wire == 2:
            ln, i = _read_varint(buf, i)
            yield field, buf[i:i + ln]
            i += ln
        elif wire == 1:
            yield field, buf[i:i + 8]
            i += 8
        elif wire == 5:
            yield field, buf[i:i + 4]
            i += 4
        else:  # groups are not used by these messages
            return


def _account_player_level(body: bytes) -> int | None:
    """player_level (field 17) out of one CMsgGCCStrike15_v2 account profile."""
    for field, val in _iter_fields(body):
        if field == 17 and isinstance(val, int):
            return val
    return None


def _gc_body(payload: bytes) -> bytes:
    """Strip the inner GC proto header, leaving the message body."""
    if len(payload) < 8:
        return b""
    header_len = struct.unpack("<i", payload[4:8])[0]
    if header_len < 0 or len(payload) < 8 + header_len:
        return b""
    return payload[8 + header_len:]


def _send_gc(client: SteamClient, steamid64: int, msgtype: int, body: bytes, jobid: int) -> None:
    header = _pf64(1, steamid64) + _pv(3, _GC_APP) + _pf64(10, jobid)
    packet = struct.pack("<I", msgtype | _PROTO_MASK) + struct.pack("<i", len(header)) + header + body
    message = MsgProto(EMsg.ClientToGC)
    message.body.appid = _GC_APP
    message.body.msgtype = msgtype | _PROTO_MASK
    message.body.payload = bytes(packet)
    client.send(message)


def _fetch_profile_level(client: SteamClient, steamid64: int) -> int | None:
    """Best-effort profile rank for the logged-on account. Never raises — a
    failure just means the level is unknown and the rest of the check stands."""
    account_id = steamid64 & 0xFFFFFFFF
    found: dict[str, int | None] = {"welcome": None, "level": None}

    def on_gc(message):
        try:
            raw = message.body.msgtype & ~_PROTO_MASK
            body = _gc_body(message.body.payload)
            if raw == _GC_WELCOME:
                found["welcome"] = 1
            elif raw == _PLAYERS_PROFILE:
                for field, val in _iter_fields(body):
                    if field == 2 and isinstance(val, (bytes, bytearray)):
                        level = _account_player_level(bytes(val))
                        if level is not None:
                            found["level"] = level
            elif raw == _MM_GC2CLIENT_HELLO and found["level"] is None:
                level = _account_player_level(body)
                if level is not None:
                    found["level"] = level
        except Exception:  # noqa: BLE001
            pass

    try:
        client.on(EMsg.ClientFromGC, on_gc)
        client.games_played([_GC_APP])
        gevent.sleep(1.0)

        jobid = 1
        deadline = time.monotonic() + _GC_DEADLINE
        # Say hello until the GC welcomes us, then ask for our own profile.
        for _ in range(6):
            _send_gc(client, steamid64, _GC_HELLO, _pv(1, _CS_CLIENT_VERSION) + _pv(3, 0) + _pv(4, 0) + _pv(9, 0), jobid)
            jobid += 1
            gevent.sleep(1.2)
            if found["welcome"] or time.monotonic() > deadline:
                break

        _send_gc(client, steamid64, _MM_CLIENT2GC_HELLO, b"", jobid)
        jobid += 1
        _send_gc(client, steamid64, _REQ_PLAYERS_PROFILE, _pv(3, account_id) + _pv(4, 32), jobid)

        while found["level"] is None and time.monotonic() < deadline:
            gevent.sleep(0.3)
    except Exception as exc:  # noqa: BLE001
        logger.warning("%s GC level fetch failed: %s", _elapsed(), exc)
    finally:
        try:
            client.remove_listener(EMsg.ClientFromGC, on_gc)
        except Exception:  # noqa: BLE001
            pass

    logger.info("%s profile level = %s", _elapsed(), found["level"])
    return found["level"]


def mint_web_cookies(refresh_token: str, steamid: int) -> dict | None:
    """Return ``{"cookies": {...}, "level": int | None}`` or None on a transient
    failure. Raises :class:`TokenRejected` when the token is dead. The profile
    rank is read from the GC on the same session and is best-effort — None when
    the GC did not answer in time.

    Non-destructive: the token logs on and mints a web access token with renewal
    not requested; if the mint hands back a rotated refresh token the result is
    thrown away rather than used.
    """
    try:
        servers = _cm_servers()
    except Exception as exc:  # noqa: BLE001
        logger.warning("%s could not fetch the CM list: %s", _elapsed(), exc)
        return None

    rejected: int | None = None
    for host, port in servers[:_CM_TAKE]:
        client = SteamClient()
        client.connection = WsConnection()
        client.cm_servers.clear()
        client.cm_servers.merge_list([(host, port)])
        try:
            logger.info("%s connecting to %s", _elapsed(), host)
            if not client.connect():
                continue
            client.channel_secured = True  # WebSocket is TLS; skip the AES handshake

            eresult = _logon(client, refresh_token, steamid)
            if eresult == 1:
                logger.info("%s logon OK", _elapsed())
                um = client.send_um_and_wait(
                    _UM_METHOD,
                    {"refresh_token": refresh_token, "steamid": steamid},
                    timeout=15,
                )
                if getattr(um.body, "refresh_token", "") if um else "":
                    logger.error("%s mint returned a rotated token — aborting", _elapsed())
                    return None
                access_token = getattr(um.body, "access_token", "") if um else ""
                if not access_token:
                    logger.warning("%s mint returned no access token", _elapsed())
                    return None
                logger.info("%s cookie minted", _elapsed())
                cookies = {
                    "steamLoginSecure": urllib.parse.quote(
                        f"{steamid}||{access_token}", safe=""
                    ),
                    "sessionid": secrets.token_hex(12),
                }
                # Same session, still logged on: ask the GC for the profile rank.
                level = _fetch_profile_level(client, SteamID(steamid).as_64)
                return {"cookies": cookies, "level": level}

            logger.info("%s logon failed (eresult=%s)", _elapsed(), eresult)
            if eresult in _DEAD_ERESULTS:
                rejected = eresult
                break  # a bad token will not recover on another CM
            if eresult == 15:
                # AccessDenied is consistent for this account right now (signed
                # in, or logged on too often); another CM will only repeat it and
                # deepen the rate limit, so stop and report it as transient.
                break
        except Exception as exc:  # noqa: BLE001
            logger.warning("%s CM attempt error: %s", _elapsed(), exc)
        finally:
            try:
                client.disconnect()
            except Exception:  # noqa: BLE001
                pass

    if rejected is not None:
        raise TokenRejected(rejected)
    return None


def fetch_gcpd_html(steamid: int, cookies: dict) -> str | None:
    url = (
        f"https://steamcommunity.com/profiles/{steamid}"
        "/gcpd/730?tab=matchmaking&l=english"
    )
    header = "; ".join(f"{k}={v}" for k, v in cookies.items())
    request = urllib.request.Request(
        url, headers={"Cookie": header, "User-Agent": "Mozilla/5.0"}
    )
    try:
        with urllib.request.urlopen(request, timeout=_HTTP_TIMEOUT) as response:
            html = response.read().decode("utf-8", "replace")
    except Exception as exc:  # noqa: BLE001
        logger.warning("%s GCPD fetch failed: %s", _elapsed(), exc)
        return None
    if "g_steamID = false" in html or "<title>Sign In" in html:
        return None
    if "generic_kv_table" not in html and "Personal Game Data" not in html:
        return None
    return html


def fetch_vac(steamid: int) -> bool | None:
    url = f"https://steamcommunity.com/profiles/{steamid}/?xml=1"
    request = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    try:
        with urllib.request.urlopen(request, timeout=_HTTP_TIMEOUT) as response:
            body = response.read().decode("utf-8", "replace")
    except Exception as exc:  # noqa: BLE001
        logger.warning("%s VAC lookup failed: %s", _elapsed(), exc)
        return None
    match = _VAC_RE.search(body)
    return (match.group(1) == "1") if match else None


def run(refresh_token: str) -> dict:
    token = _clean_token(refresh_token)
    try:
        steamid = _jwt_sub(token)
    except Exception:  # noqa: BLE001
        return {"status": "error", "error": "bad token"}

    logger.info("%s check start for %s", _elapsed(), steamid)

    vac = {}
    vac_thread = threading.Thread(
        target=lambda: vac.__setitem__("banned", fetch_vac(steamid)), daemon=True
    )
    vac_thread.start()

    try:
        session = mint_web_cookies(token, steamid)
    except TokenRejected:
        return {"status": "dead", "error": "rejected"}
    except Exception:  # noqa: BLE001
        logger.exception("unexpected error during cookie mint")
        return {"status": "error", "error": "mint failed"}

    if not session:
        return {"status": "error", "error": "could not reach Steam"}

    cookies = session["cookies"]
    level = session["level"]

    html = fetch_gcpd_html(steamid, cookies)
    if html is None:
        return {"status": "error", "error": "gcpd fetch failed"}

    vac_thread.join(timeout=_HTTP_TIMEOUT)
    logger.info(
        "%s done — %d bytes, vac=%s, level=%s", _elapsed(), len(html), vac.get("banned"), level
    )
    return {"status": "ok", "html": html, "vacBanned": vac.get("banned"), "profileLevel": level}


def _log_file_path() -> str | None:
    try:
        return os.path.join(os.path.dirname(os.path.abspath(sys.executable)), "cs2-rank.log")
    except Exception:  # noqa: BLE001
        return None


def main(argv: list[str]) -> int:
    handlers: list[logging.Handler] = [logging.StreamHandler(sys.stderr)]
    log_path = _log_file_path()
    if log_path:
        try:
            handlers.append(logging.FileHandler(log_path, encoding="utf-8"))
        except OSError:
            pass
    logging.basicConfig(
        level=logging.INFO, handlers=handlers, format="%(asctime)s %(message)s", datefmt="%H:%M:%S"
    )

    if "--selftest" in argv:
        sys.stdout.write(json.dumps({"status": "selftest-ok"}))
        return 0

    token = sys.stdin.read().strip()
    if not token:
        sys.stdout.write(json.dumps({"status": "error", "error": "no token on stdin"}))
        return 0

    sys.stdout.write(json.dumps(run(token)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
