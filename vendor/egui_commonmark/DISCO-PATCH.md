# disco Markdown presentation patch

Upstream: egui_commonmark 0.26.0, https://github.com/lampsitter/egui_commonmark/commit/4374859276536d5660ba05b1f64f009e2e8199c1.
Source copied from the installed crates.io release; MIT OR Apache-2.0 licenses retained.

Presentation changes only: optional named heading/body font styles, code line height,
theme-controlled code surfaces, padded code blocks with drawn copy/check icons,
compact ruled tables, heading rules/spacing and alt-text fallbacks for missing images.
The HTML callback accepts a borrowed closure for native, lazy details/summary widgets. Parser, viewport cache and resource rules
remain upstream. Re-check these small changes when updating egui_commonmark.
