// src/components/ErrorBoundary.tsx
import { Component, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error: string;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, error: "" };

  static getDerivedStateFromError(error: Error) {
    return { hasError: true, error: error.message || String(error) };
  }

  handleReload = () => {
    window.location.reload();
  };

  render() {
    if (this.state.hasError) {
      return (
        <div className="fixed inset-0 bg-ac-pitch flex items-center justify-center p-8">
          <div className="max-w-md text-center">
            <p className="text-ac-red text-lg mb-4">Something went wrong</p>
            <pre className="text-xs text-ac-stone bg-ac-bg p-4 rounded mb-4 text-left overflow-auto max-h-48">
              {this.state.error}
            </pre>
            <button
              onClick={this.handleReload}
              className="ac-btn px-4 py-2 text-sm"
            >
              Reload
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
