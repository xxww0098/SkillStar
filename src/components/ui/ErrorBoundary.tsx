import { Component } from "react";
import type { ErrorInfo, ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
  errorInfo: ErrorInfo | null;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null, errorInfo: null };
  }

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    this.setState({ errorInfo: info });
    console.error(
      "[ErrorBoundary] Uncaught error:",
      error,
      "\nComponent stack:",
      info.componentStack,
    );
  }

  private handleReload = () => {
    this.setState({ hasError: false, error: null, errorInfo: null });
  };

  private formatErrorDetails(): string {
    const { error, errorInfo } = this.state;
    if (!error) return "Unknown error";
    const parts: string[] = [];
    if (error.name && error.name !== "Error") {
      parts.push(`[${error.name}]`);
    }
    parts.push(error.message || "Unknown error");
    if (error.stack) {
      // Show first few stack frames for context
      const stackLines = error.stack.split("\n").slice(1, 4).join("\n");
      if (stackLines) parts.push(stackLines);
    }
    if (errorInfo?.componentStack) {
      const compLines = errorInfo.componentStack
        .split("\n")
        .filter((l) => l.trim())
        .slice(0, 3)
        .join("\n");
      if (compLines) parts.push(`\nComponent:\n${compLines}`);
    }
    const detail = parts.join("\n");
    return detail.length > 500 ? detail.slice(0, 497) + "..." : detail;
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="flex-1 flex flex-col items-center justify-center gap-4 p-8 text-center">
          <div className="w-14 h-14 rounded-2xl bg-destructive/10 flex items-center justify-center text-destructive text-2xl">
            !
          </div>
          <h2 className="text-heading-md">Something went wrong</h2>
          <p className="text-caption max-w-md">
            An unexpected error occurred. You can try reloading the page.
          </p>
          <pre className="text-xs text-muted-foreground bg-muted rounded-lg p-3 max-w-lg overflow-auto max-h-40 text-left whitespace-pre-wrap break-words">
            {this.formatErrorDetails()}
          </pre>
          <div className="flex gap-3 mt-2">
            <button
              onClick={this.handleReload}
              className="px-4 py-2 rounded-lg bg-primary text-primary-foreground text-sm font-medium hover:bg-primary-hover transition-colors"
            >
              Reload
            </button>
            <button
              onClick={() => window.location.reload()}
              className="px-4 py-2 rounded-lg bg-muted text-foreground text-sm font-medium hover:bg-muted/80 transition-colors"
            >
              Restart App
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}

