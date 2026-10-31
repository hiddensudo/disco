//! Syntect runs on one reusable background worker. Egui only lays out cached
//! spans; no parser work or syntax loading runs in a paint frame.
use eframe::egui::{self, Color32, FontId};
use std::{
    ops::Range,
    path::Path,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender},
    },
};
use syntect::{
    easy::HighlightLines,
    highlighting::{Color, StyleModifier, Theme, ThemeItem},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};
const BASE: Color = Color {
    r: 211,
    g: 219,
    b: 224,
    a: 255,
};
fn theme() -> Theme {
    let mut theme = Theme::default();
    theme.settings.foreground = Some(BASE);
    for (scope, (r, g, b)) in [
        ("comment", (122, 139, 149)),
        ("string", (158, 199, 158)),
        ("constant.numeric, constant.language", (222, 183, 124)),
        ("keyword, storage", (234, 141, 129)),
        ("entity.name.function, support.function", (221, 202, 145)),
        (
            "entity.name.type, support.type, entity.name.tag",
            (138, 188, 212),
        ),
        (
            "variable.parameter, entity.other.attribute-name",
            (174, 205, 216),
        ),
    ] {
        theme.scopes.push(ThemeItem {
            scope: scope.parse().expect("built-in scope"),
            style: StyleModifier {
                foreground: Some(Color { r, g, b, a: 255 }),
                ..Default::default()
            },
        });
    }
    theme
}
struct Parsed {
    generation: u64,
    source: Arc<str>,
    spans: Vec<(Range<usize>, Color)>,
    language: String,
    extension: String,
}
type Request = (u64, Arc<str>, String);
#[derive(Default)]
pub(crate) struct Syntax {
    source: Arc<str>,
    extension: String,
    generation: u64,
    revision: u64,
    request: Arc<Mutex<Option<Request>>>,
    tx: Option<SyncSender<()>>,
    rx: Option<Receiver<Parsed>>,
    parsed: Option<Parsed>,
    layout_job: Option<(FontId, u32, u32, u64, Color32, egui::text::LayoutJob)>,
}
fn parse(
    syntaxes: &SyntaxSet,
    theme: &Theme,
    source: Arc<str>,
    extension: &str,
    generation: u64,
) -> Parsed {
    let syntax = syntaxes
        .find_syntax_by_extension(extension)
        .or_else(|| {
            source
                .lines()
                .next()
                .and_then(|s| syntaxes.find_syntax_by_first_line(s))
        })
        .unwrap_or_else(|| syntaxes.find_syntax_plain_text());
    let mut parsed = Parsed {
        generation,
        source: source.clone(),
        spans: vec![],
        language: syntax.name.clone(),
        extension: extension.to_owned(),
    };
    if source.lines().any(|line| line.len() > 8192) {
        return parsed;
    }
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut offset = 0;
    for line in LinesWithEndings::from(&source) {
        match highlighter.highlight_line(line, syntaxes) {
            Ok(parts) => {
                for (style, text) in parts {
                    parsed
                        .spans
                        .push((offset..offset + text.len(), style.foreground));
                    offset += text.len();
                }
            }
            Err(_) => {
                parsed.spans.clear();
                break;
            }
        }
    }
    parsed
}
impl Syntax {
    fn request(&mut self, source: &str, extension: &str, ctx: &egui::Context) {
        if source == self.source.as_ref() && extension == self.extension && self.tx.is_some() {
            return;
        }
        self.source = Arc::from(source);
        self.extension = extension.into();
        self.generation = self.generation.wrapping_add(1);
        self.revision = self.revision.wrapping_add(1);
        self.layout_job = None;
        if self.tx.is_none() {
            let (tx, wake) = mpsc::sync_channel(1);
            let (results, rx) = mpsc::channel();
            self.tx = Some(tx);
            self.rx = Some(rx);
            let request = self.request.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                crate::scan::background_priority();
                let syntaxes = two_face::syntax::extra_newlines();
                let theme = theme();
                while wake.recv().is_ok() {
                    // Coalesce keystrokes arriving while the previous parse ran.
                    let Some((generation, source, extension)) = request.lock().unwrap().take()
                    else {
                        continue;
                    };
                    let parsed = parse(&syntaxes, &theme, source, &extension, generation);
                    if results.send(parsed).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            });
        }
        *self.request.lock().unwrap() =
            Some((self.generation, self.source.clone(), self.extension.clone()));
        let _ = self.tx.as_ref().unwrap().try_send(());
    }
    pub fn layout(
        &mut self,
        ui: &egui::Ui,
        text: &str,
        font: FontId,
        path: Option<&Path>,
        color: Color32,
    ) -> Arc<egui::Galley> {
        let extension = path
            .and_then(|p| p.extension().or_else(|| p.file_name()))
            .and_then(|p| p.to_str())
            .unwrap_or("txt");
        self.request(text, extension, ui.ctx());
        if let Some(rx) = &self.rx {
            while let Ok(parsed) = rx.try_recv() {
                if parsed.generation == self.generation {
                    self.parsed = Some(parsed);
                    self.revision = self.revision.wrapping_add(1);
                    self.layout_job = None;
                }
            }
        }
        let dpi = ui.ctx().pixels_per_point().to_bits();
        if let Some((old, width, old_dpi, revision, old_color, job)) = &self.layout_job {
            if old == &font
                && *width == 0
                && *old_dpi == dpi
                && *revision == self.revision
                && *old_color == color
            {
                return ui.fonts_mut(|fonts| fonts.layout_job(job.clone()));
            }
        }
        let mut job = egui::text::LayoutJob::default();
        if let Some(parsed) = self.parsed.as_ref().filter(|p| {
            p.source.as_ref() == text && p.generation == self.generation && !p.spans.is_empty()
        }) {
            for (range, foreground) in &parsed.spans {
                let foreground = if *foreground == BASE {
                    color
                } else {
                    Color32::from_rgba_unmultiplied(
                        foreground.r,
                        foreground.g,
                        foreground.b,
                        foreground.a,
                    )
                };
                job.append(
                    &text[range.clone()],
                    0.,
                    egui::TextFormat {
                        font_id: font.clone(),
                        color: foreground,
                        ..Default::default()
                    },
                );
            }
        } else {
            job.append(
                text,
                0.,
                egui::TextFormat {
                    font_id: font.clone(),
                    color,
                    ..Default::default()
                },
            );
        }
        job.wrap.max_width = f32::INFINITY;
        // Cache syntax formatting, never atlas-dependent glyph UVs. Opening
        // the terminal installs fonts and replaces the atlas; egui's own
        // layout cache invalidates correctly for that and atlas rollover.
        self.layout_job = Some((font, 0, dpi, self.revision, color, job.clone()));
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    }
    pub fn language(&self) -> Option<&str> {
        self.parsed
            .as_ref()
            .filter(|p| p.extension == self.extension)
            .map(|p| p.language.as_str())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unchanged_frames_reuse_the_same_colored_galley_without_reparsing() {
        let ctx = egui::Context::default();
        let mut syntax = Syntax::default();
        let text = "fn render() { let value = 42; /* comment */ }\n".repeat(200);
        let path = Path::new("sample.rs");
        let font = FontId::monospace(13.);
        let start = std::time::Instant::now();
        while syntax.language().is_none() && start.elapsed() < std::time::Duration::from_secs(3) {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                syntax.layout(ui, &text, font.clone(), Some(path), Color32::WHITE);
            })
            .drop_without_applying_deltas();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(syntax.language(), Some("Rust"));
        let generation = syntax.generation;
        let start = std::time::Instant::now();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let first = syntax.layout(ui, &text, font.clone(), Some(path), Color32::WHITE);
            for _ in 0..1000 {
                assert!(Arc::ptr_eq(
                    &first,
                    &syntax.layout(ui, &text, font.clone(), Some(path), Color32::WHITE)
                ));
            }
        })
        .drop_without_applying_deltas();
        assert_eq!(syntax.generation, generation);
        eprintln!(
            "1000 cached colored layouts: {:.3} ms",
            start.elapsed().as_secs_f64() * 1000.
        );
    }
    #[test]
    fn terminal_font_install_rebuilds_cached_text_for_the_current_atlas() {
        let ctx = egui::Context::default();
        crate::art::fonts(&ctx);
        let mut syntax = Syntax::default();
        let text = "int main() { return 42; }\n";
        let font = FontId::monospace(13.);
        let mut before = None;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            before = Some(syntax.layout(ui, text, font.clone(), None, Color32::WHITE));
        })
        .drop_without_applying_deltas();
        // Freeze parser state: the next frame must change only because the
        // terminal replaces the shared font atlas, not because parsing finished.
        syntax.rx = None;
        crate::art::terminal_fonts(&ctx);
        let mut after = None;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            after = Some(syntax.layout(ui, text, font.clone(), None, Color32::WHITE));
            let fresh = ui
                .fonts_mut(|fonts| fonts.layout_job(before.as_ref().unwrap().job.as_ref().clone()));
            assert!(
                Arc::ptr_eq(after.as_ref().unwrap(), &fresh),
                "editor must use a layout belonging to the current font atlas"
            );
        })
        .drop_without_applying_deltas();
        assert!(!Arc::ptr_eq(
            before.as_ref().unwrap(),
            after.as_ref().unwrap()
        ));
    }
    #[test]
    fn syntect_handles_multiline_comments_unicode_and_supported_languages() {
        let ss = two_face::syntax::extra_newlines();
        let theme = theme();
        for (extension, code) in [
            (
                "rs",
                "/* comment\ncontinued */\nfn main() { let café = 42; }\n",
            ),
            ("json", "{\"name\": \"value\", \"count\": 2}"),
            ("py", "def greet():\n    return 'hello'\n"),
            ("toml", "name = \"disco\"\n"),
        ] {
            let result = parse(&ss, &theme, Arc::from(code), extension, 1);
            assert_eq!(
                result
                    .spans
                    .iter()
                    .map(|(r, _)| &code[r.clone()])
                    .collect::<String>(),
                code
            );
            assert!(result.spans.iter().any(|(_, c)| *c != BASE), "{extension}");
        }
    }
}
