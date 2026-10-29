# Local prototype measurements — 2026-10-29

These are observations from this machine, not cross-hardware guarantees. Prototype dev profile: root opt-level 1, dependencies opt-level 2, egui 0.37 / Alacritty 0.26, native Glow renderer. No web runtime.

| Probe | Observation |
| --- | --- |
| Headless real-PTY render, 800×450 terminal, after 5,000 output lines, 120 frames | UI construction p50 0.73 ms; p95 0.84 ms |
| Native two-panel capture with live htop | UI construction p95 about 0.9 ms; first visible PTY output about 1 ms for `/bin/sh` |
| Native idle window with two default shell sessions, after 3-second warmup, 5-second sample | Parent window CPU 0.0% at OS tick resolution |
| Native parent window memory | RSS about 240–243 MiB; PSS about 167 MiB; anonymous memory about 23.5 MiB |

Most RSS in this run belonged to file-backed graphics libraries: LLVM (~78 MiB), NVIDIA GPU compiler (~76 MiB), NVIDIA EGL (~21 MiB), and Mesa/Gallium (~20 MiB). These are included in RSS and must not be mistaken for terminal scrollback alone. Shell/child-process memory is additional. Graphics-driver usage varies by environment. No permanent loader overrides are applied.

UI times exclude GPU submission, monitor latency and actual end-to-end keystroke latency. A low UI construction time is not proof of a particular visible FPS. The idle observation does not cover a busy TUI. Startup of the user's configured shell can take longer than the minimal `/bin/sh` sample.

Validation: 2 real-PTY integration tests + 7 widget tests passed; 4 repository installation/translation checks passed. Real htop was displayed in the native window. Other TUI applications still need their own interactive compatibility checks.

## Color fix and empty Kitty comparison

The prototype now removes the launcher's `NO_COLOR` at process entry before threads start, so it is not accidentally inherited by PTY shells. It loads system monospace and supplementary-symbol fonts through Fontconfig once at startup; bundled egui fonts remain fallbacks. ANSI RGB output, Cyrillic, block drawing and Legacy Computing glyph coverage are tested.

Kitty 0.49.2 vs the complete colored prototype (not just a terminal widget): both requested a 1440×880 window, used `/bin/sh`, same working directory, no commands or agents running. Kitty used `--config NONE` and Noto Sans Mono. Two concurrent rounds with reversed startup order; each had 5 seconds warmup and three 1-second-spaced `/proc/*/smaps_rollup` samples. All descendants of each application were included. Existing unrelated applications were left running.

| Full process tree | Round 1 RSS sum / PSS / USS, MiB | Round 2 RSS sum / PSS / USS, MiB |
| --- | --- | --- |
| Colored disco prototype + shell | 246.7 / 112.7 / 42.3 | 244.5 / 110.4 / 40.0 |
| Empty Kitty + shell | 306.1 / 142.8 / 64.3 | 304.3 / 141.0 / 62.5 |

PSS apportions shared mappings, USS sums private clean/dirty resident pages. Summed RSS double-counts pages shared between descendants and is not the total unique physical footprint. PSS changes with other running applications and graphics-library sharing. In these paired samples the prototype used about 21% less PSS than empty Kitty; this is a local configuration result, not a universal claim. No Codex process is included in either empty baseline. Additional sessions/agents add their own memory.

Reproduce using `python3 tools/compare_memory.py --binary ./disco-terminal-prototype --cwd /path/to/project --output /tmp/terminal-vs-kitty.json` (absolute binary path recommended). The probe terminates only its newly created windows, never an existing user terminal.
