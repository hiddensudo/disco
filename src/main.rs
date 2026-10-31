mod app;
mod art;
mod associations;
mod branding;
mod categories;
mod deletion;
mod demo;
mod directory_files;
mod file_creation;
mod file_preview;
mod file_search;
mod filesystem;
mod format;
mod frame_probe;
mod i18n;
mod index;
mod live_files;
mod markdown;
mod motion;
mod navigation;
mod path_input;
mod recycle;
mod refresh;
mod scan;
mod scoped_refresh;
mod settings;
mod shortcuts;
mod snapshot;
mod syntax;
mod terminal;
mod text_editor;
mod themes;

use eframe::egui;
use scan::{Control, background_priority, human, scan};
use std::{path::Path, sync::atomic::Ordering, time::Instant};

fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("disco {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--syntax-licenses") {
        println!("# Syntax grammar licenses\n");
        for license in two_face::acknowledgement::listing().for_syntaxes() {
            let mut text = String::new();
            license.write_md(&mut text);
            print!("{text}");
        }
        return Ok(());
    }
    if args.iter().any(|a| a == "--snapshot-info") {
        i18n::set_current(settings::load().1.language);
        let started = Instant::now();
        match snapshot::load() {
            Ok(Some(saved)) => {
                let loaded = started.elapsed();
                let index = index::Index::new(&saved.report);
                println!(
                    "folders={} saved_at={} disk_baseline={} load_ms={:.1} ready_ms={:.1} indexed={}",
                    saved.report.folders.len(),
                    saved.saved_at,
                    saved.disk.is_some(),
                    loaded.as_secs_f64() * 1000.,
                    started.elapsed().as_secs_f64() * 1000.,
                    index.folder_count()
                );
            }
            Ok(None) => println!("No saved snapshot"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    if let Some(i) = args.iter().position(|x| x == "--scan") {
        i18n::set_current(settings::load().1.language);
        if let Some(p) = args.get(i + 1) {
            let c = Control::default();
            c.gentle.store(true, Ordering::Relaxed);
            background_priority();
            match scan(Path::new(p), &c, |_| {}) {
                Ok(r) => {
                    println!(
                        "{} entries | {} | {:.3}s | skipped {} | hardlinks {}",
                        r.entries,
                        human(r.folders[0].bytes),
                        r.elapsed.as_secs_f64(),
                        r.skipped,
                        r.hardlinks
                    );
                    if args.iter().any(|a| a == "--save-scan") {
                        if let Err(e) = snapshot::save(&r) {
                            eprintln!("{e}");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        } else {
            eprintln!("Usage: disco --scan /absolute/path");
            std::process::exit(2);
        }
        return Ok(());
    }
    let startup = match path_input::startup(&args) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1360., 660.])
            .with_min_inner_size([1140., 620.])
            .with_decorations(false)
            .with_transparent(false)
            .with_app_id(branding::desktop_app_id(&settings::config_dir()))
            .with_title(format!("disco {}", env!("CARGO_PKG_VERSION")))
            .with_icon(art::app_icon(settings::color(
                &settings::load().0.background,
            ))),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    if args.iter().any(|a| a == "--demo") && args.iter().any(|a| a == "--frame-probe") {
        let x = args
            .iter()
            .position(|a| a == "--frame-probe-x")
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(80.);
        options.viewport = options
            .viewport
            .with_position([x, 80.])
            .with_window_level(egui::WindowLevel::AlwaysOnTop);
    }
    options.glow_options.vsync = true;
    eframe::run_native(
        "disco",
        options,
        Box::new(move |cc| Ok(Box::new(app::Disco::new_with_path(cc, startup)))),
    )
}
