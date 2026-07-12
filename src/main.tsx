import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { ThemeProvider } from "./components/ThemeProvider";
import { ToastProvider } from "./components/shared/Toast";
import "./styles/globals.css";

// ThemeProvider wraps the whole app so the brand theme is applied to every
// screen — including Splash/Welcome/Connection (ADR-004), which previously
// rendered outside the provider and ignored the user's theme choice.
// ToastProvider provides global toast notifications (replaces console.error).
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ThemeProvider>
      <ToastProvider>
        <App />
      </ToastProvider>
    </ThemeProvider>
  </React.StrictMode>
);
