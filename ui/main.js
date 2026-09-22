const invoke = window.__TAURI__.core.invoke;
const listen = window.__TAURI__.event.listen;

const el = (id) => document.getElementById(id);
const accountList = el("accountList");
const accountCount = el("accountCount");
const emptyState = el("emptyState");
const toastWrap = el("toastWrap");

let accounts = [];
let confirmHandler = null;
let settings = {
  always_invisible: true,
  cancel_downloads_on_login: false,
  streamer_mode: false,
  launch_steam_minimized: false,
  mute_notifications_on_login: false,
};

const SIGNIN_SVG =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/><path d="M10 17l5-5-5-5"/><path d="M15 12H3"/></svg>';
const COPY_SVG =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>';
const TRASH_SVG =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M10 11v6"/><path d="M14 11v6"/></svg>';

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

function render() {
  accountCount.textContent = accounts.length ? String(accounts.length) : "";

  if (accounts.length === 0) {
    accountList.innerHTML = "";
    emptyState.classList.remove("hidden");
    return;
  }
  emptyState.classList.add("hidden");

  accountList.innerHTML = accounts
    .map((acc, index) => {
      const view = displayAccount(acc, index);
      const avatar = view.avatar
        ? `<div class="avatar"><img src="${escapeAttr(view.avatar)}" alt="" /></div>`
        : `<div class="avatar">${escapeHtml(view.initials)}</div>`;
      const tag = acc.most_recent ? '<span class="row-tag">Last used</span>' : "";
      // Hidden in streamer mode along with everything else that identifies the
      // account: a precise expiry date is as good as a fingerprint on stream.
      const expiry = settings.streamer_mode ? null : tokenExpiry(acc.token_expires_at);
      return `
        <div class="row">
          ${avatar}
          <div class="row-info">
            <div class="row-name"><span>${escapeHtml(view.display_name)}</span>${tag}</div>
            <div class="row-login">${escapeHtml(view.account_name)}</div>
            ${expiry ? `<div class="row-expiry ${expiry.level}">${escapeHtml(expiry.text)}</div>` : ""}
          </div>
          <div class="row-actions">
            <button class="icon-btn primary" data-signin="${escapeAttr(acc.steamid)}" title="Sign in" aria-label="Sign in as ${escapeAttr(view.display_name)}">${SIGNIN_SVG}</button>
            <button class="icon-btn" data-copy="${escapeAttr(acc.steamid)}" title="Copy login token" aria-label="Copy the login token for ${escapeAttr(view.display_name)}">${COPY_SVG}</button>
            <button class="icon-btn danger" data-remove="${escapeAttr(acc.steamid)}" title="Remove" aria-label="Remove account">${TRASH_SVG}</button>
          </div>
        </div>`;
    })
    .join("");
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
 * The check runs here rather than in Rust on purpose.
 *
 * api.github.com is reachable from Iran without a VPN; the host the installer
 * actually lives on is not (measured: zero bytes in 20s direct, 64KB in 1.5s
 * through a VPN). So the app finds out about the update itself and hands the
 * download to the browser, which already has whatever the customer uses for the
 * rest of GitHub.
 */
async function checkForUpdates() {
  const btn = el("updateBtn");
  const status = el("updateStatus");

  if (pendingUpdateUrl) {
    try {
      await invoke("open_release_link", { url: pendingUpdateUrl });
      status.textContent = "Opened in your browser. Run the installer when it finishes.";
    } catch (e) {
      status.textContent = formatError(e);
      status.classList.add("is-error");
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
      btn.textContent = "Download";
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
  const signin = e.target.closest("[data-signin]");
  if (signin) return signIn(signin.dataset.signin);
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

listen("accounts-changed", () => refresh());
listen("settings-changed", () => loadSettings());
listen("status", (e) => {
  refresh().then(() => toast(e.payload, "ok"));
});
listen("status-error", (e) => toast(e.payload, "err"));

loadSettings().then(() => refresh());
loadVersion();
