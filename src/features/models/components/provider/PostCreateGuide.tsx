import { ArrowRight, Check, Plug, X, Zap } from "lucide-react";
import { Button } from "../../../../components/ui/button";
import { cn } from "../../../../lib/utils";

export interface PostCreateGuideProps {
  /** Step 3 done — an agent already got bound during create (autoBind). */
  agentBound: boolean;
  onTestConnection: () => void;
  onGoConnect: () => void;
  onDismiss: () => void;
}

const STEPS = ["添加供应商", "测试连接", "接入 Agent"] as const;

/** One-time banner shown in the editor drawer right after a provider is created. */
export function PostCreateGuide({ agentBound, onTestConnection, onGoConnect, onDismiss }: PostCreateGuideProps) {
  const doneIndex = agentBound ? 2 : 0;
  return (
    <div className="rounded-xl border border-primary/25 bg-primary/[0.06] px-3.5 py-3" role="status">
      <div className="flex items-start gap-2">
        <span className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-primary/15">
          <Check className="h-3 w-3 text-primary" />
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-xs font-semibold text-foreground">供应商已创建</p>
          <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
            {STEPS.map((label, i) => (
              <span key={label} className="flex items-center gap-1.5">
                <span
                  className={cn(
                    "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10px] font-medium",
                    i <= doneIndex
                      ? "border-primary/35 bg-primary/10 text-primary"
                      : "border-border/55 text-muted-foreground",
                  )}
                >
                  {i <= doneIndex ? <Check className="h-2.5 w-2.5" /> : <span className="font-mono">{i + 1}</span>}
                  {label}
                </span>
                {i < STEPS.length - 1 ? <ArrowRight className="h-3 w-3 text-muted-foreground/50" /> : null}
              </span>
            ))}
          </div>
          <div className="mt-2 flex flex-wrap gap-1.5">
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="h-7 gap-1 text-[11px]"
              onClick={onTestConnection}
            >
              <Zap className="h-3 w-3" />
              测试连接
            </Button>
            {!agentBound ? (
              <Button type="button" size="sm" variant="outline" className="h-7 gap-1 text-[11px]" onClick={onGoConnect}>
                <Plug className="h-3 w-3" />
                去接入 Agent
              </Button>
            ) : null}
          </div>
        </div>
        <button
          type="button"
          onClick={onDismiss}
          aria-label="关闭引导"
          className="shrink-0 rounded p-0.5 text-muted-foreground/70 transition hover:text-foreground"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  );
}
