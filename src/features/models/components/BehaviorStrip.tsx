/**
 * BehaviorStrip — compact inline behavior controls for each app.
 *
 * Config-table driven: each app defines 2-5 BehaviorField entries,
 * rendered as segment pills, selects, toggles, or model selectors.
 * All changes are instant — no Save button.
 *
 * Includes a small "docs" link per app so users can jump to official config docs.
 */

import { AnimatePresence, motion } from "framer-motion";
import { ExternalLink, Loader2, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { openExternalUrl } from "../../../lib/externalOpen";
import { cn } from "../../../lib/utils";
import { useAppSettings } from "../hooks/useAppSettings";
import type { ProviderEntry } from "../hooks/useModelProviders";
import type { ModelAppId } from "./AppCapsuleSwitcher";

// ── Components ────────────────────────────────────────────────────────────

const TreePadIcon = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
    <path d="M6.5 0A1.5 1.5 0 005 1.5v13A1.5 1.5 0 006.5 16h5a1.5 1.5 0 001.5-1.5V13H8.75a.75.75 0 010-1.5H13V8.75H8.75a.75.75 0 010-1.5H13V4.5H8.75a.75.75 0 010-1.5H13V1.5A1.5 1.5 0 0011.5 0h-5zM3.5 3H5v1.5H3.5a1 1 0 100 2H5V8H3.5a1 1 0 100 2H5v1.5H3.5a1 1 0 100 2H5V15H3.5A2.5 2.5 0 011 12.5v-9A2.5 2.5 0 013.5 1H5v2H3.5z" />
  </svg>
);

const InfoTip = ({ text }: { text: string }) => {
  const renderFormattedText = (rawText: string) => {
    return rawText.split("\n").map((line, idx, arr) => {
      const colonIdx = line.indexOf(": ");
      // If we spot a colon near the start, bold everything before it
      if (colonIdx > 0 && colonIdx < 20) {
        return (
          <span key={idx}>
            <span className="font-bold tracking-tight text-foreground">{line.slice(0, colonIdx)}</span>
            {line.slice(colonIdx)}
            {idx < arr.length - 1 ? "\n" : ""}
          </span>
        );
      }
      return (
        <span key={idx}>
          {line}
          {idx < arr.length - 1 ? "\n" : ""}
        </span>
      );
    });
  };

  return (
    <div className="relative group inline-flex items-center ml-1.5 z-20 hover:z-[100]">
      <div className="w-[14px] h-[14px] rounded-full border border-border/80 text-[9px] font-bold text-muted-foreground/80 flex items-center justify-center cursor-help group-hover:bg-accent group-hover:text-foreground transition-colors">
        ?
      </div>
      <div className="absolute left-full top-1/2 -translate-y-1/2 ml-2 w-48 p-2.5 rounded-lg bg-white dark:bg-[#1e1e24] shadow-xl ring-1 ring-border/20 text-[12px] leading-relaxed text-slate-800 dark:text-zinc-200 opacity-0 pointer-events-none group-hover:opacity-100 transition-opacity z-[100] whitespace-pre-wrap">
        {renderFormattedText(text)}
      </div>
    </div>
  );
};

// ── Types ────────────────────────────────────────────────────────────

interface FieldOption {
  value: string;
  label: string;
}

interface BehaviorField {
  key: string;
  label: string;
  type: "segment" | "toggle" | "number" | "compact_group" | "context_group" | "string" | "select" | "comma_array";
  options?: FieldOption[];
  default: unknown;
  /** For number type: min, max, step */
  min?: number;
  max?: number;
  step?: number;
  /** For number type: if set, field is toggleable (null if stopped), starting with this value */
  toggleOnValue?: number;
  /** Key of another field that must be truthy (non-null) for this field to render */
  dependsOn?: string;
  /** Short text explaining what this setting enables or does */
  description?: string;
  /** For dynamic selects */
  dynamicOptionsFetcher?: string;
}

// ── Per-app behavior definitions ────────────────────────────────────

const APP_BEHAVIORS: Record<ModelAppId, BehaviorField[]> = {
  claude: [
    {
      key: "effortLevel",
      label: "Effort",
      type: "segment",
      options: [
        { value: "low", label: "Low" },
        { value: "medium", label: "Med" },
        { value: "high", label: "High" },
      ],
      default: "medium",
      description:
        "设定模型针对复杂问题的深入思考力度等级：\n\nLow: 轻度思考，适合常规问题\nMed: 中度思考（默认），平衡速度质量\nHigh: 深度思考，适合复杂逻辑推理",
    },
    {
      key: "permissions",
      label: "Mode",
      type: "segment",
      options: [
        { value: "default", label: "Default" },
        { value: "plan", label: "Plan" },
        { value: "acceptEdits", label: "Accept Edits" },
        { value: "bypassPermissions", label: "YOLO" },
      ],
      default: "default",
      description:
        "设置模型操作宿主机时的权限拦截级别：\n\nDefault: 默认防御状态，拦截危险命令\nPlan: 只生成运行计划，全需人工确认\nAccept Edits: 允许改代码，拦截危险终端\nYOLO: 最高危险权限，静默放行一切修改和命令",
    },
    {
      key: "alwaysThinkingEnabled",
      label: "Thinking",
      type: "toggle",
      default: false,
      description: "强制要求模型每次响应前都产生前置心智流思维过程",
    },
    {
      key: "showThinkingSummaries",
      label: "Think Summary",
      type: "toggle",
      default: false,
      description: "心智摘要透传：决定是否在面板中明文展示底层思考过程的最后提炼日志，而非隐藏背后的内部想法",
    },
    {
      key: "showClearContextOnPlanAccept",
      label: "Plan Lock",
      type: "toggle",
      default: false,
      description:
        "方案锁定防抖：接受架构设计与重构计划时，立刻斩断并清空此前讨论的杂乱 Context 旧记忆，保持写码阶段的思绪极度专注",
    },
    {
      key: "includeGitInstructions",
      label: "Git Context",
      type: "toggle",
      default: false,
      description:
        "Git 上下文强注入：强制要求 AI 在编写任何代码之前，自动深潜后台检索当前的 git status 以及最近变动的未提交 Diff，避免出现逻辑错位",
    },
    {
      key: "autoMode",
      label: "Auto Mode",
      type: "segment",
      options: [
        { value: "allow", label: "Allow" },
        { value: "ask", label: "Ask" },
        { value: "soft_deny", label: "Deny" },
      ],
      default: "ask",
      description:
        "自动避险防灾引擎遭遇未知异常或越权行为时的策略决策：\n\nAllow: 激进模式，信任安全环境，全速强行推进\nAsk: 温和模式，在发生越权操作前强行挂起并等待人工授权\nDeny: 保守模式，遇隐患毫无妥协立即拉响警报并终止",
    },
    {
      key: "cleanupPeriodDays",
      label: "Cleanup Days",
      type: "number",
      min: 0,
      max: 180,
      step: 1,
      default: 20,
      description:
        "自动记忆修剪期：设定清理阈值，超过指定天数未激活的历史孤儿工作区以及自动快照缓存，将被后台静默垃圾回收释放磁盘",
    },
    {
      key: "autoMemoryDirectory",
      label: "Memory Dir",
      type: "string",
      default: "~/.claude/memory",
      description:
        "记忆挂载坞坞站：定义整个大脑自动记忆簇快照记录的存放路径，非常适合将此文件夹外置进行跨设备映射和团队云同步",
    },
  ],
  codex: [
    {
      key: "approval_policy",
      label: "Approval",
      type: "segment",
      options: [
        { value: "untrusted", label: "Untrusted" },
        { value: "on-request", label: "On Request" },
        { value: "never", label: "Never" },
      ],
      default: "on-request",
      description:
        "操作时的弹窗拦截强度：\nUntrusted: 每次操作必须手工点确认\nOn Request: 仅在执行敏感大动作时弹窗（推荐）\nNever: 完全信任并放行，绝不弹窗打扰",
    },
    {
      key: "sandbox_mode",
      label: "Sandbox",
      type: "segment",
      options: [
        { value: "workspace-write", label: "Workspace" },
        { value: "danger-full-access", label: "Full Access" },
      ],
      default: "workspace-write",
      description:
        "限制 AI 修改文件的活动范围：\nWorkspace: 只能读写被当前项目框住的文件\nFull Access: 授予跨域权限，可以读写整台电脑的所有位置（高危）",
    },
    {
      key: "model_reasoning_effort",
      label: "Reasoning",
      type: "segment",
      options: [
        { value: "none", label: "None" },
        { value: "minimal", label: "Min" },
        { value: "low", label: "Low" },
        { value: "medium", label: "Mid" },
        { value: "high", label: "High" },
        { value: "xhigh", label: "XHigh" },
      ],
      default: "medium",
      description:
        "思考推演的算力层级：\n选择越靠右侧，回答质量和逻辑往往越严密。代价是生成结果所需等待的时间呈指数级变长。",
    },
    {
      key: "web_search",
      label: "Search",
      type: "segment",
      options: [
        { value: "cached", label: "Cached" },
        { value: "live", label: "Live" },
        { value: "disabled", label: "Off" },
      ],
      default: "cached",
      description:
        "联网搜索时的缓存策略：\nCached: 优先读取前次的搜索记忆（省钱、快速）\nLive: 遇到需要检索时直接现查外网最新数据（适合前沿问题）\nOff: 断开网络，只靠脑子里的知识硬猜",
    },
    {
      key: "context_group",
      label: "Context",
      type: "context_group",
      default: null,
      description:
        "人工限制代理的短期记忆大小：\nLIMIT: 上下文爆出来的绝对死亡上限\nCOMPACT: 触发自动遗忘废话内容的水位线\n注：保持 [Auto] 可将脑容量托管给引擎自动打理。",
    },
    {
      key: "features.fast_mode",
      label: "Fast Mode",
      type: "toggle",
      default: false,
      description: "极速模式：牺牲一部分深度写码逻辑以换取回复的秒级响应。",
    },
    {
      key: "features.smart_approvals",
      label: "Smart Appr.",
      type: "toggle",
      default: false,
      description: "智能放行：只拦截高危报错，其他常规命令不再问你直接后台运行。",
    },
    {
      key: "features.multi_agent",
      label: "Multi-Agent",
      type: "toggle",
      default: false,
      description: "多核并跑：允许启动后台小弟分头处理任务区逻辑扫描。",
    },
    {
      key: "show_raw_agent_reasoning",
      label: "Raw Reason",
      type: "toggle",
      default: false,
      description: "明盘思考：让底层的深奥打草稿过程也赤裸裸显示在输出日志中。",
    },
    {
      key: "model_verbosity",
      label: "Verbosity",
      type: "segment",
      options: [
        { value: "low", label: "Low" },
        { value: "normal", label: "Norm" },
        { value: "high", label: "High" },
      ],
      default: "normal",
      description:
        "对话交流的啰嗦程度：\nLow: 惜字如金类型，扔下代码就走\nNorm: 正常交流，带着设计解释\nHigh: 极其话痨，把心里想的每一句话都写出来报告",
    },
    {
      key: "shell_environment_policy.inherit",
      label: "Env Inherit",
      type: "segment",
      options: [
        { value: "all", label: "All" },
        { value: "core", label: "Core" },
        { value: "none", label: "None" },
      ],
      default: "all",
      description:
        "传递给终端的环境变量基座：\nAll: 继承电脑现有的所有环境变量（省事）\nCore: 净室模式，只保留必要的底层变量做安全防守\nNone: 完全切断基础环境变量（容易引起基础命令瘫痪）",
    },
    {
      key: "shell_environment_policy.ignore_default_excludes",
      label: "Env Exclude",
      type: "toggle",
      default: false,
      description:
        "敏感词封锁黑名单（高风险）：\n【开启】代表解除警报。因为系统默认会自动抛弃带 SECRET 或 TOKEN 的凭据变量。想要往进注入 GitHub Token，得在这里先放行解除警报。",
    },
    {
      key: "shell_environment_policy.include_only",
      label: "Env Match",
      type: "comma_array",
      default: "",
      description:
        "注入环境变量白名单（逗号分隔）：\n输入你想透穿的密钥，如 PATH, GH_TOKEN。只有存在于此名单的特权变量才能强行被送进后台沙箱。\n※ 需联手开启上方的解除警报",
    },
    {
      key: "history_persistence",
      label: "History",
      type: "segment",
      options: [
        { value: "local_only", label: "Local" },
        { value: "cloud_sync", label: "Cloud" },
        { value: "off", label: "Off" },
      ],
      default: "local_only",
      description:
        "对话记忆与历史留存方式：\nLocal: 记录只留存在本地盘上，不主动同步官方\nCloud: 启用同步功能，把现场环境记忆漫游\nOff: 拔管子，完全无痕模式打完即删",
    },
    {
      key: "telemetry.enabled",
      label: "Telemetry",
      type: "toggle",
      default: false,
      description:
        "传回遥测与报错数据：\n【开启】代表愿意帮官方发送非涉密的报错来修模型 Bug。\n【关闭】彻底切断此数据流保护隐私不受侵犯。",
    },
  ],
  opencode: [
    {
      key: "model",
      label: "Model",
      type: "select",
      default: "",
      dynamicOptionsFetcher: "get_opencode_cli_models",
      description: "设置默认的模型分配给该 OpenCode 环境",
    },
    {
      key: "permission.edit",
      label: "Edit",
      type: "segment",
      options: [
        { value: "allow", label: "Allow" },
        { value: "ask", label: "Ask" },
      ],
      default: "allow",
      description:
        "配置当代码代理试图修改受控文件系统时的拦截行为：\n\nAllow: 信任代理，静默在后台直接完成本地 I/O 写入\nAsk: 强拦截，弹出代码 Diff 面板等待用户人工确认",
    },
    {
      key: "permission.bash",
      label: "Bash",
      type: "segment",
      options: [
        { value: "allow", label: "Allow" },
        { value: "ask", label: "Ask" },
      ],
      default: "allow",
      description:
        "配置代理在后台运行 Bash 脚本与终端管道时的行为：\n\nAllow: 极速模式，自动放行执行并收集标准输出\nAsk: 在终端运行前挂起流水线，等待人工安全授权",
    },
    {
      key: "share",
      label: "Share",
      type: "segment",
      options: [
        { value: "manual", label: "Manual" },
        { value: "auto", label: "Auto" },
        { value: "disabled", label: "Off" },
      ],
      default: "manual",
      description:
        "控制会话分析报告与云端共享资源的生成机制：\n\nManual: 极其克制，仅在人工主动点击分享时生成打包\nAuto: 每次结束时自动生成摘要日志并回传处理\nOff: 最高隐私保护，彻底关闭云端映射外放行为",
    },
  ],
  gemini: [],
};

// ── Official doc links ──────────────────────────────────────────────

const APP_DOC_LINKS: Record<ModelAppId, { url: string; label: string }> = {
  claude: {
    url: "https://code.claude.com/docs/en/settings",
    label: "Claude 配置文档",
  },
  codex: {
    url: "https://developers.openai.com/codex/config-basic",
    label: "Codex 配置文档",
  },
  opencode: {
    url: "https://opencode.ai/docs/config/",
    label: "OpenCode 配置文档",
  },
  gemini: {
    url: "https://aistudio.google.com/app/apikey",
    label: "Gemini 配置文档",
  },
};

// ── Sub-components ──────────────────────────────────────────────────

/** Compact segment pill group (like AppCapsuleSwitcher style) */
function SegmentPill({
  id,
  options,
  value,
  onChange,
  appColor,
}: {
  id: string;
  options: FieldOption[];
  value: string;
  onChange: (v: string) => void;
  appColor: string;
}) {
  return (
    <div className="inline-flex items-center rounded-full bg-muted/50 border border-border/60 p-0.5 relative z-0">
      {options.map((opt) => {
        const isActive = value === opt.value;
        return (
          <button
            key={opt.value}
            type="button"
            onClick={() => onChange(opt.value)}
            className={cn(
              "relative z-10 px-2.5 py-1 rounded-full text-[11px] font-medium transition-colors duration-150 whitespace-nowrap",
              isActive ? "text-white" : "text-muted-foreground hover:text-foreground",
            )}
          >
            {isActive && (
              <motion.div
                layoutId={`seg-${id}`}
                className="absolute inset-0 rounded-full -z-10"
                style={{ backgroundColor: appColor }}
                transition={{ type: "spring", stiffness: 500, damping: 35 }}
              />
            )}
            <span className="relative z-10">{opt.label}</span>
          </button>
        );
      })}
    </div>
  );
}

/** Compact inline select */
function InlineSelect({
  options,
  value,
  onChange,
}: {
  options: FieldOption[];
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="h-7 px-2 pr-6 rounded-full bg-muted/50 border border-border/60 text-[11px] font-medium text-foreground focus:outline-none focus:ring-1 focus:ring-primary/40 cursor-pointer appearance-none"
      style={{
        backgroundImage:
          "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%236b7280' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpolyline points='6 9 12 15 18 9'%3E%3C/polyline%3E%3C/svg%3E\")",
        backgroundRepeat: "no-repeat",
        backgroundPosition: "right 6px center",
      }}
    >
      {options.map((opt) => (
        <option key={opt.value} value={opt.value}>
          {opt.label}
        </option>
      ))}
    </select>
  );
}

/** Compact inline dynamic select */
function InlineDynamicSelect({
  fetcher,
  value,
  onChange,
}: {
  fetcher: string;
  value: string;
  onChange: (v: string) => void;
}) {
  const cacheKey = `cached_dynamic_options_${fetcher}`;

  const [options, setOptions] = useState<FieldOption[]>(() => {
    try {
      const cached = localStorage.getItem(cacheKey);
      if (cached) {
        return JSON.parse(cached);
      }
    } catch (e) {
      // ignore
    }
    return [];
  });
  const [loading, setLoading] = useState(false);

  const fetchModels = useCallback(
    (forceRefresh = false) => {
      // If we're not forcing a refresh and we already have cached data, do nothing
      if (!forceRefresh && localStorage.getItem(cacheKey)) {
        return;
      }
      setLoading(true);
      import("@tauri-apps/api/core").then(({ invoke }) => {
        invoke<string[]>(fetcher)
          .then((models) => {
            const opts = models.map((m) => ({ value: m, label: m }));
            setOptions(opts);
            try {
              localStorage.setItem(cacheKey, JSON.stringify(opts));
            } catch (e) {
              // ignore
            }
            setLoading(false);
          })
          .catch((err) => {
            console.error("Failed to fetch dynamic options:", err);
            setLoading(false);
          });
      });
    },
    [fetcher, cacheKey],
  );

  // Intentionally removed auto-fetch on mount (useEffect).
  // The user explicitly requested to wait for a manual refresh click.

  return (
    <div className="flex items-center gap-1">
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="h-7 px-2 pr-6 rounded-full bg-muted/50 border border-border/60 text-[11px] font-medium text-foreground focus:outline-none focus:ring-1 focus:ring-primary/40 cursor-pointer appearance-none max-w-[150px] truncate"
        style={{
          backgroundImage:
            "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%236b7280' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpolyline points='6 9 12 15 18 9'%3E%3C/polyline%3E%3C/svg%3E\")",
          backgroundRepeat: "no-repeat",
          backgroundPosition: "right 6px center",
        }}
      >
        <option value="" disabled className="text-muted-foreground">
          None
        </option>
        {value && !options.find((o) => o.value === value) && <option value={value}>{value}</option>}
        {options.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
      <button
        type="button"
        onClick={() => fetchModels(true)}
        disabled={loading}
        title="刷新可用模型"
        className="flex items-center justify-center p-1.5 rounded-full hover:bg-muted/80 text-muted-foreground transition-colors disabled:opacity-50 ml-1"
      >
        <RefreshCw className={cn("w-3.5 h-3.5", loading && "animate-spin")} />
      </button>
    </div>
  );
}

/** Compact toggle switch */
function InlineToggle({
  checked,
  onChange,
  appColor,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  appColor: string;
}) {
  return (
    <button
      type="button"
      onClick={() => onChange(!checked)}
      className={cn(
        "relative w-8 h-[18px] rounded-full transition-colors duration-200 border",
        checked ? "border-transparent" : "border-border/60 bg-muted/50",
      )}
      style={checked ? { backgroundColor: appColor } : undefined}
    >
      <motion.div
        className="absolute top-[2px] w-3 h-3 rounded-full bg-white shadow-sm"
        animate={{ left: checked ? 14 : 2 }}
        transition={{ type: "spring", stiffness: 500, damping: 35 }}
      />
    </button>
  );
}

/** Compact inline number input with optional toggle support */
function InlineNumber({
  value,
  onChange,
  min,
  max,
  step,
  toggleOnValue,
  appColor,
}: {
  value: number | null;
  onChange: (v: number | null) => void;
  min?: number;
  max?: number;
  step?: number;
  toggleOnValue?: number;
  appColor?: string;
}) {
  const hasToggle = toggleOnValue !== undefined;
  const isOn = value !== null;

  return (
    <div className="flex items-center gap-2">
      {hasToggle && (
        <InlineToggle
          checked={isOn}
          onChange={(on) => onChange(on ? toggleOnValue : null)}
          appColor={appColor || "#ffffff"}
        />
      )}
      {(!hasToggle || isOn) && (
        <input
          type="number"
          value={value === null ? "" : value}
          min={min}
          max={max}
          step={step}
          onChange={(e) => {
            if (e.target.value === "") {
              if (!hasToggle) onChange(null);
              return;
            }
            const val = parseInt(e.target.value, 10);
            if (!isNaN(val)) onChange(val);
          }}
          className="h-7 w-[80px] px-2 rounded-lg bg-muted/50 border border-border/60 text-[11px] font-mono text-foreground focus:outline-none focus:ring-1 focus:ring-primary/40 appearance-none"
          style={{ MozAppearance: "textfield" }}
        />
      )}
    </div>
  );
}

function InlineText({
  value,
  onChange,
  placeholder,
  widthClass = "w-[120px]",
}: {
  value: string | null;
  onChange: (v: string | null) => void;
  placeholder?: string;
  widthClass?: string;
}) {
  const [local, setLocal] = useState<string>(value || "");
  useEffect(() => setLocal(value || ""), [value]);

  return (
    <input
      type="text"
      value={local}
      placeholder={placeholder}
      onChange={(e) => setLocal(e.target.value)}
      onBlur={(e) => onChange(e.target.value || null)}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
      }}
      className={cn(
        "h-7 px-2 rounded-lg bg-muted/50 border border-border/60 text-[11px] font-mono text-foreground focus:outline-none focus:ring-1 focus:ring-primary/40",
        widthClass,
      )}
    />
  );
}

/** Robust blurred input to prevent jumping cursor during optimistic sync */
function DebouncedNumberInput({
  value,
  onChange,
  placeholder,
}: {
  value: number | null | undefined;
  onChange: (v: number | null) => void;
  placeholder?: string;
}) {
  const [local, setLocal] = useState(value === null || value === undefined ? "" : String(value));

  useEffect(() => {
    setLocal(value === null || value === undefined ? "" : String(value));
  }, [value]);

  return (
    <input
      type="number"
      value={local}
      placeholder={placeholder}
      onChange={(e) => setLocal(e.target.value)}
      onBlur={() => {
        if (local === "") {
          onChange(null);
        } else {
          onChange(parseInt(local, 10));
        }
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
      }}
      className="h-7 w-[86px] px-2 rounded-lg bg-muted/50 border border-border/60 text-[11px] font-mono text-foreground focus:outline-none focus:ring-1 focus:ring-primary/40 appearance-none"
      style={{ MozAppearance: "textfield" }}
    />
  );
}

// ── Main component ──────────────────────────────────────────────────

interface BehaviorStripProps {
  appId: ModelAppId;
  appColor: string;
  /** Current active provider (to extract endpoint for model fetching) */
  currentProvider?: ProviderEntry;
}

export function BehaviorStrip({ appId, appColor }: BehaviorStripProps) {
  const { get, set, loading } = useAppSettings(appId);
  const fields = useMemo(() => APP_BEHAVIORS[appId], [appId]);
  const docLink = APP_DOC_LINKS[appId];

  const resolveValue = useCallback(
    (field: BehaviorField): unknown => {
      const val = get(field.key);
      if (val === undefined || val === null) return field.default;
      // Claude permissions is a special case — it can be a string or an object
      if (field.key === "permissions" && typeof val === "object") {
        return (val as Record<string, unknown>).defaultMode ?? field.default;
      }
      return val;
    },
    [get],
  );

  const handleChange = useCallback(
    async (field: BehaviorField, newValue: unknown) => {
      // Claude permissions special case
      if (field.key === "permissions") {
        await set("permissions", { defaultMode: newValue });
        return;
      }

      // For Codex advanced toggles: when turned off, we want to completely remove
      // the key from the config file rather than keeping it as `false`.
      let finalValue = newValue;
      if (appId === "codex" && field.type === "toggle") {
        finalValue = newValue === false ? null : newValue;
      }

      await set(field.key, finalValue);
    },
    [set, appId],
  );

  const [isOpen, setIsOpen] = useState(false);

  if (fields.length === 0) return null;

  return (
    <div className="absolute left-0 top-[73px] z-50 flex items-start">
      <AnimatePresence>
        {isOpen && (
          <motion.div
            initial={{ width: 0, opacity: 0 }}
            animate={{ width: "auto", opacity: 1 }}
            exit={{ width: 0, opacity: 0 }}
            transition={{ duration: 0.2, ease: "easeInOut" }}
            className="overflow-hidden bg-card/95 backdrop-blur-md border border-l-0 border-border shadow-xl rounded-r-2xl h-auto relative"
          >
            <div className="w-[420px] p-5 flex flex-col gap-5 max-h-[85vh] overflow-y-auto">
              <div className="flex items-center gap-2 mb-1">
                <span className="w-4 h-4 flex items-center justify-center text-muted-foreground">
                  <TreePadIcon />
                </span>
                <span className="text-sm font-medium text-foreground">高级配置 (Behavior)</span>
              </div>

              {loading ? (
                <div className="flex items-center justify-center py-4">
                  <Loader2 className="w-4 h-4 animate-spin text-muted-foreground" />
                </div>
              ) : (
                <div className="flex flex-col gap-4">
                  <div className="flex flex-col gap-0">
                    {/* Controls rows */}
                    {fields.map((field) => {
                      if (field.dependsOn) {
                        const dependencyField = fields.find((f) => f.key === field.dependsOn);
                        if (dependencyField) {
                          const depValue = resolveValue(dependencyField);
                          if (depValue === null || depValue === undefined || depValue === false) {
                            return null;
                          }
                        }
                      }
                      const currentValue = resolveValue(field);
                      return (
                        <div
                          key={field.key}
                          className="flex items-center justify-between gap-4 py-3 border-b border-border/30 last:border-0"
                        >
                          <div className="flex items-center">
                            <span className="text-[13px] font-medium text-foreground">{field.label}</span>
                            {field.description && <InfoTip text={field.description} />}
                          </div>
                          <div className="shrink-0 flex items-center justify-end">
                            {field.type === "select" && field.options && (
                              <InlineSelect
                                options={field.options}
                                value={String(currentValue)}
                                onChange={(v) => handleChange(field, v)}
                              />
                            )}
                            {field.type === "select" && field.dynamicOptionsFetcher && (
                              <InlineDynamicSelect
                                fetcher={field.dynamicOptionsFetcher}
                                value={currentValue === null ? "" : String(currentValue)}
                                onChange={(v) => handleChange(field, v)}
                              />
                            )}
                            {field.type === "segment" && field.options && (
                              <SegmentPill
                                id={field.key}
                                options={field.options}
                                value={String(currentValue)}
                                onChange={(v) => handleChange(field, v)}
                                appColor={appColor}
                              />
                            )}
                            {field.type === "toggle" && (
                              <InlineToggle
                                checked={Boolean(currentValue)}
                                onChange={(v) => handleChange(field, v)}
                                appColor={appColor}
                              />
                            )}
                            {field.type === "number" && (
                              <InlineNumber
                                value={currentValue === null ? null : Number(currentValue)}
                                onChange={(v) => handleChange(field, v)}
                                min={field.min}
                                max={field.max}
                                step={field.step}
                                toggleOnValue={field.toggleOnValue}
                                appColor={appColor}
                              />
                            )}
                            {field.type === "string" && (
                              <InlineText
                                value={currentValue === null ? null : String(currentValue)}
                                onChange={(v) => handleChange(field, v)}
                                placeholder={field.default as string}
                              />
                            )}
                            {field.type === "comma_array" && (
                              <InlineText
                                widthClass="w-[200px]"
                                value={(() => {
                                  if (currentValue === null || currentValue === undefined) return null;
                                  const str = String(currentValue);
                                  if (str.startsWith("[")) {
                                    try {
                                      // Parse raw TOML array like ["PATH", "HOME"] back to "PATH, HOME"
                                      const arr = JSON.parse(str);
                                      if (Array.isArray(arr)) return arr.join(", ");
                                    } catch {
                                      return str;
                                    }
                                  }
                                  return str;
                                })()}
                                onChange={(v) => {
                                  if (!v || !v.trim()) {
                                    handleChange(field, null);
                                  } else {
                                    // Serialize comma-separated string back to TOML array
                                    const arr = v
                                      .split(",")
                                      .map((s) => s.trim())
                                      .filter(Boolean);
                                    handleChange(field, `[${arr.map((x) => `"${x}"`).join(", ")}]`);
                                  }
                                }}
                                placeholder="如: PATH, HOME"
                              />
                            )}
                            {field.type === "context_group" && (
                              <div className="flex items-center gap-2">
                                <span className="text-[10px] text-muted-foreground uppercase opacity-80 pl-1">
                                  Limit
                                </span>
                                <DebouncedNumberInput
                                  value={get("model_context_window") as number | null | undefined}
                                  onChange={(v) =>
                                    set("model_context_window", v === null ? null : Math.min(v, 1000000))
                                  }
                                  placeholder="Auto"
                                />
                                <span className="text-[10px] text-muted-foreground uppercase opacity-80 pl-1">
                                  Compact
                                </span>
                                <DebouncedNumberInput
                                  value={get("model_auto_compact_token_limit") as number | null | undefined}
                                  onChange={(v) =>
                                    set("model_auto_compact_token_limit", v === null ? null : Math.min(v, 1000000))
                                  }
                                  placeholder="Auto"
                                />
                              </div>
                            )}
                          </div>
                        </div>
                      );
                    })}
                  </div>

                  {/* Footer: hint + doc link */}
                  <div className="flex items-center justify-between mt-2 pt-3 border-t border-border/50">
                    <span className="text-[10px] text-muted-foreground/60">即时生效 · 更多设置在「配置文件」</span>
                    <button
                      type="button"
                      onClick={() => void openExternalUrl(docLink.url)}
                      className="flex items-center gap-1 text-[11px] text-muted-foreground/70 hover:text-primary transition-colors"
                    >
                      <ExternalLink className="w-3 h-3" />
                      {docLink.label}
                    </button>
                  </div>
                </div>
              )}
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <button
        onClick={() => setIsOpen(!isOpen)}
        className={cn(
          "flex items-center justify-center w-[24px] h-[52px] mt-[52px] bg-card/90 backdrop-blur-md border border-l-0 border-border/80 transition-all z-50 text-muted-foreground",
          "rounded-r-[12px] shadow-[2px_0_8px_rgba(0,0,0,0.06)] dark:shadow-[4px_0_12px_rgba(0,0,0,0.2)]",
          "hover:bg-accent/40 hover:text-foreground hover:scale-[1.03] active:scale-[0.97]",
          isOpen ? "ml-[-1px] rounded-l-none text-primary" : "",
        )}
        title="行为配置"
      >
        <span className="transition-transform duration-300">
          <TreePadIcon />
        </span>
      </button>
    </div>
  );
}
