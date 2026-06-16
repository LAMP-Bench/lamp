import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import "./i18n";

// Block the webview's right-click "Inspect Element" menu. Devtools are still
// reachable in dev via F12 / Ctrl+Shift+I, but the default browser context
// menu just looks unprofessional in a native-looking app.
document.addEventListener("contextmenu", (e) => e.preventDefault());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
