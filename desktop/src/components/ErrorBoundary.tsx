import React from "react";
import { AlertTriangle, RefreshCw } from "lucide-react";
import { Button } from "./common/Button";

interface Props { children: React.ReactNode; }
interface State { hasError: boolean; error: string | null; }

export class ErrorBoundary extends React.Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error: error.message };
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="h-screen flex items-center justify-center bg-app-bg">
          <div className="flex flex-col items-center gap-4 text-center max-w-md px-4">
            <div className="w-14 h-14 rounded-full bg-app-red-bg border border-[var(--app-red-bg)] flex items-center justify-center">
              <AlertTriangle size={28} className="text-app-red" />
            </div>
            <div>
              <div className="text-base text-app-text font-mono font-semibold mb-1">应用遇到了未预期的错误</div>
              <div className="text-sm text-app-text-dim mb-2">
                {this.state.error ? (
                  <code className="text-xs text-app-red bg-app-red-bg px-2 py-0.5 block mt-1 max-h-[100px] overflow-auto font-mono">
                    {this.state.error}
                  </code>
                ) : null}
              </div>
              <Button
                variant="primary"
                size="md"
                onClick={() => { window.location.reload(); }}
              >
                <RefreshCw size={14} />
                重新加载
              </Button>
            </div>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
