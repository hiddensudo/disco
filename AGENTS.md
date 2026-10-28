# Repository instructions

- Keep changes within the user's requested scope and preserve Ukrainian and English UI text together.
- The app has two viewing layouts, **Overview** and **Focus**. Use these names in UI, documentation and screenshots.
- Before changing a version number, adding a release tag or publishing a release, use the exact number explicitly agreed with the user. If no number has been agreed for that release, ask the user for it. Do not independently choose the size of a version bump or rewrite historical versions/tags.
- The user explicitly approved naming the completed current build 0.2 (Cargo version 0.2.0), including the scoped refresh/highlighting work and DSC-002, DSC-003, DSC-006 and DSC-007. Do not include unfinished backlog features in this release.
- Verify relevant Rust tests, Python installation/translation checks and `git diff --check` before committing. Do not commit local binaries, configuration, scan caches or personal test fixtures.

- The user explicitly approved 0.2.1 for the reusable keyboard preview tab and Enter/Right-arrow keep-open behavior. Preserve existing release tags.
