# Empty categories after switching between cached workspaces

DSC-033 investigation, 2026-10-31. Diagnostics and regression tests run without opening a desktop window. Private profile copies and their metadata/hashes remain outside the repository.

## Confirmed behavior

- A launcher configured with `DISCO_CONFIG_DIR` reads `<profile>/cache/last-scan.bin.gz`. The default profile instead uses `$XDG_CACHE_HOME/disco` (or `~/.cache/disco`). Renaming the launcher/application does not migrate either cache. The investigated copies contain a valid large snapshot in the default cache and a valid narrow snapshot in the separate development profile; there is no evidence that the old large snapshot was deleted.
- `last-scan` is one snapshot, not an archive or a union of all previous roots. A later successful scan of a child directory replaces the broader snapshot at that same destination. Scoped refresh persists only complete results, but the base may already be a narrow snapshot. Independent processes use atomic files to avoid corruption; they still have last-completed-writer semantics. The historical identity of a writer cannot be inferred from snapshot metadata alone.
- Configured collection roots previously required an exact folder entry in the snapshot. If the snapshot starts at `/workspace/practice` and a collection contains `/workspace`, the collection incorrectly hid even its known descendant. The fix displays the known snapshot subtree and retains `/workspace` in the missing-coverage list, allowing the UI to distinguish partial cached coverage from full coverage. Unrelated paths remain excluded.
- “Current scope refreshed” describes one completed scope refresh, not discovery of every category. An empty category outside the loaded root is compatible with that message; it is not evidence of cache corruption.

## Verification

`disco --snapshot-info` now reports cache path, root, completeness, counts and per-category configured/indexed/missing roots. It performs no scan and opens no window. Use a copy if legacy JSON migration might occur.

The opt-in `replay_copied_profile_normal_and_cli_category_restore_without_writes` test requires a `.diagnostic-copy` marker. It replays ordinary startup and direct-file startup, switches every category and verifies unchanged snapshot bytes, visible known descendants and preserved open documents. Auto-refresh is disabled in the in-memory test configuration. Separate owned-fixture regressions cover Refresh/Scan and saving a narrow report after a broad one.

## Remaining storage design work

Per-root snapshot storage with an explicit current-workspace pointer would prevent a narrow scan from discarding broader cached coverage. Migration/import between profiles should be explicit and preserve originals; silently borrowing a default user's cache for every isolated profile would violate profile separation. Concurrent writers additionally need a revision/ownership policy if older jobs must not replace newer workspace selections. These are storage changes beyond the category-filter fix and investigation; this change does not claim to restore missing coverage or migrate the live user's cache.
