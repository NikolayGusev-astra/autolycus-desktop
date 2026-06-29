import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { ThemeProvider } from "./components/ThemeProvider";
import "./styles/globals.css";

// ThemeProvider wraps the whole app so the brand theme is applied to every
// screen — including Splash/Welcome/Connection (ADR-004), which previously
// rendered outside the provider and ignored the user's theme choice.
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ThemeProvider>
      <App />
    </ThemeProvider>
  </React.StrictMode>
);
