
import { Loader2 } from "lucide-react";

export interface LoadingPhaseProps {
  message: string;
}

export function LoadingPhase({ message }: LoadingPhaseProps) {
  return (
    <div className="flex flex-col items-center justify-center py-16 gap-4">
      <div className="relative">
        <div className="w-12 h-12 rounded-2xl bg-primary/10 flex items-center justify-center">
          <Loader2 className="w-6 h-6 text-primary animate-spin" />
        </div>
      </div>
      <p className="text-sm text-muted-foreground">{message}</p>
    </div>
  );
}
