import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button } from "@/components/ui/button";

interface State {
  error: Error | null;
}

/**
 * Without this, any rendering error unmounts the whole app and leaves a blank
 * window. Show what went wrong and offer a reload instead.
 */
export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // Component names and the message only; no transcript content is in here.
    console.error(error.message, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="flex h-screen flex-col items-center justify-center gap-3 p-6 text-center">
        <h1 className="text-base font-semibold">Something went wrong in the app</h1>
        <p className="max-w-lg text-sm text-muted-foreground">
          Your archive is unaffected. Reloading usually fixes this; if it keeps happening, please
          report the message below.
        </p>
        <pre className="max-w-lg rounded-md bg-muted p-3 text-left text-xs whitespace-pre-wrap">
          {this.state.error.message}
        </pre>
        <Button onClick={() => window.location.reload()}>Reload</Button>
      </div>
    );
  }
}
