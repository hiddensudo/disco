use super::*;
use egui::{Event, Key, Modifiers, Pos2, RawInput, Rect};
use std::thread;

fn render(
    ctx: &egui::Context,
    backend: &mut TerminalBackend,
    size: Vec2,
    events: Vec<Event>,
) -> f64 {
    let start = Instant::now();
    ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 700.))),
            events,
            ..Default::default()
        },
        |ui| {
            let view = TerminalView::new(ui, backend)
                .set_focus(true)
                .set_size(size);
            ui.add(view);
        },
    )
    .drop_without_applying_deltas();
    start.elapsed().as_secs_f64() * 1000.
}
fn contents(backend: &mut TerminalBackend) -> String {
    backend
        .sync()
        .grid
        .display_iter()
        .map(|cell| cell.c)
        .collect()
}
fn wait_for(backend: &mut TerminalBackend, token: &str) {
    let start = Instant::now();
    while !contents(backend).contains(token) && start.elapsed() < Duration::from_secs(4) {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        contents(backend).contains(token),
        "missing {token}: {}",
        contents(backend)
    );
}
fn shell(ctx: &egui::Context, id: u64) -> TerminalBackend {
    let (tx, _rx) = mpsc::channel();
    TerminalBackend::new(
        id,
        ctx.clone(),
        tx,
        BackendSettings {
            shell: "/bin/sh".into(),
            working_directory: Some(std::env::temp_dir()),
            ..Default::default()
        },
    )
    .unwrap()
}
fn write(backend: &mut TerminalBackend, text: &str) {
    backend.process_command(BackendCommand::Write(text.as_bytes().to_vec()));
}
#[test]
fn real_pty_keyboard_works_outside_pointer_and_paste_preserves_utf8() {
    let ctx = egui::Context::default();
    let mut backend = shell(&ctx, 0);
    let size = Vec2::new(820., 480.);
    render(&ctx, &mut backend, size, vec![]);
    // Pointer is outside the terminal; keyboard focus still owns input.
    let command = "test -t 0 && test \"$TERM\" = xterm-256color && printf '\\120\\124\\131\\137\\117\\113 Привіт\\n'";
    render(
        &ctx,
        &mut backend,
        size,
        vec![
            Event::PointerMoved(Pos2::new(990., 690.)),
            Event::Paste(command.into()),
        ],
    );
    render(
        &ctx,
        &mut backend,
        size,
        vec![Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    wait_for(&mut backend, "PTY_OK Привіт");
    let output = ctx.run_ui(
        RawInput {
            events: vec![Event::Key {
                key: Key::V,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::CTRL | Modifiers::SHIFT,
            }],
            ..Default::default()
        },
        |ui| {
            let view = TerminalView::new(ui, &mut backend)
                .set_focus(true)
                .set_size(size);
            ui.add(view);
        },
    );
    assert!(output.viewport_output.values().any(|v| {
        v.commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::RequestPaste))
    }));
    output.drop_without_applying_deltas();
    write(&mut backend, "exit\r");
}
#[test]
fn real_pty_sessions_resize_interrupt_and_release_children() {
    let ctx = egui::Context::default();
    let mut a = shell(&ctx, 1);
    let mut b = shell(&ctx, 2);
    let pids = [a.pty_id(), b.pty_id()];
    assert_ne!(pids[0], pids[1]);
    render(&ctx, &mut a, Vec2::new(810., 400.), vec![]);
    render(&ctx, &mut b, Vec2::new(480., 240.), vec![]);
    write(
        &mut a,
        "printf '\\033[?1049h\\033[2J\\033[H'; printf '\\123\\105\\123\\123\\111\\117\\116\\137\\101'; stty size\r",
    );
    wait_for(&mut a, "SESSION_A");
    assert!(
        a.last_content()
            .terminal_mode
            .contains(egui_term::TerminalMode::ALT_SCREEN)
    );
    assert!(!contents(&mut b).contains("SESSION_A"));
    let large = a.last_content().terminal_size;
    render(&ctx, &mut a, Vec2::new(400., 190.), vec![]);
    let small = a.last_content().terminal_size;
    assert!(small.cell_width > 0 && small.cell_height > 0);
    let big_size: alacritty_terminal::event::WindowSize = large.into();
    let small_size: alacritty_terminal::event::WindowSize = small.into();
    assert!(small_size.num_cols < big_size.num_cols && small_size.num_lines < big_size.num_lines);
    write(&mut a, "printf '\\033[?1049l'; sleep 30\r");
    thread::sleep(Duration::from_millis(80));
    render(&ctx, &mut a, Vec2::new(400., 190.), vec![Event::Copy]);
    write(
        &mut a,
        "printf '\\101\\106\\124\\105\\122\\137\\111\\116\\124\\105\\122\\122\\125\\120\\124\\n'\r",
    );
    wait_for(&mut a, "AFTER_INTERRUPT");
    // Show that retained scrollback does not make every frame progressively expensive.
    render(&ctx, &mut b, Vec2::new(800., 450.), vec![]);
    write(
        &mut b,
        "i=0; while [ $i -lt 5000 ]; do printf 'scrollback row %s\\n' $i; i=$((i+1)); done; printf '\\102\\125\\122\\123\\124\\137\\104\\117\\116\\105\\n'\r",
    );
    wait_for(&mut b, "BURST_DONE");
    let mut samples: Vec<_> = (0..120)
        .map(|_| render(&ctx, &mut b, Vec2::new(800., 450.), vec![]))
        .collect();
    samples.sort_by(f64::total_cmp);
    eprintln!(
        "terminal visible 800x450, 5000 history lines: UI p50={:.3}ms p95={:.3}ms",
        samples[60], samples[114]
    );
    drop(a);
    drop(b);
    let start = Instant::now();
    while pids
        .iter()
        .any(|pid| unsafe { libc::kill(*pid as i32, 0) == 0 })
        && start.elapsed() < Duration::from_secs(4)
    {
        thread::sleep(Duration::from_millis(10));
    }
    for pid in pids {
        assert_ne!(
            unsafe { libc::kill(pid as i32, 0) },
            0,
            "shell survived backend drop"
        );
    }
}

#[test]
fn system_font_fallback_covers_terminal_blocks_and_truecolor_survives_pty() {
    let ctx = egui::Context::default();
    terminal_fonts(&ctx);
    ctx.run_ui(RawInput::default(), |ui| {
        assert!(ui.fonts_mut(|f| f.has_glyphs(&FontId::monospace(14.), "Привіт █▄▀▌▐░▒▓🬀")));
    })
    .drop_without_applying_deltas();
    let mut backend = shell(&ctx, 3);
    render(&ctx, &mut backend, Vec2::new(800., 450.), vec![]);
    write(
        &mut backend,
        "printf '\\033[2J\\033[H\\033[38;2;255;45;60mCOLOR_OK\\033[0m\\n'\r",
    );
    wait_for(&mut backend, "COLOR_OK");
    let color = alacritty_terminal::vte::ansi::Color::Spec(alacritty_terminal::vte::ansi::Rgb {
        r: 255,
        g: 45,
        b: 60,
    });
    assert!(
        backend
            .sync()
            .grid
            .display_iter()
            .any(|cell| cell.c == 'C' && cell.fg == color)
    );
}
