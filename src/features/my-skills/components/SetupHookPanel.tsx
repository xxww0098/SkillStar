import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import {
  Terminal,
  Play,
  Trash2,
  Pencil,
  Bot,
  Check,
  AlertCircle,
  Clock,
  Loader2,
} from "lucide-react";
import { Button } from "../../../components/ui/button";
import { toast } from "sonner";

interface SetupHook {
  skill_name: string;
  script: string;
  last_run: string | null;
  last_result: "success" | "failed" | "timeout" | null;
  last_output: string | null;
}

interface HookRunOutput {
  result: "success" | "failed" | "timeout";
  stdout: string;
  stderr: string;
  duration_ms: number;
}

interface SetupHookPanelProps {
  skillName: string;
  installed: boolean;
}

export function SetupHookPanel({ skillName, installed }: SetupHookPanelProps) {
  const { t } = useTranslation();
  const [hook, setHook] = useState<SetupHook | null>(null);
  const [loading, setLoading] = useState(true);
  const [running, setRunning] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [editing, setEditing] = useState(false);
  const [editScript, setEditScript] = useState("");
  const [showOutput, setShowOutput] = useState(false);

  // Load hook on mount / skill change
  const loadHook = useCallback(async () => {
    if (!installed) {
      setHook(null);
      setLoading(false);
      return;
    }
    try {
      const result = await invoke<SetupHook | null>("get_setup_hook", {
        skillName,
      });
      setHook(result);
    } catch {
      setHook(null);
    } finally {
      setLoading(false);
    }
  }, [skillName, installed]);

  useEffect(() => {
    setLoading(true);
    setEditing(false);
    setShowOutput(false);
    loadHook();
  }, [loadHook]);

  // Generate via ACP
  const handleGenerate = useCallback(async () => {
    setGenerating(true);
    try {
      // Load the user's ACP config to get the agent command
      const config = await invoke<{ agent_command: string }>("get_acp_config");
      const result = await invoke<SetupHook>("acp_generate_setup_hook", {
        skillName,
        agentCommand: config.agent_command,
      });
      setHook(result);
      toast.success(t("setupHook.generateSuccess"));
    } catch (e: any) {
      toast.error(t("setupHook.generateFailed"), {
        description: e?.toString?.() || "Unknown error",
      });
    } finally {
      setGenerating(false);
    }
  }, [skillName, t]);

  // Run hook
  const handleRun = useCallback(async () => {
    setRunning(true);
    try {
      const output = await invoke<HookRunOutput>("run_setup_hook", {
        skillName,
      });
      if (output.result === "success") {
        toast.success(t("setupHook.runSuccess"), {
          description: `${output.duration_ms}ms`,
        });
      } else {
        toast.error(t("setupHook.runFailed"), {
          description: output.stderr.slice(0, 200),
        });
      }
      await loadHook(); // refresh metadata
    } catch (e: any) {
      toast.error(t("setupHook.runFailed"), {
        description: e?.toString?.(),
      });
    } finally {
      setRunning(false);
    }
  }, [skillName, t, loadHook]);

  // Save edited script
  const handleSave = useCallback(async () => {
    try {
      await invoke("save_setup_hook", {
        skillName,
        script: editScript,
      });
      setEditing(false);
      await loadHook();
      toast.success(t("common.saved"));
    } catch (e: any) {
      toast.error(t("setupHook.saveFailed"));
    }
  }, [skillName, editScript, t, loadHook]);

  // Delete hook
  const handleDelete = useCallback(async () => {
    try {
      await invoke("delete_setup_hook", { skillName });
      setHook(null);
      setEditing(false);
      toast.success(t("setupHook.deleted"));
    } catch {
      toast.error(t("setupHook.deleteFailed"));
    }
  }, [skillName, t]);

  if (!installed || loading) return null;

  const statusIcon = () => {
    if (!hook?.last_result) return null;
    switch (hook.last_result) {
      case "success":
        return <Check className="w-3.5 h-3.5 text-emerald-400" />;
      case "failed":
        return <AlertCircle className="w-3.5 h-3.5 text-red-400" />;
      case "timeout":
        return <Clock className="w-3.5 h-3.5 text-amber-400" />;
    }
  };

  return (
    <div className="space-y-2 rounded-xl border border-border/50 bg-card/30 p-3">
      <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
        <Terminal className="w-3.5 h-3.5" />
        {t("setupHook.title")}
      </div>

      {!hook ? (
        /* No hook yet — show generate button */
        <Button
          variant="outline"
          size="sm"
          className="w-full text-xs"
          disabled={generating}
          onClick={handleGenerate}
        >
          {generating ? (
            <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
          ) : (
            <Bot className="w-3.5 h-3.5 mr-1.5" />
          )}
          {generating
            ? t("setupHook.generating")
            : t("setupHook.generateViaAcp")}
        </Button>
      ) : editing ? (
        /* Edit mode */
        <div className="space-y-2">
          <textarea
            className="w-full h-32 rounded-lg bg-background/60 border border-border/50 p-2 text-xs font-mono resize-y focus:outline-none focus:ring-1 focus:ring-primary/50"
            value={editScript}
            onChange={(e) => setEditScript(e.target.value)}
            spellCheck={false}
          />
          <div className="flex gap-1.5">
            <Button size="sm" className="flex-1 text-xs" onClick={handleSave}>
              {t("common.save")}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className="text-xs"
              onClick={() => setEditing(false)}
            >
              {t("common.cancel")}
            </Button>
          </div>
        </div>
      ) : (
        /* Hook exists — show info + actions */
        <div className="space-y-2">
          {/* Script preview */}
          <div
            className="rounded-lg bg-background/40 p-2 text-[11px] font-mono leading-relaxed max-h-20 overflow-y-auto text-muted-foreground cursor-pointer hover:text-foreground transition-colors"
            onClick={() => setShowOutput(!showOutput)}
            title={t("setupHook.clickToToggle")}
          >
            {showOutput && hook.last_output
              ? hook.last_output
              : hook.script.split("\n").slice(0, 4).join("\n") +
                (hook.script.split("\n").length > 4 ? "\n…" : "")}
          </div>

          {/* Status line */}
          {hook.last_run && (
            <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
              {statusIcon()}
              <span>
                {t("setupHook.lastRun", {
                  time: new Date(hook.last_run).toLocaleString(),
                })}
              </span>
            </div>
          )}

          {/* Action buttons */}
          <div className="flex gap-1.5">
            <Button
              variant="outline"
              size="sm"
              className="flex-1 text-xs"
              disabled={running}
              onClick={handleRun}
            >
              {running ? (
                <Loader2 className="w-3 h-3 mr-1 animate-spin" />
              ) : (
                <Play className="w-3 h-3 mr-1" />
              )}
              {running ? t("setupHook.running") : t("setupHook.run")}
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8"
              onClick={() => {
                setEditScript(hook.script);
                setEditing(true);
              }}
              title={t("common.edit")}
            >
              <Pencil className="w-3 h-3" />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 text-destructive/70 hover:text-destructive"
              onClick={handleDelete}
              title={t("common.delete")}
            >
              <Trash2 className="w-3 h-3" />
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
