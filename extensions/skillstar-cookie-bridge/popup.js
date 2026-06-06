const tokenInput = document.querySelector("#token");
const bindButton = document.querySelector("#bind");
const pushButton = document.querySelector("#push");
const resetButton = document.querySelector("#reset");
const statusEl = document.querySelector("#status");
const statePill = document.querySelector("#state-pill");

const IMPORT_URL = "http://127.0.0.1:1461/usage/cookie-import";
const STORAGE_KEY = "skillstarBindToken";

let bindToken = null;

chrome.storage.local.get([STORAGE_KEY], (result) => {
  bindToken = result[STORAGE_KEY] || null;
  render();
});

bindButton.addEventListener("click", async () => {
  const oneTimeToken = tokenInput.value.trim();
  if (!oneTimeToken) {
    setStatus("请先粘贴 SkillStar 设置页生成的绑定码。", "error");
    return;
  }
  await sendCookies({ token: oneTimeToken }, true);
});

pushButton.addEventListener("click", async () => {
  if (!bindToken) {
    clearBinding();
    setStatus("尚未绑定。请先到 SkillStar 设置页生成绑定码。", "error");
    return;
  }
  await sendCookies({ bind_token: bindToken }, false);
});

resetButton.addEventListener("click", async () => {
  await clearBinding();
  setStatus("已解除浏览器本地绑定。需要重新在 SkillStar 设置页生成绑定码。", "normal");
});

async function sendCookies(auth, isPairing) {
  setBusy(true);
  setStatus("正在读取 OpenCode Cookie...", "normal");
  try {
    const cookies = await readOpenCodeCookies();
    if (!cookies.length) throw new Error("没有读取到 OpenCode Cookie，请先登录 opencode.ai/workspace/default/go。");

    setStatus(`读取到 ${cookies.length} 个 Cookie，正在发送到 SkillStar...`, "normal");
    const sourceUrl = await getOpenCodeTabUrl();
    const response = await fetch(IMPORT_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ provider: "opencode", ...auth, source_url: sourceUrl, cookies }),
    });
    const result = await response.json().catch(() => ({}));
    if (!response.ok || !result.ok) throw new Error(result.error || `SkillStar 返回 ${response.status}`);

    if (isPairing) {
      if (!result.bind_token) throw new Error("SkillStar 未返回绑定凭证，请重新生成绑定码。");
      bindToken = result.bind_token;
      await chrome.storage.local.set({ [STORAGE_KEY]: bindToken });
      tokenInput.value = "";
      render();
      setStatus("绑定完成，Cookie 已推送。后续点击“推送”即可。", "success");
      return;
    }

    setStatus("Cookie 已推送到 SkillStar。", "success");
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (message.includes("尚未绑定") || message.includes("绑定") || message.includes("token")) {
      await clearBinding();
    }
    setStatus(message, "error");
  } finally {
    setBusy(false);
  }
}

async function getOpenCodeTabUrl() {
  const tabs = await chrome.tabs.query({ active: true, currentWindow: true });
  const url = tabs[0]?.url || "";
  return url.includes("opencode.ai") ? url : null;
}

async function readOpenCodeCookies() {
  const [rootCookies, consoleCookies] = await Promise.all([
    chrome.cookies.getAll({ url: "https://opencode.ai/" }),
    chrome.cookies.getAll({ url: "https://console.opencode.ai/" }),
  ]);
  const seen = new Set();
  return [...rootCookies, ...consoleCookies].filter((cookie) => {
    const key = `${cookie.domain}|${cookie.path}|${cookie.name}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

async function clearBinding() {
  bindToken = null;
  await chrome.storage.local.remove([STORAGE_KEY]);
  render();
}

function render() {
  const isBound = Boolean(bindToken);
  document.body.classList.toggle("is-bound", isBound);
  statePill.textContent = isBound ? "已绑定" : "未绑定";
  statePill.className = isBound ? "pill ready" : "pill";
}

function setBusy(isBusy) {
  bindButton.disabled = isBusy;
  pushButton.disabled = isBusy;
  resetButton.disabled = isBusy;
}

function setStatus(message, tone) {
  statusEl.textContent = message;
  statusEl.dataset.tone = tone;
  statePill.classList.toggle("error", tone === "error");
}
