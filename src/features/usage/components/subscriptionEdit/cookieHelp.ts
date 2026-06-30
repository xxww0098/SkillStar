export interface CookieHelp {
  title: string;
  openLabel: string;
  intro: string;
  requestTargets: string[];
  outro: string;
  copyHint: string;
}

export function cookieHelpForCatalog(catalogId: string, displayName?: string): CookieHelp {
  if (catalogId === "opencode") {
    return {
      title: "登录 OpenCode 控制台后复制 workspace 请求 Cookie，用于读取 Go / Zen 用量。",
      openLabel: "打开 OpenCode",
      intro: "在浏览器打开 opencode.ai 的 workspace/default/go 页面后，打开 DevTools → Network → 找到 ",
      requestTargets: ["/workspace/...", "/_server"],
      outro: " 请求 → 右键 ",
      copyHint: " → 粘贴到终端中提取 Cookie 字段，或直接从 Request Headers 中复制 ",
    };
  }

  if (catalogId === "stepfun") {
    return {
      title: "登录阶跃星辰开发者平台后复制控制台请求 Cookie，用于读取账户余额与消费。",
      openLabel: "打开阶跃控制台",
      intro: "在浏览器打开 platform.stepfun.com/account-overview 并登录后，打开 DevTools → Network → 刷新页面，找到 ",
      requestTargets: ["QueryAccountBalance", "/api/..."],
      outro: " 请求 → 右键 ",
      copyHint: " → 粘贴到终端中提取 Cookie 字段（需包含 Oasis-Token），或直接从 Request Headers 中复制 ",
    };
  }

  return {
    title: `登录 ${displayName ?? "服务"} 控制台后复制浏览器 Cookie。`,
    openLabel: "打开控制台",
    intro: "在浏览器打开对应控制台后，打开 DevTools → Network → 刷新页面，找到任意同域接口请求 → 右键 ",
    requestTargets: [],
    outro: "",
    copyHint: " → 粘贴到终端中提取 Cookie 字段，或直接从 Request Headers 中复制 ",
  };
}
