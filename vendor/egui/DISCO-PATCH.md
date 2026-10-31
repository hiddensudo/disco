# disco's opt-in action undo patch

Based on the existing egui 0.37.0 Cargo registry source (registry VCS metadata: `7c0b40402271e8d40cda69f87cf3c2617c2e4382`, `crates/egui`). The version and public default behavior are unchanged. Upstream: https://github.com/emilk/egui. MIT/Apache-2.0 licenses are retained here and in the runtime licenses directory.

The local adaptation adds `TextEdit::action_undo(bool)`, enabled only on disco's file editor. Other input fields retain egui's timer-based undo. Changed implementation files:

- `widgets/text_edit/builder.rs`: boundaries per input event, including multiple actions arriving in one frame; isolated paste/cut/Enter/Tab/selection replacement, cursor/type/pause grouping for typing and deletion, IME preedit/commit grouping, read-only undo/redo guards.
- `widgets/text_edit/state.rs`: transient per-widget grouping/source state, excluded from serde. Save and view/tab changes do not reset history; an external replacement establishes a new baseline.
- `util/undoer.rs`: preserve redo through cursor-only navigation without rewriting original undo cursor states; coalesce same-content boundaries rather than adding cursor-only undo points. New edits invalidate redo. The existing 100-point bound remains.

Application-level headless regressions exercise the actual file widget. See `docs/investigations/editor-undo.md` for the expected scenarios and comparison limits. No global text-field policy changes and no keyboard/input injection into the user's desktop are required.
