/// Point `@monaco-editor/react` at the bundled copy of Monaco.
///
/// Without this the package fetches the editor from `cdn.jsdelivr.net` at
/// runtime. For a *local* development tool that's the wrong dependency in
/// three ways: the editor never loads offline, every editor window pings a
/// third party, and it would break the moment a Content-Security-Policy is
/// set on the webview.
///
/// Importing this module for its side effects is enough — do it before the
/// first `<Editor>` mounts.

import * as monaco from "monaco-editor";
import { loader } from "@monaco-editor/react";
// monaco-editor 0.56 exposes its internals through an exports map that
// rewrites `monaco-editor/<path>` to `esm/vs/<path>.js`, so these specifiers
// deliberately omit the `esm/vs/` prefix the older docs use.
import editorWorker from "monaco-editor/editor/editor.worker.js?worker";
import jsonWorker from "monaco-editor/language/json/json.worker.js?worker";
import cssWorker from "monaco-editor/language/css/css.worker.js?worker";
import htmlWorker from "monaco-editor/language/html/html.worker.js?worker";
import tsWorker from "monaco-editor/language/typescript/ts.worker.js?worker";

// Monaco loads its language services in web workers and asks the host how to
// construct them. Vite's `?worker` imports give us bundled worker classes, so
// these resolve locally like everything else.
self.MonacoEnvironment = {
  getWorker(_workerId: string, label: string) {
    switch (label) {
      case "json":
        return new jsonWorker();
      case "css":
      case "scss":
      case "less":
        return new cssWorker();
      case "html":
      case "handlebars":
      case "razor":
        return new htmlWorker();
      case "typescript":
      case "javascript":
        return new tsWorker();
      default:
        // PHP, SQL, INI, YAML and friends are tokenised on the main thread —
        // Monaco ships no dedicated worker for them.
        return new editorWorker();
    }
  },
};

loader.config({ monaco });
