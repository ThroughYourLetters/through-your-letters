import React from "react";
import { AlertTriangle, RefreshCw } from "lucide-react";

interface Props {
  children: React.ReactNode;
  fallback?: React.ReactNode;
  onReload?: () => void;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

class ErrorBoundary extends React.Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error("ErrorBoundary caught:", error, info.componentStack);
  }

  handleRetry = () => {
    if (this.isChunkError()) {
      if (this.props.onReload) this.props.onReload();
      else window.location.reload();
      return;
    }
    this.setState({ hasError: false, error: null });
  };

  isChunkError = () =>
    /Failed to fetch dynamically imported module|Importing a module script failed|error loading dynamically imported module|Unable to preload CSS/i.test(
      this.state.error?.message ?? "",
    );

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) return this.props.fallback;

      return (
        <div
          role="alert"
          className="min-h-[400px] flex items-center justify-center p-8"
        >
          <div className="max-w-md w-full border border-line bg-surface p-10 shadow-card space-y-6 text-center">
            <AlertTriangle size={48} className="mx-auto text-terracotta" />
            <h2 className="font-display font-semibold tracking-tight text-2xl">
              {this.isChunkError()
                ? "This page could not load"
                : "Something went wrong"}
            </h2>
            <p className="text-sm font-medium text-stone-600">
              {this.isChunkError()
                ? "A site update or connection change interrupted loading. Reload the page to try again."
                : "Please try again. If the problem continues, reload this page."}
            </p>
            <button onClick={this.handleRetry} className="btn-dark">
              <RefreshCw size={14} />
              {this.isChunkError() ? "Reload page" : "Try again"}
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}

export default ErrorBoundary;
