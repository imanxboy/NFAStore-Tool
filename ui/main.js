const invoke = window.__TAURI__.core.invoke;
const listen = window.__TAURI__.event.listen;

const el = (id) => document.getElementById(id);
const accountList = el("accountList");
const detailPane = el("detailPane");
const workspace = el("workspace");
const accountCount = el("accountCount");
const emptyState = el("emptyState");
const toastWrap = el("toastWrap");

let accounts = [];
// Keyed by steamid: {loading} | {data: Cs2Rank} | {error}. Kept across renders
// so a rank a customer pulled stays put until they ask again or reopen the app.
let ranks = {};
// Which account the detail panel is showing.
let selectedSteamid = null;
let confirmHandler = null;
let settings = {
  always_invisible: true,
  cancel_downloads_on_login: false,
  streamer_mode: false,
  launch_steam_minimized: false,
  mute_notifications_on_login: false,
};

/** The far-future second the parser uses for a ban with no end (see gcpd.rs). */
const COOLDOWN_PERMANENT = 2000000000;

/**
 * How long the login token has left, in words.
 *
 * The date comes out of the token itself, so it is the moment the account stops
 * being reachable — not a guess and not a warranty date. Anything inside a
 * fortnight is worth warning about, and an expired one is worth saying plainly
 * rather than printing a date in the past and leaving the customer to work it
 * out.
 */
function tokenExpiry(seconds) {
  if (!seconds) return null;

  const when = new Date(seconds * 1000);
  if (Number.isNaN(when.getTime())) return null;

  const date = when.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
  const daysLeft = Math.floor((when.getTime() - Date.now()) / 86400000);

  if (daysLeft < 0) return { text: `Token expired ${date}`, level: "gone" };
  if (daysLeft === 0) return { text: "Token expires today", level: "soon" };
  if (daysLeft <= 14) return { text: `Token expires in ${daysLeft} day${daysLeft === 1 ? "" : "s"}`, level: "soon" };
  return { text: `Token valid until ${date}`, level: "ok" };
}

/**
 * The cooldown, in words, with a severity so the chip can carry the colour.
 *
 * 0 is a confirmed clear account; the sidecar already turned an expired
 * cooldown into 0, so anything positive is really pending. The far-future
 * sentinel is a ban with no end.
 */
function cooldownText(unix) {
  if (!unix) return { text: "No cooldown", level: "ok" };
  if (unix === COOLDOWN_PERMANENT) return { text: "Permanent ban", level: "gone" };
  const remaining = unix * 1000 - Date.now();
  if (remaining <= 0) return { text: "No cooldown", level: "ok" };
  const mins = Math.floor(remaining / 60000);
  if (mins < 60) return { text: `Cooldown ${Math.max(1, mins)}m`, level: "soon" };
  const hours = Math.floor(mins / 60);
  if (hours < 48) return { text: `Cooldown ${hours}h`, level: "soon" };
  return { text: `Cooldown ${Math.floor(hours / 24)}d`, level: "gone" };
}

/** Token expiry as a detail-row value (label supplies the word "Token"). */
function tokenRowValue(seconds) {
  const e = tokenExpiry(seconds);
  if (!e) return { text: "—", level: "dim" };
  const text = e.text.replace(/^Token /, "").replace(/^./, (c) => c.toUpperCase());
  return { text, level: e.level };
}

/** VAC as a detail-row value. null (unresolved) is not the same as clean. */
function vacRowValue(v) {
  if (v === true) return { text: "Banned", level: "gone" };
  if (v === false) return { text: "None", level: "ok" };
  return { text: "—", level: "dim" };
}

function toast(message, kind = "ok") {
  const node = document.createElement("div");
  node.className = "toast " + kind;
  node.textContent = message;
  toastWrap.appendChild(node);
  setTimeout(() => {
    node.style.opacity = "0";
    node.style.transition = "opacity 0.2s ease";
    setTimeout(() => node.remove(), 220);
  }, 3500);
}

function formatError(e) {
  return typeof e === "string" ? e : String(e);
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c])
  );
}
function escapeAttr(s) {
  return escapeHtml(s);
}

function displayAccount(acc, index) {
  if (!settings.streamer_mode) return acc;
  return {
    ...acc,
    display_name: `Account ${index + 1}`,
    account_name: "••••",
  };
}

function avatarHtml(view, cls) {
  return view.avatar
    ? `<div class="avatar ${cls}"><img src="${escapeAttr(view.avatar)}" alt="" /></div>`
    : `<div class="avatar ${cls}">${escapeHtml(view.initials)}</div>`;
}

function render() {
  accountCount.textContent = accounts.length ? String(accounts.length) : "";

  if (accounts.length === 0) {
    workspace.classList.add("is-empty");
    accountList.innerHTML = "";
    detailPane.innerHTML = "";
    return;
  }
  workspace.classList.remove("is-empty");

  // Keep the selection valid; default to the last-used account.
  if (!accounts.some((a) => a.steamid === selectedSteamid)) {
    selectedSteamid = (accounts.find((a) => a.most_recent) || accounts[0]).steamid;
  }

  renderList();
  renderDetail();
}

function renderList() {
  accountList.innerHTML = accounts
    .map((acc, index) => {
      const view = displayAccount(acc, index);
      const tag = acc.most_recent ? '<span class="li-tag">Last used</span>' : "";
      const selected = acc.steamid === selectedSteamid ? " selected" : "";
      // A checked account carries its status as a hint, unless on stream:
      // whether it signs in (and any cooldown), or that its token is gone.
      const r = ranks[acc.steamid];
      let hint = "";
      if (!settings.streamer_mode && r) {
        if (r.loading) {
          hint = `<span class="li-hint dim">Checking…</span>`;
        } else if (r.error) {
          const dead = /no longer valid|no token is stored|no saved login/i.test(r.error);
          hint = `<span class="li-hint gone">${dead ? "Token expired" : "Check failed"}</span>`;
        } else if (r.data) {
          const cd = cooldownText(r.data.cooldownExpiresUnix);
          // A successful check means the token signed in; show a cooldown or ban
          // when there is one, otherwise a plain confirmation.
          hint =
            cd.level === "ok"
              ? `<span class="li-hint ok">Signs in</span>`
              : `<span class="li-hint ${cd.level}">${escapeHtml(cd.text)}</span>`;
        }
      }
      return `
        <button class="li${selected}" data-select="${escapeAttr(acc.steamid)}">
          ${avatarHtml(view, "")}
          <span class="li-info">
            <span class="li-name"><span>${escapeHtml(view.display_name)}</span>${tag}</span>
            <span class="li-sub">${escapeHtml(view.account_name)}</span>
            ${hint}
          </span>
        </button>`;
    })
    .join("");
}

/** One stat box (Premier / CS2 Level) for the detail panel. */
function statBox(label, state, value, note) {
  const known = state === "value";
  const valueClass = known ? "" : " dim";
  return `
    <div class="stat-box">
      <div class="stat-box-label">${escapeHtml(label)}</div>
      <div class="stat-box-value${valueClass}">${escapeHtml(value)}</div>
      <div class="stat-box-note">${escapeHtml(note)}</div>
    </div>`;
}

function renderDetail() {
  const acc = accounts.find((a) => a.steamid === selectedSteamid);
  if (!acc) {
    detailPane.innerHTML = '<div class="detail-empty">Select an account.</div>';
    return;
  }
  const index = accounts.indexOf(acc);
  const view = displayAccount(acc, index);
  const streamer = settings.streamer_mode;
  const sid = escapeAttr(acc.steamid);
  const r = ranks[acc.steamid];
  const d = r && r.data;
  const loading = Boolean(r && r.loading);

  // Premier / CS2 Level boxes. The left box is the account's Premier rating; the
  // right box is its profile rank (the "Rank 37" the game shows), read from the
  // Game Coordinator — more useful than Wingman, which most stock never touches.
  let premier;
  let level;
  if (loading) {
    premier = statBox("Premier", "dim", "…", "Checking…");
    level = statBox("CS2 Level", "dim", "…", "Checking…");
  } else if (d) {
    premier =
      d.premierRating > 0
        ? statBox("Premier", "value", d.premierRating.toLocaleString("en-US"), "CS Rating")
        : statBox("Premier", "dim", "—", "No rating yet");
    level =
      d.profileLevel > 0
        ? statBox("CS2 Level", "value", String(d.profileLevel), "Profile rank")
        : statBox("CS2 Level", "dim", "—", "Not read");
  } else {
    premier = statBox("Premier", "dim", "—", "Not checked");
    level = statBox("CS2 Level", "dim", "—", "Not checked");
  }

  // Token comes from the stored token itself and needs no lookup.
  const token = tokenRowValue(acc.token_expires_at);
  // Cooldown / VAC only after a check.
  let cooldown = { text: "—", level: "dim" };
  let vac = { text: "—", level: "dim" };
  if (loading) {
    cooldown = { text: "Checking…", level: "dim" };
    vac = { text: "Checking…", level: "dim" };
  } else if (d) {
    cooldown = cooldownText(d.cooldownExpiresUnix);
    vac = vacRowValue(d.vacBanned);
  }

  const errorRow =
    r && r.error
      ? `<div class="drow"><span class="drow-k">Status</span><span class="drow-v gone">${escapeHtml(r.error)}</span></div>`
      : "";

  // The whole stats block is identifying, so it is dropped on stream.
  const stats = streamer
    ? ""
    : `
      <div class="stat-boxes">${premier}${level}</div>
      <div class="detail-rows">
        <div class="drow"><span class="drow-k">Token</span><span class="drow-v ${token.level}">${escapeHtml(token.text)}</span></div>
        <div class="drow"><span class="drow-k">Cooldown</span><span class="drow-v ${cooldown.level}">${escapeHtml(cooldown.text)}</span></div>
        <div class="drow"><span class="drow-k">VAC</span><span class="drow-v ${vac.level}">${escapeHtml(vac.text)}</span></div>
        ${errorRow}
      </div>`;

  const refreshBtn = streamer
    ? ""
    : `<button class="btn wide" data-rank="${sid}"${loading ? " disabled" : ""}>${loading ? "Checking status…" : "Refresh status"}</button>`;

  detailPane.innerHTML = `
    <div class="detail">
      <div class="detail-head">
        ${avatarHtml(view, "avatar-lg")}
        <div class="detail-id">
          <div class="detail-name"><span>${escapeHtml(view.display_name)}</span></div>
          <div class="detail-sub">${escapeHtml(streamer ? "••••••••" : acc.steamid)}</div>
        </div>
      </div>
      ${stats}
      <div class="detail-actions">
        ${refreshBtn}
        <button class="btn btn-accent" data-signin="${sid}">Sign in</button>
        <button class="btn" data-copy="${sid}">Copy token</button>
        <button class="btn danger" data-remove="${sid}">Remove</button>
      </div>
    </div>`;
}

async function refresh() {
  try {
    accounts = await invoke("list_accounts");
    render();
  } catch (e) {
    accounts = [];
    render();
    toast(formatError(e), "err");
  }
}

/* ---------- Import ---------- */
const importModal = el("importModal");
const importInput = el("importInput");
const importStatus = el("importStatus");

function openImport() {
  importInput.value = "";
  importStatus.textContent = "";
  importStatus.className = "dialog-msg";
  importModal.classList.remove("hidden");
  setTimeout(() => importInput.focus(), 50);
}

function closeImport() {
  importModal.classList.add("hidden");
  importStatus.textContent = "";
  importStatus.className = "dialog-msg";
}

async function pasteIntoImport() {
  try {
    const text = await invoke("read_clipboard");
    importInput.value = text.trim();
    importStatus.textContent = "";
    importStatus.className = "dialog-msg";
  } catch (e) {
    importStatus.textContent = formatError(e);
    importStatus.className = "dialog-msg err";
  }
}

async function importManual() {
  const payload = importInput.value.trim();
  if (!payload) {
    importStatus.textContent = "No codes entered.";
    importStatus.className = "dialog-msg err";
    return;
  }

  importStatus.textContent = "Importing…";
  importStatus.className = "dialog-msg";

  try {
    const msg = await invoke("import_account", { payload });
    closeImport();
    await refresh();
    toast(msg, "ok");
  } catch (e) {
    importStatus.textContent = formatError(e);
    importStatus.className = "dialog-msg err";
  }
}

/* ---------- Account actions ---------- */
async function signIn(steamid) {
  try {
    const msg = await invoke("sign_in", { steamid });
    await refresh();
    toast(msg, "ok");
  } catch (e) {
    toast(formatError(e), "err");
  }
}

/**
 * Put the account's login token back on the clipboard.
 *
 * The token never comes through here: Rust reads it from the sealed store and
 * writes it to the clipboard itself, so the only thing this function handles is
 * the sentence that comes back.
 */
async function copyToken(steamid) {
  try {
    toast(await invoke("copy_token", { steamid }), "ok");
  } catch (e) {
    toast(formatError(e), "err");
  }
}

/**
 * Pull one account's CS2 rank and cooldown.
 *
 * Slow on purpose — it is a real Steam logon behind the scenes — so it only
 * runs when the customer asks, and the row shows "Checking status…" while it
 * does. The token never comes through here; Rust reads it from the sealed store
 * and hands it to the helper.
 */
async function checkRank(steamid) {
  ranks[steamid] = { loading: true };
  render();
  try {
    ranks[steamid] = { data: await invoke("cs2_rank", { steamid }) };
  } catch (e) {
    ranks[steamid] = { error: formatError(e) };
  }
  render();
}

let checkingAll = false;

/**
 * Check every account in turn — the "which of these still sign in?" pass.
 *
 * Sequential on purpose: each check is a real Steam logon, and firing ten at
 * once invites a rate limit that makes good tokens look dead. The footer button
 * counts progress; each row shows its own result as it lands. Accounts with no
 * token of ours fall back to the one Steam saved, so a signed-in account with no
 * imported token is checked too.
 */
async function checkAll() {
  if (checkingAll) return;
  const ids = accounts.map((a) => a.steamid);
  if (ids.length === 0) return;
  checkingAll = true;
  const btn = el("checkAllBtn");
  if (btn) btn.disabled = true;
  try {
    for (let i = 0; i < ids.length; i++) {
      const steamid = ids[i];
      if (btn) btn.textContent = `Checking ${i + 1}/${ids.length}…`;
      ranks[steamid] = { loading: true };
      render();
      try {
        ranks[steamid] = { data: await invoke("cs2_rank", { steamid }) };
      } catch (e) {
        ranks[steamid] = { error: formatError(e) };
      }
      render();
    }
  } finally {
    checkingAll = false;
    if (btn) {
      btn.disabled = false;
      btn.textContent = "Check all";
    }
  }
}

function askRemove(steamid) {
  const idx = accounts.findIndex((a) => a.steamid === steamid);
  const acc = accounts[idx];
  const name = settings.streamer_mode
    ? `Account ${idx + 1}`
    : acc
      ? acc.display_name
      : "this account";
  openConfirm("Remove account", `Remove ${name}?`, "Remove", async () => {
    try {
      const msg = await invoke("remove_account", { steamid });
      await refresh();
      toast(msg, "ok");
    } catch (e) {
      toast(formatError(e), "err");
    }
  });
}

function askClearSteam() {
  openConfirm(
    "Reset cache",
    "Clear Steam's cached login tokens on this PC? Your saved accounts stay here and can sign in again.",
    "Reset",
    async () => {
      try {
        const msg = await invoke("clear_steam");
        await refresh();
        toast(msg, "ok");
      } catch (e) {
        toast(formatError(e), "err");
      }
    }
  );
}

/* ---------- Confirm ---------- */
const confirmModal = el("confirmModal");
function openConfirm(title, text, label, handler) {
  el("confirmTitle").textContent = title;
  el("confirmText").textContent = text;
  el("confirmYes").textContent = label;
  confirmHandler = handler;
  confirmModal.classList.remove("hidden");
  setTimeout(
    () => confirmModal.querySelector('.btn[data-action="close-confirm"]')?.focus(),
    50
  );
}
function closeConfirm() {
  confirmModal.classList.add("hidden");
  confirmHandler = null;
}

/* ---------- Settings ---------- */
/* ---------- updates ---------- */

const REPO = "imanxboy/NFAStore-Tool";
const RELEASES_API = `https://api.github.com/repos/${REPO}/releases/latest`;
const INSTALLER_URL = `https://github.com/${REPO}/releases/latest/download/nfastore-tool-setup.exe`;

let pendingUpdateUrl = null;

/**
 * The check runs here rather than in Rust on purpose: it is one small JSON call
 * and keeping it here lets the button name the new version before anything is
 * downloaded. The download and the install are Rust's, in `install_update`.
 *
 * Two presses, deliberately. The first says what is available, the second
 * commits to it — an update that starts installing the moment you ask "is there
 * one?" is not a question, it is a surprise.
 */
async function checkForUpdates() {
  const btn = el("updateBtn");
  const status = el("updateStatus");

  if (pendingUpdateUrl) {
    btn.disabled = true;
    status.classList.remove("is-error");
    status.textContent = "Downloading…";

    try {
      // Rust downloads, checks the file really is an installer, runs it and
      // closes the app. Nothing after this line is expected to matter.
      status.textContent = await invoke("install_update", { url: pendingUpdateUrl });
      return;
    } catch (e) {
      // GitHub's download host is the one part that is unreachable from here
      // without a VPN. When it will not come through, the browser still can —
      // so the button becomes that instead of just reporting a failure.
      status.textContent = `${formatError(e)} Opening your browser instead…`;
      status.classList.add("is-error");
      try {
        await invoke("open_release_link", { url: pendingUpdateUrl });
      } catch {
        status.textContent = `${formatError(e)} Download it from the site instead.`;
      }
    } finally {
      btn.disabled = false;
    }
    return;
  }

  btn.disabled = true;
  status.classList.remove("is-error");
  status.textContent = "Checking…";

  try {
    // GitHub is slow to answer from here but it does answer; give it room
    // rather than reporting a failure that is really just latency.
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 20000);
    const res = await fetch(RELEASES_API, {
      headers: { Accept: "application/vnd.github+json" },
      signal: controller.signal,
    });
    clearTimeout(timer);

    if (!res.ok) throw new Error(`GitHub answered ${res.status}`);

    const release = await res.json();
    const latest = String(release.tag_name || "").trim();
    if (!latest) throw new Error("No release found.");

    const available = await invoke("is_update_available", { latest });

    if (available) {
      pendingUpdateUrl = INSTALLER_URL;
      status.textContent = `Version ${latest.replace(/^v/i, "")} is available.`;
      btn.textContent = "Update now";
      btn.classList.add("is-update");
    } else {
      status.textContent = "You are on the latest version.";
    }
  } catch (e) {
    status.textContent =
      e && e.name === "AbortError"
        ? "GitHub did not answer in time. Try again, or check with a VPN on."
        : `Could not check for updates. ${formatError(e)}`;
    status.classList.add("is-error");
  } finally {
    btn.disabled = false;
  }
}

async function loadVersion() {
  try {
    el("appVersion").textContent = await invoke("app_version");
  } catch {
    el("appVersion").textContent = "?";
  }
}

const settingsModal = el("settingsModal");

function syncSettingsForm() {
  for (const input of settingsModal.querySelectorAll("[data-setting]")) {
    input.checked = Boolean(settings[input.dataset.setting]);
  }
}

function openSettings() {
  syncSettingsForm();
  settingsModal.classList.remove("hidden");
  setTimeout(() => settingsModal.querySelector(".toggle")?.focus(), 50);
}

function closeSettings() {
  settingsModal.classList.add("hidden");
}

async function loadSettings() {
  try {
    settings = await invoke("get_settings");
    syncSettingsForm();
    render();
  } catch (e) {
    toast(formatError(e), "err");
  }
}

async function persistSettings() {
  try {
    await invoke("save_settings", { settings });
    render();
    toast("Settings saved", "ok");
  } catch (e) {
    toast(formatError(e), "err");
    syncSettingsForm();
  }
}

function onSettingToggle(e) {
  const input = e.target.closest("[data-setting]");
  if (!input) return;
  settings = { ...settings, [input.dataset.setting]: input.checked };
  persistSettings();
}

/* ---------- Wiring ---------- */
el("importBtn").addEventListener("click", openImport);
el("emptyImportBtn").addEventListener("click", openImport);
el("pasteBtn").addEventListener("click", pasteIntoImport);
el("refreshBtn").addEventListener("click", () => refresh());
el("doImportBtn").addEventListener("click", importManual);
el("checkAllBtn").addEventListener("click", () => checkAll());
el("dangerBtn").addEventListener("click", askClearSteam);
el("settingsBtn").addEventListener("click", openSettings);
el("updateBtn").addEventListener("click", checkForUpdates);
settingsModal.addEventListener("change", onSettingToggle);

// Custom titlebar window controls (frameless window).
const tauriWin = window.__TAURI__ && window.__TAURI__.window;
const appWindow = tauriWin
  ? tauriWin.getCurrentWindow
    ? tauriWin.getCurrentWindow()
    : tauriWin.getCurrent && tauriWin.getCurrent()
  : null;
if (appWindow) {
  el("winMin").addEventListener("click", () => appWindow.minimize());
  el("winMax").addEventListener("click", () => appWindow.toggleMaximize());
  el("winClose").addEventListener("click", () => appWindow.close());
}
el("confirmYes").addEventListener("click", () => {
  const fn = confirmHandler;
  closeConfirm();
  if (fn) fn();
});

document.addEventListener("click", (e) => {
  const t = e.target.closest("[data-action]");
  if (t) {
    const action = t.dataset.action;
    if (action === "close-import") closeImport();
    if (action === "close-confirm") closeConfirm();
    if (action === "close-settings") closeSettings();
    return;
  }
  const select = e.target.closest("[data-select]");
  if (select) {
    selectedSteamid = select.dataset.select;
    return render();
  }
  const signin = e.target.closest("[data-signin]");
  if (signin) return signIn(signin.dataset.signin);
  const rank = e.target.closest("[data-rank]");
  if (rank) return checkRank(rank.dataset.rank);
  const copy = e.target.closest("[data-copy]");
  if (copy) return copyToken(copy.dataset.copy);
  const remove = e.target.closest("[data-remove]");
  if (remove) return askRemove(remove.dataset.remove);
});

[importModal, confirmModal, settingsModal].forEach((m) => {
  m.addEventListener("click", (e) => {
    if (e.target === m) m.classList.add("hidden");
  });
});

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    closeImport();
    closeConfirm();
    closeSettings();
  }
  if (e.key === "Enter" && !importModal.classList.contains("hidden")) {
    if (e.ctrlKey || e.target === importInput) importManual();
  }
});

listen("update-progress", (e) => {
  const status = el("updateStatus");
  // Only while the download is the thing on screen: a late event must not
  // overwrite the message that replaced it.
  if (status.textContent.startsWith("Downloading")) {
    status.textContent = `Downloading… ${e.payload}%`;
  }
});

listen("accounts-changed", () => refresh());
listen("settings-changed", () => loadSettings());
listen("status", (e) => {
  refresh().then(() => toast(e.payload, "ok"));
});
listen("status-error", (e) => toast(e.payload, "err"));

loadSettings().then(() => refresh());
loadVersion();
