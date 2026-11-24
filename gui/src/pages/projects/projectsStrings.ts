import { locale, type Locale } from "../../services/locale";

const STRINGS: Record<Locale, Record<string, string>> = {
  "zh-CN": {
    "projects.plan.title": "待审批的项目技能 MCP 计划",
    "projects.plan.hint":
      "开发 Agent 通过 MCP 推荐了以下项目技能变更。批准仅写入本机授权记录，不会立即部署文件或修改项目树。",
    "projects.plan.canonicalRoot": "规范路径",
    "projects.plan.registered": "注册状态",
    "projects.plan.registeredYes": "已注册到 SkillStar",
    "projects.plan.registeredNo": "未注册（待首次应用时登记）",
    "projects.plan.owner": "共享路径 Owner",
    "projects.plan.affectedAgents": "受影响 Agent",
    "projects.plan.operations": "计划操作",
    "projects.plan.actionCreate": "新建链接",
    "projects.plan.actionAlready": "已存在",
    "projects.plan.approveBtn": "批准计划",
    "projects.plan.approvingBtn": "正在写入批准…",
    "projects.plan.approvedBadge": "已获桌面批准",
    "projects.plan.approvedHint": "已记录 SkillStar 批准，等待 MCP 客户端继续应用（仅写批准，未执行部署）。",
    "projects.plan.approveDone": "已写入计划批准（{{id}}），未触发部署",
    "projects.plan.approveFailed": "批准计划失败",
  },
  en: {
    "projects.plan.title": "Pending Project-Skill MCP Plan",
    "projects.plan.hint":
      "A coding agent recommended the project skill changes below via MCP. Approving writes the local SkillStar approval record only without deploying files yet.",
    "projects.plan.canonicalRoot": "Canonical Root",
    "projects.plan.registered": "Registration",
    "projects.plan.registeredYes": "Registered in SkillStar",
    "projects.plan.registeredNo": "Unregistered (will register on apply)",
    "projects.plan.owner": "Shared Path Owner",
    "projects.plan.affectedAgents": "Affected Agents",
    "projects.plan.operations": "Planned Operations",
    "projects.plan.actionCreate": "Create link",
    "projects.plan.actionAlready": "Already present",
    "projects.plan.approveBtn": "Approve Plan",
    "projects.plan.approvingBtn": "Recording Approval…",
    "projects.plan.approvedBadge": "Desktop Approved",
    "projects.plan.approvedHint":
      "SkillStar approval has been recorded and is waiting for the MCP client to apply it (no files deployed yet).",
    "projects.plan.approveDone": "Recorded plan approval ({{id}}) without deploying",
    "projects.plan.approveFailed": "Failed to approve project-skill plan",
  },
};

export function pt(key: string): string {
  const table = STRINGS[locale()];
  return table[key] ?? STRINGS.en[key] ?? key;
}

export function ptf(key: string, vars: Record<string, string | number>): string {
  let out = pt(key);
  for (const [k, v] of Object.entries(vars)) {
    out = out.replaceAll(`{{${k}}}`, String(v));
  }
  return out;
}
