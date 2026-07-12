// src/components/ErrorBoundary.tsx
import { Component, type ReactNode } from "react";
import { Copy, Check, RefreshCw } from "lucide-react";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error: string;
  copied: boolean;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, error: "", copied: false };

  static getDerivedStateFromError(error: Error) {
    return { hasError: true, error: error.message || String(error) };
  }

  handleReload = () => {
    window.location.reload();
  };

  handleCopy = () => {
    navigator.clipboard.writeText(this.state.error).then(() => {
      this.setState({ copied: true });
      setTimeout(() => this.setState({ copied: false }), 2000);
    });
  };

  render() {
    if (this.state.hasError) {
      return (
        <div className="fixed inset-0 bg-ac-bg flex items-center justify-center p-8">
          <div className="max-w-md text-center">
            <p className="text-ac-red text-lg mb-4">Something went wrong</p>
            <pre className="text-xs text-ac-muted bg-ac-bg p-4 rounded mb-4 text-left overflow-auto max-h-48">
              {this.state.error}
            </pre>
            <div className="flex items-center justify-center gap-2">
              <button
                onClick={this.handleCopy}
                className="px-4 py-2 text-sm rounded-lg border border-ac-border text-ac-muted hover:text-ac-brand hover:border-ac-brand transition-colors flex items-center gap-1.5"
              >
                {this.state.copied ? (
                  <>
                    <Check className="w-4 h-4" /> Copied
                  </>
                ) : (
                  <>
                    <Copy className="w-4 h-4" /> Copy
                  </>
                )}
              </button>
              <button
                onClick={this.handleReload}
                className="ac-btn px-4 py-2 text-sm flex items-center gap-1.5"
              >
                <RefreshCw className="w-4 h-4" /> Reload
              </button>
            </div>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
