# Edit-action undo boundaries

DSC-027, 2026-10-31.

The previous editor used egui's general undo policy, which captures stable states after a pause and periodic states during continuous changes. That policy is useful for controls, but may merge separate rapid editing actions.

The reference is the explicit PasteOperation and EnterOperation in VS Code's [operation routing](https://github.com/microsoft/vscode/blob/main/src/vs/editor/common/cursor/cursorTypeOperations.ts) and [edit operations](https://github.com/microsoft/vscode/blob/main/src/vs/editor/common/cursor/cursorTypeEditOperations.ts). This is a primary-source comparison of operation boundaries, not a claim that every VS Code command, language service or auto-indent feature is reproduced. The installed VS Code GUI was not driven on the user's desktop.

Expected/control scenarios:

- `base → Paste A → Enter → Paste B`: three undos return `base+A+newline`, `base+A`, then `base`, even if all events arrive in one frame. Redo restores the same states/cursors.
- Two pastes in one frame remain separate; cut, selection replacement and Tab indentation are isolated.
- Continuous ordinary typing or deletion can group while operation type, cursor continuity and a sub-0.8-second pause agree. Moving the cursor or switching operation type starts a new group. Cursor-only actions do not themselves become undo entries.
- IME preedit states group until commit; undo returns to the text before composition, and redo restores the committed text. UTF-8/UTF-16 content remains whole through the normal buffer/IME implementation.
- Redo survives cursor movement; a new text edit discards the redo branch. Read-only mode cannot mutate the document or consume its undo/redo history.
- State stays keyed to the document widget across tab changes, Save and Markdown Read/Edit. External clean replacement starts a new baseline so an old undo stack cannot silently reapply obsolete disk content.

The local egui adaptation is opt-in only for the file widget. It retains the existing bounded 100-point undo history and the library's cursor, IME and editing machinery; no second competing editor history is maintained.
