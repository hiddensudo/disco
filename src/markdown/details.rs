//! Turn only actual HTML details blocks into native, lazy disclosure widgets.
//! Code fences/inline code and unmatched HTML remain literal source.
use super::Reader;
use pulldown_cmark::{Event, Options, Parser};
use std::cell::{Cell, RefCell};

pub(super) struct Detail {
    pub title: String,
    pub source: String,
    pub expanded: Cell<bool>,
    pub reader: RefCell<Reader>,
}
pub(super) fn extract(text: &str, depth: u8) -> (String, Vec<Detail>) {
    if depth >= 8 {
        return (text.into(), vec![]);
    }
    let mut stack = Vec::new();
    let mut ranges = Vec::new();
    for (event, range) in Parser::new_ext(text, Options::all()).into_offset_iter() {
        let Event::Html(raw) = event else { continue };
        let lower = raw.to_ascii_lowercase();
        let mut cursor = 0;
        while cursor < lower.len() {
            let rest = &lower[cursor..];
            let opening = rest.find("<details").filter(|i| {
                rest.as_bytes()
                    .get(i + 8)
                    .is_some_and(|b| *b == b'>' || b.is_ascii_whitespace())
            });
            let closing = rest.find("</details>");
            let (offset, close) = match (opening, closing) {
                (Some(a), Some(b)) if a < b => (a, false),
                (_, Some(b)) => (b, true),
                (Some(a), None) => (a, false),
                _ => break,
            };
            let at = cursor + offset;
            if close {
                if let Some((start, body_start)) = stack.pop()
                    && stack.is_empty()
                {
                    ranges.push((start..range.start + at + 10, body_start..range.start + at));
                }
                cursor = at + 10;
            } else {
                let Some(end) = lower[at..].find('>').map(|i| at + i + 1) else {
                    break;
                };
                stack.push((range.start + at, range.start + end));
                cursor = end;
            }
        }
    }
    let mut rendered = String::new();
    let mut details = Vec::new();
    let mut cursor = 0;
    for (range, body) in ranges {
        rendered.push_str(&text[cursor..range.start]);
        let mut source = text[body.clone()].trim().to_string();
        let lower = source.to_ascii_lowercase();
        let mut title = crate::tr!("Докладніше", "Details").to_string();
        if lower.starts_with("<summary>")
            && let Some(end) = lower.find("</summary>")
        {
            title = Parser::new(&source[9..end])
                .filter_map(|event| match event {
                    Event::Text(s) | Event::Code(s) => Some(s.into_string()),
                    _ => None,
                })
                .collect();
            source = source[end + 10..].trim().to_string();
        }
        let opening = &text[range.start..body.start];
        let open = opening
            .to_ascii_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .any(|word| word == "open");
        rendered.push_str(&format!(
            "\n\n<div data-disco-details=\"{}\"></div>\n\n",
            details.len()
        ));
        details.push(Detail {
            title,
            source,
            expanded: Cell::new(open),
            reader: RefCell::new(Reader {
                depth: depth + 1,
                ..Default::default()
            }),
        });
        cursor = range.end;
    }
    rendered.push_str(&text[cursor..]);
    (rendered, details)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn details_are_lazy_nested_and_never_interpreted_inside_code() {
        let source = "```html\n<details><summary>literal</summary></details>\n```\n\n<details open>\n<summary>Update **disco**</summary>\n\n```sh\ngit pull\n```\n\n<details>\n<summary>Nested</summary>\n\nBody\n</details>\n\n</details>\n";
        let (display, details) = extract(source, 0);
        assert_eq!(details.len(), 1);
        assert!(display.contains("<summary>literal</summary>"));
        assert!(!display.contains("git pull"));
        assert_eq!(details[0].title, "Update disco");
        assert!(details[0].expanded.get());
        assert!(details[0].source.contains("git pull"));
        assert_eq!(extract(&details[0].source, 1).1.len(), 1);
        assert!(extract("<details>\nnever closed", 0).1.is_empty());
        assert!(extract(source, 8).1.is_empty());
    }
}

#[cfg(test)]
mod widget_tests {
    use super::*;
    use eframe::egui::{self, Pos2, Rect, Vec2};
    #[test]
    fn native_disclosure_opens_lazily_and_does_not_modify_source() {
        let source = "<details>\n<summary>Update disco</summary>\n\n```sh\ngit pull --ff-only\n```\n\n</details>\n\nAfter\n";
        let ctx = egui::Context::default();
        crate::art::fonts(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
        ctx.style_mut_of(egui::Theme::Light, |s| s.animation_time = 0.);
        let mut reader = Reader::default();
        let path = std::path::Path::new("/tmp/disclosure.md");
        let mut time = 0.;
        let mut tick = |events| {
            time += 0.1;
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(700., 500.))),
                    events,
                    time: Some(time),
                    ..Default::default()
                },
                |ui| {
                    ui.style_mut().animation_time = 0.;
                    reader.show(ui, path, source);
                },
            )
            .drop_without_applying_deltas();
        };
        for _ in 0..3 {
            tick(vec![]);
        }
        let id = egui::Id::unique(("markdown-details-toggle", path, 0usize));
        let rect = ctx
            .read_response(id)
            .expect("native disclosure, not raw HTML")
            .rect;
        let pos = rect.center();
        tick(vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            tick(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
        tick(vec![]);
        assert!(reader.details[0].expanded.get(), "header did not toggle");
        assert_eq!(reader.source.as_deref(), Some(source));
        assert!(reader.details[0].reader.borrow().rebuilds > 0);
        assert!(
            reader.details[0]
                .reader
                .borrow()
                .rendered
                .contains("git pull --ff-only")
        );
        assert_eq!(reader.rebuilds, 1);
    }
}
