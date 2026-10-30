use crate::art;
use crate::motion;
use crate::settings::{Theme, color};
use eframe::egui::{self, Align2, Color32 as C, FontId, Pos2, Rect, Sense, Stroke, Vec2};

pub(in crate::app) fn chevron(ui: &egui::Ui, c: Pos2, down: bool, color: C) {
    let points = if down {
        vec![
            c + Vec2::new(-4., -2.),
            c + Vec2::new(0., 2.),
            c + Vec2::new(4., -2.),
        ]
    } else {
        vec![
            c + Vec2::new(-2., -4.),
            c + Vec2::new(2., 0.),
            c + Vec2::new(-2., 4.),
        ]
    };
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(1.5, color)));
}
pub(in crate::app) fn logo(ui: &mut egui::Ui, size: f32, surface: C) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_logo(ui, r, surface);
}
pub(in crate::app) fn paint_logo(ui: &egui::Ui, r: Rect, surface: C) {
    art::paint_app_logo(ui, r, surface);
}
pub(in crate::app) fn visual_button(
    ui: &mut egui::Ui,
    art: &art::Art,
    r: Rect,
    icon: usize,
    text: &str,
    enabled: bool,
    t: &Theme,
) -> egui::Response {
    let response = ui.interact(
        r,
        ui.scope_id()
            .with(("visual", text, r.min.x.to_bits(), r.min.y.to_bits())),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hover = motion::hover(
        ui,
        ("button-hover", text, r.min.x.to_bits(), r.min.y.to_bits()),
        enabled && response.hovered(),
    );
    let press = motion::value(
        ui,
        ("button-press", text, r.min.x.to_bits(), r.min.y.to_bits()),
        if enabled && response.is_pointer_button_down_on() {
            1.
        } else {
            0.
        },
        0.5,
    );
    let fill = mix(
        mix(color(&t.panel), color(&t.raised), 0.22),
        color(&t.raised),
        hover,
    );
    let fill = mix(fill, color(&t.background), press * 0.25);
    ui.painter().rect_filled(r, t.radius, fill);
    ui.painter().rect_stroke(
        r,
        t.radius,
        Stroke::new(
            1.,
            if enabled && response.has_focus() {
                color(&t.accent)
            } else if enabled {
                color(&t.border)
            } else {
                color(&t.border).gamma_multiply(0.65)
            },
        ),
        egui::StrokeKind::Inside,
    );
    let fg = if enabled {
        color(&t.text)
    } else {
        color(&t.muted).gamma_multiply(0.52)
    };
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(13.), fg);
    let has_icon = icon < art.len();
    if has_icon && text.is_empty() {
        art.paint(
            ui,
            icon,
            Rect::from_center_size(r.center(), Vec2::splat(18.)),
            fg,
        );
        return response;
    }
    let total = galley.size().x + if has_icon { 26. } else { 0. };
    let left = r.center().x - total / 2.;
    if has_icon {
        art.paint(
            ui,
            icon,
            Rect::from_center_size(Pos2::new(left + 8., r.center().y), Vec2::splat(17.)),
            fg,
        );
    }
    ui.painter().galley(
        Pos2::new(
            left + if has_icon { 26. } else { 0. },
            r.center().y - galley.size().y / 2.,
        ),
        galley,
        fg,
    );
    response
}
pub(in crate::app) fn theme_color_row(
    ui: &mut egui::Ui,
    name: &str,
    value: &mut String,
    t: &Theme,
) {
    ui.horizontal(|ui| {
        let (label, _) = ui.allocate_exact_size(Vec2::new(74., 34.), Sense::hover());
        ui.painter().text(
            label.left_center(),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(13.),
            color(&t.text),
        );
        let mut c = color(value);
        let old = c;
        let response = ui.add_sized(
            [32., 32.],
            egui::Button::new("")
                .fill(c)
                .stroke(Stroke::new(1., color(&t.border))),
        );
        egui::Popup::from_toggle_button_response(&response).show(|ui| {
            egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque);
        });
        if c != old {
            *value = format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
        }
        ui.add_sized(
            [100., 32.],
            egui::TextEdit::singleline(value).font(FontId::proportional(13.)),
        );
    });
    ui.add_space(3.);
}
pub(in crate::app) fn check_row(
    ui: &mut egui::Ui,
    value: &mut bool,
    text: &str,
    enabled: bool,
    t: &Theme,
) {
    let (r, b) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 24.),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let c = Rect::from_center_size(Pos2::new(r.left() + 9., r.center().y), Vec2::splat(17.));
    ui.painter().rect_filled(
        c,
        2,
        if *value {
            color(&t.accent).gamma_multiply(if enabled { 1. } else { 0.65 })
        } else {
            color(&t.panel)
        },
    );
    ui.painter().rect_stroke(
        c,
        2,
        Stroke::new(
            1.,
            if *value {
                color(&t.accent)
            } else {
                color(&t.border)
            },
        ),
        egui::StrokeKind::Inside,
    );
    if *value {
        ui.painter().add(egui::Shape::line(
            vec![
                c.center() + Vec2::new(-4., 0.),
                c.center() + Vec2::new(-1., 3.),
                c.center() + Vec2::new(5., -4.),
            ],
            Stroke::new(1.8, C::WHITE),
        ));
    }
    ui.painter().text(
        Pos2::new(r.left() + 28., r.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(14.),
        if enabled {
            color(&t.text)
        } else {
            color(&t.muted)
        },
    );
    if enabled && b.clicked() {
        ui.ctx().request_repaint();
        *value = !*value;
    }
}
pub(in crate::app) fn resize_edges(ui: &mut egui::Ui) {
    if ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let r = ui.max_rect();
    let edges = [
        (
            Rect::from_min_size(r.min, Vec2::new(7., r.height())),
            egui::ResizeDirection::West,
            egui::CursorIcon::ResizeHorizontal,
        ),
        (
            Rect::from_min_size(
                Pos2::new(r.right() - 7., r.top()),
                Vec2::new(7., r.height()),
            ),
            egui::ResizeDirection::East,
            egui::CursorIcon::ResizeHorizontal,
        ),
        (
            Rect::from_min_size(r.min, Vec2::new(r.width(), 5.)),
            egui::ResizeDirection::North,
            egui::CursorIcon::ResizeVertical,
        ),
        (
            Rect::from_min_size(
                Pos2::new(r.left(), r.bottom() - 5.),
                Vec2::new(r.width(), 5.),
            ),
            egui::ResizeDirection::South,
            egui::CursorIcon::ResizeVertical,
        ),
    ];
    for (i, (rect, dir, cursor)) in edges.into_iter().enumerate() {
        let response = ui
            .interact(rect, ui.scope_id().with(("resize", i)), Sense::drag())
            .on_hover_cursor(cursor);
        if response.drag_started() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }
}
pub(in crate::app) fn mix(a: C, b: C, t: f32) -> C {
    let lerp = |x: u8, y: u8| ((x as f32) * (1. - t) + (y as f32) * t).round() as u8;
    C::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}
