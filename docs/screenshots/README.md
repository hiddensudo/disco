# Screenshot sources

The 0.2.3-unstable guide images are native disco captures, not UI mockups. They use a disposable profile and synthetic files under `/tmp/disco-docs/`, with Carbon / Soft Rose and separate English / Ukrainian sessions. LSP panels use the C++ example in [`../examples/lsp/`](../examples/lsp/main.cpp) and a real system clangd.

Captures run on a private Xvfb display with software rendering. Input is sent only to that display; no visible windows or input events are sent to the user's desktop. Tight crops retain the original pixel resolution. No code, labels or application states are painted onto the screenshots.

| Images | Shown action |
| --- | --- |
| `lsp-settings*` | Enable language, check installation, inspect the running project |
| `lsp-completion*` | `comb` → one entry with three overloads |
| `lsp-signature*` | Accept → `(` → Alt+Right; overload 2/3 |
| `lsp-parameter*` | Enter `1,`; current second argument, narrowed signatures |
| `address*`, `filename-search*` | Ctrl+L path suggestions; filename search in Overview |
| `find*` | Ctrl+F floating find in the live editor |
| `tabs*`, `scratch-023*` | Kept/preview tabs; first Save of an untitled document |
| `markdown*`, `focus-023*` | Read/Edit toolbar, code block, table/disclosure, zoom; side terminal |
| `themes*`, `theme-prompt*`, `soft-rose*` | Accent swatches, pinned Create, generated prompt, light preset |
| `interface*`, `file-icons*`, `shortcuts-023*` | Independent settings, extension icons, shortcut recording page |

Names ending in `.uk.png` show Ukrainian UI. Prompt screenshots intentionally use the *other* prompt language to demonstrate its independence. English source-code comments and library documentation remain English in both UIs. Temporary paths, PIDs and resource readings are demonstration values, not configuration instructions or benchmarks.

Older `overview-terminal*`, `focus-terminal*`, `folders*`, `trash*` and `categories*` captures document earlier releases. The main guides link the current captures; historical images remain available.
