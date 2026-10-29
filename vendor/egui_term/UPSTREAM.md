Source: https://github.com/kemokempo/egui_term
Revision: 31bbc7ab8503c9518fcee5717cfa29011e59f451
License: MIT (LICENSE).

Local experimental changes: egui 0.37 compatibility; focus independent of pointer location; bracketed paste and selection copy; event-thread shutdown; cell alignment and hidden cursor handling. See the parent prototype README for validation and limits.

The host also answers terminal color/size queries. No complete upstream compatibility claim is made. Upstream's default key bindings and rendering architecture are retained unless noted; the prototype tests document exercised behavior.

Integration changes: shared root workspace dependency; stable widget IDs; PTY-only NO_COLOR removal through env; visibility-gated repaint notifications with coalesced wakeups. Session-tree teardown is owned by disco, not this widget.
