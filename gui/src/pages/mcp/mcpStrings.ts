import { locale } from "../../services/locale";

export interface McpDictionary {
  tabManaged: string;
  tabStore: string;
  tabProject: string;
  searchManaged: string;
  searchStore: string;
  btnPaste: string;
  btnNew: string;
  btnSync: string;
  btnProbe: string;
  probing: string;
  probingShort: string;
  countSuffix: string;
  pausedLabel: string;
  activeLabel: string;
  unmanagedSubtitle: string;
  adoptBtn: string;
  dropFoundBtn: string;
  warningsTitle: string;
  emptyManagedTitle: string;
  emptyManagedBody: string;
  noMatchTitle: string;
  detailCommand: string;
  detailUrl: string;
  detailEnvCount: string;
  detailHeadersCount: string;
  detailToolsEmpty: string;
  detailEdit: string;
  deleteConfirmTitle: string;
  deleteConfirmBody: string;
  deleteConfirmOk: string;
  storeOnlineToggle: string;
  storeFeatured: string;
  storeInstalled: string;
  storeInstall: string;
  storeSignInBadge: string;
  storeEmpty: string;
  storeInstallModalTitle: string;
  storeNameLabel: string;
  storeAgentsLabel: string;
  modalEditTitle: string;
  modalNewTitle: string;
  modalPasteTitle: string;
  modalPastePlaceholder: string;
  modalPasteParseBtn: string;
  modalPasteDetected: string;
  modalNameLabel: string;
  modalTransportLabel: string;
  modalCommandLabel: string;
  modalArgsLabel: string;
  modalEnvLabel: string;
  modalUrlLabel: string;
  modalHeadersLabel: string;
  modalTimeoutLabel: string;
  modalSaveBtn: string;
  projectTitle: string;
  projectSubtitle: string;
  projectCopyBtn: string;
  projectCopied: string;
  syncDoneToast: string;
  adoptedToast: string;
  savedToast: string;
  removedToast: string;
  storeInstalledToast: string;
  sortAsc: string;
  sortDesc: string;
  selectBtn: string;
  cancelSelectBtn: string;
  unprobedLabel: string;
  probeFailedLabel: string;
  toolsUnit: string;
  libraryTitle: string;
  foundLabel: string;
  foundExpand: string;
  foundCollapse: string;
  foundAgentsPrefix: string;
  foundMoreAgents: string;
  foundEmptyHint: string;
  agentBlockTitle: string;
  agentScopeInstalled: string;
  agentScopeAll: string;
  agentSearch: string;
  agentAllOn: string;
  agentAllOff: string;
  agentEnabledSuffix: string;
  agentNoMatch: string;
  agentAbsentTag: string;
  toolsSectionTitle: string;
  rowAgentsNone: string;
  copyCommand: string;
  copiedToast: string;
  detailPausedNotice: string;
  batchSelectedCount: string;
  batchEnable: string;
  batchPause: string;
  batchDelete: string;
  batchSelectAll: string;
  batchClear: string;
  storeFilterAll: string;
  storeFilterFeatured: string;
  storeFilterStdio: string;
  storeFilterRemote: string;
  storeOnlineLabel: string;
  libraryProbeAll: string;
  discoveryLosslessHint: string;
  detailAgentConflict: string;
  detailUnprobedHint: string;
  detailCopyCode: string;
  detailCopied: string;
  projectSelectAgent: string;
  projectEmpty: string;
  storeRequiredFieldError: string;
  storeNoAgentsNotice: string;
  storeNoAgentsBlocked: string;
  storeAlreadyInstalledHint: string;
  storeCloneBtn: string;
  storeOpenExistingBtn: string;
  storeInstallRequiredTag: string;
  validationNameRequired: string;
  validationCommandRequired: string;
  validationUrlRequired: string;
  validationUrlInvalid: string;
  validationArgsUnterminatedQuote: string;
  validationEnvFormat: string;
  validationHeaderFormat: string;
  validationTimeoutInvalid: string;
  btnBackups: string;
  backupsModalTitle: string;
  backupsSubtitle: string;
  backupsEmpty: string;
  backupsAbsentBadge: string;
  backupsSnapshotBadge: string;
  backupsRestoreBtn: string;
  backupsRestoreConfirmTitle: string;
  backupsRestoreConfirmBody: string;
  backupsRestoredToast: string;
  backupsRemovedToast: string;
  batchDeleteConfirmTitle: string;
  batchDeleteConfirmBody: string;
  batchDeleteConfirmOk: string;
  detailToolsUnsupportedHint: string;
  importConflictModalTitle: string;
  importConflictDesc: string;
  importConflictKeepBtn: string;
  importConflictReplaceBtn: string;
  importConflictRenameBtn: string;
  importConflictLibraryHeading: string;
  importConflictFoundHeading: string;
}

const ZH: McpDictionary = {
  tabManaged: "已纳管",
  tabStore: "内置商店",
  tabProject: "项目配置",
  searchManaged: "搜索 MCP 服务或命令…",
  searchStore: "搜索商店、仓库地址、npm/pypi 包名…",
  btnPaste: "粘贴导入",
  btnNew: "新建服务",
  btnSync: "同步全部",
  btnProbe: "检测连通性与工具",
  probing: "正在探测…",
  probingShort: "探测中",
  countSuffix: "个服务",
  pausedLabel: "已暂停",
  activeLabel: "启用中",
  unmanagedSubtitle: "无损纳管会接管账本登记，原注释、扩展字段和其它服务保持完整。",
  adoptBtn: "无损纳管",
  dropFoundBtn: "移出原配置",
  warningsTitle: "同步保护提醒",
  emptyManagedTitle: "还没有已纳管的外部 MCP",
  emptyManagedBody: "可从「内置商店」一键安装，点击右上角「新建服务」/「粘贴导入」，或纳管已有 Agent 配置。",
  noMatchTitle: "没有匹配的 MCP 服务",
  detailCommand: "启动命令",
  detailUrl: "服务地址",
  detailEnvCount: "环境变量",
  detailHeadersCount: "请求头",
  detailToolsEmpty: "该服务未声明任何工具",
  detailEdit: "编辑",
  deleteConfirmTitle: "删除 MCP 服务",
  deleteConfirmBody: "仅移除 SkillStar 账本登记写入各 Agent 的同名条目，绝不删改注释或其它自有配置。",
  deleteConfirmOk: "确认删除",
  storeOnlineToggle: "在线 Registry",
  storeFeatured: "精选",
  storeInstalled: "已纳管",
  storeInstall: "安装",
  storeSignInBadge: "首次使用需登录",
  storeEmpty: "没有找到匹配的商店服务",
  storeInstallModalTitle: "安装 MCP 服务",
  storeNameLabel: "本地服务名",
  storeAgentsLabel: "同步到 Agent",
  modalEditTitle: "编辑 MCP 服务",
  modalNewTitle: "新建外部 MCP 服务",
  modalPasteTitle: "粘贴 JSON / CLI 命令导入",
  modalPastePlaceholder:
    "粘贴 mcpServers JSON、JSONC 片段、claude mcp add 命令或 npx/uvx 启动命令…",
  modalPasteParseBtn: "识别并填入表单",
  modalPasteDetected: "识别格式：",
  modalNameLabel: "服务名称",
  modalTransportLabel: "传输协议",
  modalCommandLabel: "可执行命令",
  modalArgsLabel: "参数（空格或换行分隔）",
  modalEnvLabel: "环境变量（每行 KEY=VALUE，支持 ${VAR}）",
  modalUrlLabel: "端点 URL",
  modalHeadersLabel: "HTTP 请求头（每行 Key: Value）",
  modalTimeoutLabel: "启动超时秒数（可选，Codex）",
  modalSaveBtn: "保存并无损同步",
  projectTitle: "项目级 MCP 配置片段",
  projectSubtitle: "按当前已启用的非暂停 MCP 生成 10 家项目格式只读预览，不擅自修改项目文件。",
  projectCopyBtn: "复制片段",
  projectCopied: "已复制项目片段",
  syncDoneToast: "已完成无损同步",
  adoptedToast: "已无损纳管到 SkillStar",
  savedToast: "MCP 配置已保存并同步",
  removedToast: "已按账本安全移除",
  storeInstalledToast: "已从内置商店安装并同步",
  sortAsc: "A→Z",
  sortDesc: "Z→A",
  selectBtn: "选择",
  cancelSelectBtn: "完成",
  unprobedLabel: "未检查",
  probeFailedLabel: "检查失败",
  toolsUnit: "个工具",
  libraryTitle: "资源库",
  foundLabel: "未纳管 MCP",
  foundExpand: "展开处理",
  foundCollapse: "收起",
  foundAgentsPrefix: "来自",
  foundMoreAgents: " 等",
  foundEmptyHint: "没有匹配的未纳管条目",
  agentBlockTitle: "同步到的 Agent",
  agentScopeInstalled: "仅已装",
  agentScopeAll: "全部 35",
  agentSearch: "搜索 Agent…",
  agentAllOn: "全选",
  agentAllOff: "清空",
  agentEnabledSuffix: "已开启",
  agentNoMatch: "没有匹配的 Agent",
  agentAbsentTag: "未安装",
  toolsSectionTitle: "工具",
  rowAgentsNone: "未同步",
  copyCommand: "复制命令",
  copiedToast: "已复制命令",
  detailPausedNotice: "已暂停：不会写入任何 Agent",
  batchSelectedCount: "已选",
  batchEnable: "启用",
  batchPause: "暂停",
  batchDelete: "删除",
  batchSelectAll: "全选",
  batchClear: "清空",
  storeFilterAll: "全部",
  storeFilterFeatured: "精选",
  storeFilterStdio: "Stdio",
  storeFilterRemote: "远程",
  storeOnlineLabel: "在线 Registry",
  libraryProbeAll: "检测全部",
  discoveryLosslessHint: "无损接管，保留注释与原配置",
  detailAgentConflict: "当前传输协议与该 Agent 不兼容",
  detailUnprobedHint: "未检测，点击检测将拉起进程并读取 tools/list",
  detailCopyCode: "复制",
  detailCopied: "已复制",
  projectSelectAgent: "目标格式",
  projectEmpty: "暂无可预览的项目级 MCP 片段（请先启用至少一个非暂停服务）",
  storeRequiredFieldError: "请填写必填项：",
  storeNoAgentsNotice: "本机暂未检测到已安装的 Agent，请先勾选至少一个目标 Agent 或切换查看全部 35 个目标后再安装。",
  storeNoAgentsBlocked: "请至少选择一个目标 Agent，避免安装成 0-agent 孤立服务。",
  storeAlreadyInstalledHint: "该商店服务已纳管，已为你定位到现有服务。",
  storeCloneBtn: "安装副本",
  storeOpenExistingBtn: "打开已有服务",
  storeInstallRequiredTag: "必填",
  validationNameRequired: "服务名称不能为空",
  validationCommandRequired: "Stdio 服务必须填写启动命令",
  validationUrlRequired: "远程服务必须填写有效的 URL",
  validationUrlInvalid: "服务地址必须以 http:// 或 https:// 开头",
  validationArgsUnterminatedQuote: "启动参数中存在未闭合的引号",
  validationEnvFormat: "环境变量格式无效（应为 KEY=VALUE）：",
  validationHeaderFormat: "请求头格式无效（应为 Key: Value）：",
  validationTimeoutInvalid: "启动超时秒数必须为大于 0 的数字",
  btnBackups: "配置备份",
  backupsModalTitle: "MCP 配置滚动备份",
  backupsSubtitle: "每次无损写入前自动保留最近 40 份原配置；新文件首次创建前会记录空文件标记。",
  backupsEmpty: "暂无配置备份记录",
  backupsAbsentBadge: "新建前不存在",
  backupsSnapshotBadge: "已有快照",
  backupsRestoreBtn: "恢复此备份",
  backupsRestoreConfirmTitle: "恢复配置备份",
  backupsRestoreConfirmBody: "将按该备份还原目标配置文件（若标注「新建前不存在」则会删除该新建文件）。当前文件会先自动备份。",
  backupsRestoredToast: "已从备份恢复配置文件",
  backupsRemovedToast: "已按新建前状态移除配置文件",
  batchDeleteConfirmTitle: "批量删除 MCP 服务",
  batchDeleteConfirmBody: "确定删除已选中的 MCP 服务吗？仅移除 SkillStar 账本登记写入各 Agent 的条目，绝不删改注释或其它自有配置。",
  batchDeleteConfirmOk: "确认批量删除",
  detailToolsUnsupportedHint: "以下已启用 Agent 不支持按服务禁用工具（同步时将自动跳过）：",
  importConflictModalTitle: "发现同名服务配置冲突",
  importConflictDesc: "库中已存在同名但配置不同的 MCP 服务，请选择纳管策略：",
  importConflictKeepBtn: "保留库中配置并加入此 Agent",
  importConflictReplaceBtn: "使用发现配置覆盖库中服务",
  importConflictRenameBtn: "保留两者，另存为副本",
  importConflictLibraryHeading: "当前库中配置",
  importConflictFoundHeading: "Agent 上发现的配置",
};

const EN: McpDictionary = {
  tabManaged: "Managed",
  tabStore: "Built-in Store",
  tabProject: "Project Snippets",
  searchManaged: "Search MCP servers or commands…",
  searchStore: "Search store, repo URL, npm or pypi package…",
  btnPaste: "Paste Config",
  btnNew: "New Server",
  btnSync: "Sync All",
  btnProbe: "Probe & List Tools",
  probing: "Probing…",
  probingShort: "Probing",
  countSuffix: "servers",
  pausedLabel: "Paused",
  activeLabel: "Active",
  unmanagedSubtitle:
    "Lossless adoption tracks entries in the SkillStar ledger while preserving comments, custom fields, and siblings.",
  adoptBtn: "Adopt Losslessly",
  dropFoundBtn: "Remove Entry",
  warningsTitle: "Sync Protection Notices",
  emptyManagedTitle: "No managed external MCP servers yet",
  emptyManagedBody:
    "Install from the Built-in Store, click New Server / Paste Config, or adopt discovered entries above.",
  noMatchTitle: "No matching MCP servers",
  detailCommand: "Command",
  detailUrl: "Endpoint URL",
  detailEnvCount: "Env Vars",
  detailHeadersCount: "Headers",
  detailToolsEmpty: "Server reported zero tools",
  detailEdit: "Edit",
  deleteConfirmTitle: "Remove MCP Server",
  deleteConfirmBody:
    "Only removes SkillStar-ledger entries from configured agents, preserving comments and user-owned config blocks.",
  deleteConfirmOk: "Remove",
  storeOnlineToggle: "Online Registry",
  storeFeatured: "Featured",
  storeInstalled: "Installed",
  storeInstall: "Install",
  storeSignInBadge: "Browser Sign-In",
  storeEmpty: "No matching store servers found",
  storeInstallModalTitle: "Install MCP Server",
  storeNameLabel: "Local Server Name",
  storeAgentsLabel: "Target Agents",
  modalEditTitle: "Edit MCP Server",
  modalNewTitle: "New External MCP Server",
  modalPasteTitle: "Paste JSON / CLI Command",
  modalPastePlaceholder:
    "Paste mcpServers JSON, JSONC snippet, `claude mcp add ...`, or `npx` / `uvx` command…",
  modalPasteParseBtn: "Parse into Form",
  modalPasteDetected: "Detected:",
  modalNameLabel: "Server Name",
  modalTransportLabel: "Transport",
  modalCommandLabel: "Command",
  modalArgsLabel: "Arguments (space or newline separated)",
  modalEnvLabel: "Environment (KEY=VALUE per line, supports ${VAR})",
  modalUrlLabel: "Endpoint URL",
  modalHeadersLabel: "HTTP Headers (Key: Value per line)",
  modalTimeoutLabel: "Startup Timeout Seconds (Optional, Codex)",
  modalSaveBtn: "Save & Sync Losslessly",
  projectTitle: "Project-Level MCP Snippets",
  projectSubtitle:
    "Read-only preview across the 10 supported project MCP formats from active non-paused servers.",
  projectCopyBtn: "Copy Snippet",
  projectCopied: "Copied project snippet",
  syncDoneToast: "Lossless sync complete",
  adoptedToast: "Losslessly adopted into library",
  savedToast: "Saved & synced MCP server",
  removedToast: "Safely removed ledger-owned entries",
  storeInstalledToast: "Installed from store and synced",
  sortAsc: "A→Z",
  sortDesc: "Z→A",
  selectBtn: "Select",
  cancelSelectBtn: "Done",
  unprobedLabel: "Unchecked",
  probeFailedLabel: "Failed",
  toolsUnit: "tools",
  libraryTitle: "Library",
  foundLabel: "Unmanaged MCP",
  foundExpand: "Review",
  foundCollapse: "Collapse",
  foundAgentsPrefix: "From",
  foundMoreAgents: " and ",
  foundEmptyHint: "No matching unmanaged entries",
  agentBlockTitle: "Synced Agents",
  agentScopeInstalled: "Installed",
  agentScopeAll: "All 35",
  agentSearch: "Search agents…",
  agentAllOn: "All on",
  agentAllOff: "Clear",
  agentEnabledSuffix: "on",
  agentNoMatch: "No matching agents",
  agentAbsentTag: "missing",
  toolsSectionTitle: "Tools",
  rowAgentsNone: "No agents",
  copyCommand: "Copy command",
  copiedToast: "Command copied",
  detailPausedNotice: "Paused: written to no agent",
  batchSelectedCount: "selected",
  batchEnable: "Enable",
  batchPause: "Pause",
  batchDelete: "Delete",
  batchSelectAll: "Select All",
  batchClear: "Clear",
  storeFilterAll: "All",
  storeFilterFeatured: "Featured",
  storeFilterStdio: "Stdio",
  storeFilterRemote: "Remote",
  storeOnlineLabel: "Online Registry",
  libraryProbeAll: "Probe All",
  discoveryLosslessHint: "Lossless · preserves comments & siblings",
  detailAgentConflict: "Transport incompatible with this agent",
  detailUnprobedHint: "Not probed yet. Run probe to fetch tools/list",
  detailCopyCode: "Copy",
  detailCopied: "Copied",
  projectSelectAgent: "Target Agent",
  projectEmpty: "No active non-paused servers to preview for this project",
  storeRequiredFieldError: "Missing required field: ",
  storeNoAgentsNotice: "No installed agents detected on this machine. Select at least one target agent (or inspect All 35) before installing.",
  storeNoAgentsBlocked: "Select at least one target agent before installing to avoid creating an orphaned 0-agent server.",
  storeAlreadyInstalledHint: "This store server is already managed; opened the existing entry.",
  storeCloneBtn: "Install Copy",
  storeOpenExistingBtn: "Open Existing",
  storeInstallRequiredTag: "Required",
  validationNameRequired: "Server name is required",
  validationCommandRequired: "Command is required for stdio servers",
  validationUrlRequired: "Endpoint URL is required for remote servers",
  validationUrlInvalid: "Endpoint URL must start with http:// or https://",
  validationArgsUnterminatedQuote: "Arguments contain an unterminated quote",
  validationEnvFormat: "Invalid environment line (expected KEY=VALUE): ",
  validationHeaderFormat: "Invalid HTTP header line (expected Key: Value): ",
  validationTimeoutInvalid: "Startup timeout must be a positive number (> 0)",
  btnBackups: "Backups",
  backupsModalTitle: "Rolling MCP Config Backups",
  backupsSubtitle: "Up to 40 snapshots kept before each lossless write; newly created files record an absent marker.",
  backupsEmpty: "No config backups recorded yet",
  backupsAbsentBadge: "Did not exist",
  backupsSnapshotBadge: "Snapshot",
  backupsRestoreBtn: "Restore",
  backupsRestoreConfirmTitle: "Restore Config Backup",
  backupsRestoreConfirmBody: "Restore target config from this snapshot (or remove newly created file for absent markers). Current file is backed up first.",
  backupsRestoredToast: "Restored config from backup",
  backupsRemovedToast: "Removed newly created file per absent backup",
  batchDeleteConfirmTitle: "Delete Selected MCP Servers",
  batchDeleteConfirmBody: "Remove all selected MCP servers from the library and withdraw their ledger-owned agent entries while preserving comments and user-owned blocks?",
  batchDeleteConfirmOk: "Delete Selected",
  detailToolsUnsupportedHint: "Enabled agent(s) do not support per-tool disable (skipped during sync): ",
  importConflictModalTitle: "Name Conflict on Adopt",
  importConflictDesc: "A server with this name already exists in your library with a different configuration. Choose how to proceed:",
  importConflictKeepBtn: "Keep Library Config & Add Agent",
  importConflictReplaceBtn: "Overwrite Library With Discovered",
  importConflictRenameBtn: "Keep Both (Save as Copy)",
  importConflictLibraryHeading: "Current Library Config",
  importConflictFoundHeading: "Discovered on Agent",
};

export function ms(): McpDictionary {
  return locale() === "en" ? EN : ZH;
}
