import { CheckCircle2, Cookie, Copy, Loader2, RefreshCw, RotateCcw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { toast } from "../../../lib/toast";
import { cn } from "../../../lib/utils";
import { usageApi } from "../../usage/api";

type BridgePhase = "idle" | "pairing" | "bound" | "error";

export function CookieBridgeSection() {
  const [phase, setPhase] = useState<BridgePhase>("idle");
  const [token, setToken] = useState("");
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [status, setStatus] = useState("在这里生成绑定码，然后粘贴到浏览器插件。插件本身不会生成绑定码。");
  const [loading, setLoading] = useState(false);
  const pollStopRef = useRef(false);

  useEffect(() => {
    void refreshStatus();
    return () => {
      pollStopRef.current = true;
    };
  }, []);

  const refreshStatus = async () => {
    try {
      const next = await usageApi.getCookieBridgeBindingStatus("opencode");
      setPhase(next.bound ? "bound" : "idle");
      setStatus(
        next.bound
          ? "SkillStar 已保存插件绑定。浏览器插件可直接推送最新 Cookie。"
          : "尚未绑定。生成绑定码后，在插件里粘贴并完成一次推送。",
      );
    } catch (error) {
      setPhase("error");
      setStatus(error instanceof Error ? error.message : String(error));
    }
  };

  const generateBindingCode = async () => {
    setLoading(true);
    pollStopRef.current = false;
    try {
      if (sessionId) await usageApi.cancelCookieImportSession(sessionId).catch(() => undefined);
      const session = await usageApi.startCookieImportSession("opencode");
      setToken(session.token);
      setSessionId(session.session_id);
      setPhase("pairing");
      setStatus("绑定码已复制。打开插件，粘贴绑定码并点击“绑定并推送”。");
      await navigator.clipboard.writeText(session.token).catch(() => undefined);
      toast.success("绑定码已复制");
      void pollImportStatus(session.session_id, session.expires_in_secs);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setPhase("error");
      setStatus(message);
      toast.error(message);
    } finally {
      setLoading(false);
    }
  };

  const pollImportStatus = async (nextSessionId: string, ttlSecs: number) => {
    const startedAt = Date.now();
    while (!pollStopRef.current && Date.now() - startedAt < ttlSecs * 1000) {
      await new Promise((resolve) => setTimeout(resolve, 1500));
      const next = await usageApi.getCookieImportStatus(nextSessionId);
      if (next.status === "completed") {
        setPhase("bound");
        setSessionId(null);
        setToken("");
        setStatus("绑定成功。后续可在插件里直接点击推送 OpenCode Cookie。");
        toast.success("Cookie Bridge 已绑定");
        return;
      }
      if (next.status === "error" || next.status === "expired") {
        setPhase("error");
        setSessionId(null);
        setStatus(next.error ?? "绑定失败，请重新生成绑定码。");
        return;
      }
    }
    if (!pollStopRef.current) {
      setPhase("error");
      setSessionId(null);
      setStatus("绑定码已过期，请重新生成。");
    }
  };

  const cancelPairing = async () => {
    if (sessionId) await usageApi.cancelCookieImportSession(sessionId).catch(() => undefined);
    setSessionId(null);
    setToken("");
    setPhase("idle");
    setStatus("已取消当前绑定码。需要绑定时重新生成即可。");
  };

  const resetBinding = async () => {
    setLoading(true);
    try {
      await usageApi.resetCookieBridgeBinding("opencode");
      setPhase("idle");
      setToken("");
      setSessionId(null);
      setStatus("SkillStar 侧绑定已重置。请同时在浏览器插件里解除本机绑定。");
    } catch (error) {
      setPhase("error");
      setStatus(error instanceof Error ? error.message : String(error));
    } finally {
      setLoading(false);
    }
  };

  return (
    <section>
      <div className="mb-3 flex items-center justify-between px-1">
        <div className="flex items-center gap-2">
          <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-primary/20 bg-primary/10">
            <Cookie className="h-4 w-4 text-primary" />
          </div>
          <h2 className="text-sm font-semibold tracking-tight text-foreground">Cookie Bridge</h2>
        </div>
        <StatusPill phase={phase} />
      </div>

      <div className="rounded-2xl border border-border bg-card p-4">
        <div className="grid gap-4">
          <div className="grid grid-cols-3 gap-2 text-[11px]">
            <Step active={phase === "idle" || phase === "pairing"} done={phase === "bound"} label="1. 生成绑定码" />
            <Step active={phase === "pairing"} done={phase === "bound"} label="2. 插件粘贴并推送" />
            <Step active={phase === "bound"} done={phase === "bound"} label="3. 后续一键推送" />
          </div>

          <div className="space-y-2">
            <label className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">绑定码</label>
            <div className="flex gap-2">
              <Input
                value={token}
                readOnly
                placeholder="点击“生成绑定码”"
                className="h-9 flex-1 rounded-xl border-input-border bg-input text-xs text-foreground"
              />
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => navigator.clipboard.writeText(token).catch(() => undefined)}
                disabled={!token}
              >
                <Copy className="mr-1 h-3.5 w-3.5" />
                复制
              </Button>
            </div>
          </div>

          <p
            className={cn(
              "rounded-xl border px-3 py-2 text-[11px] leading-relaxed",
              phase === "error"
                ? "border-destructive/25 bg-destructive/10 text-destructive"
                : "border-border bg-muted/35 text-muted-foreground",
            )}
          >
            {status}
          </p>

          <div className="flex flex-wrap gap-2">
            <Button type="button" size="sm" onClick={generateBindingCode} disabled={loading}>
              {loading ? <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" /> : null}
              {phase === "pairing" ? "重新生成" : "生成绑定码"}
            </Button>
            <Button type="button" size="sm" variant="outline" onClick={refreshStatus} disabled={loading}>
              <RefreshCw className="mr-1 h-3.5 w-3.5" />
              刷新状态
            </Button>
            {phase === "pairing" && (
              <Button type="button" size="sm" variant="outline" onClick={cancelPairing}>
                取消配对
              </Button>
            )}
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={resetBinding}
              disabled={loading || phase !== "bound"}
            >
              <RotateCcw className="mr-1 h-3.5 w-3.5" />
              重置绑定
            </Button>
          </div>
        </div>
      </div>
    </section>
  );
}

function StatusPill({ phase }: { phase: BridgePhase }) {
  const label =
    phase === "bound" ? "已绑定" : phase === "pairing" ? "等待插件" : phase === "error" ? "需要处理" : "未绑定";
  return (
    <span
      className={cn(
        "rounded-full border px-2.5 py-1 text-[11px] font-semibold",
        phase === "bound"
          ? "border-primary/25 bg-primary/10 text-primary"
          : phase === "error"
            ? "border-destructive/25 bg-destructive/10 text-destructive"
            : "border-border bg-muted/40 text-muted-foreground",
      )}
    >
      {label}
    </span>
  );
}

function Step({ active, done, label }: { active: boolean; done: boolean; label: string }) {
  return (
    <div
      className={cn(
        "flex items-center gap-1.5 rounded-xl border px-2 py-2",
        done
          ? "border-primary/25 bg-primary/10 text-primary"
          : active
            ? "border-border bg-muted/50 text-foreground"
            : "border-border/60 bg-muted/20 text-muted-foreground",
      )}
    >
      {done ? <CheckCircle2 className="h-3.5 w-3.5" /> : <span className="h-1.5 w-1.5 rounded-full bg-current" />}
      <span className="truncate font-semibold">{label}</span>
    </div>
  );
}
