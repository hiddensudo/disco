use crate::app::ui::widgets::{mix, paint_logo};
use crate::art;
use crate::settings::{Layout, Theme, color};
use eframe::egui::{self, Align2, Color32 as C, FontId, Pos2, Rect, Sense, Stroke, Vec2};

pub(super) fn show(ui: &mut egui::Ui, art: &art::Art, theme: &Theme, layout: &Layout) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().min(300.), 208.),
        Sense::hover(),
    );
    let p = ui.painter().with_clip_rect(rect);
    let fg = color(&theme.text);
    let muted = color(&theme.muted);
    let border = Stroke::new(1., color(&theme.border));
    p.rect_filled(rect, theme.radius, color(&theme.background));
    let top = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 25.));
    p.rect_filled(top, theme.radius, color(&theme.raised));
    paint_logo(
        ui,
        Rect::from_min_size(rect.min + Vec2::new(8., 6.), Vec2::splat(13.)),
        color(&theme.raised),
    );
    p.text(
        rect.min + Vec2::new(28., 12.),
        Align2::LEFT_CENTER,
        "disco",
        art::bold(10.),
        fg,
    );
    for (i, icon) in [19, 20, 21].into_iter().enumerate() {
        art.paint(
            ui,
            icon,
            Rect::from_center_size(
                Pos2::new(rect.right() - 45. + i as f32 * 17., rect.top() + 12.),
                Vec2::splat(9.),
            ),
            muted,
        );
    }
    let side = if layout.categories { 32. } else { 0. };
    let prop = if layout.properties { 66. } else { 0. };
    let body = Rect::from_min_max(rect.min + Vec2::new(0., 25.), rect.max - Vec2::new(0., 12.));
    if layout.categories {
        let nav = Rect::from_min_size(body.min, Vec2::new(side, body.height()));
        p.rect_filled(nav, 0, color(&theme.panel));
        p.vline(nav.right(), nav.y_range(), border);
        for (i, icon) in [1, 2, 3, 4, 5].into_iter().enumerate() {
            let y = nav.top() + 21. + i as f32 * 29.;
            if i == 2 {
                p.rect_filled(
                    Rect::from_min_size(Pos2::new(nav.left(), y - 13.), Vec2::new(side, 26.)),
                    0,
                    color(&theme.raised),
                );
                p.rect_filled(
                    Rect::from_min_size(Pos2::new(nav.left(), y - 13.), Vec2::new(2., 26.)),
                    0,
                    color(&theme.accent),
                );
            }
            art.paint(
                ui,
                icon,
                Rect::from_center_size(
                    Pos2::new(nav.center().x, y),
                    Vec2::splat(16. * layout.icon_scale),
                ),
                fg,
            );
        }
    }
    let table = Rect::from_min_max(
        body.min + Vec2::new(side + 7., 7.),
        body.max - Vec2::new(prop + 7., 7.),
    );
    p.text(
        table.min + Vec2::new(2., 4.),
        Align2::LEFT_TOP,
        "‹   /   home   ›",
        FontId::proportional(8.),
        muted,
    );
    p.hline(table.x_range(), table.top() + 21., border);
    let tree_scale = if layout.preview_mode == crate::settings::PreviewMode::Focus {
        layout.sidebar_text_scale
    } else {
        1.
    };
    let row_h = (if layout.compact { 19. } else { 23. }) * tree_scale;
    for i in 0..5 {
        let row = Rect::from_min_size(
            table.min + Vec2::new(0., 27. + i as f32 * row_h),
            Vec2::new(table.width(), row_h),
        );
        if layout.striped_rows && i % 2 == 1 {
            p.rect_filled(
                row,
                0,
                mix(color(&theme.background), color(&theme.raised), 0.3),
            );
        }
        if i == 1 {
            p.rect_filled(
                row,
                theme.radius,
                mix(color(&theme.panel), color(&theme.accent), 0.31),
            );
            p.rect_filled(
                Rect::from_min_size(row.min, Vec2::new(2., row_h)),
                0,
                color(&theme.accent),
            );
        }
        let indent = if i == 0 { 0. } else { 7. };
        p.text(
            row.min + Vec2::new(4. + indent, row_h * 0.5),
            Align2::LEFT_CENTER,
            "›",
            FontId::proportional(10.),
            muted,
        );
        art.paint(
            ui,
            0,
            Rect::from_center_size(
                row.min + Vec2::new(20. + indent, row_h * 0.5),
                Vec2::splat(13. * layout.icon_scale * tree_scale),
            ),
            C::WHITE,
        );
        p.line_segment(
            [
                row.min + Vec2::new(32. + indent, row_h * 0.5),
                row.min + Vec2::new(65. + (i % 3) as f32 * 8., row_h * 0.5),
            ],
            Stroke::new(3. * tree_scale, muted),
        );
        p.line_segment(
            [
                Pos2::new(row.right() - 22., row.center().y),
                Pos2::new(row.right() - 5., row.center().y),
            ],
            Stroke::new(2., muted),
        );
        if layout.row_separators {
            p.hline(
                row.x_range(),
                row.bottom(),
                Stroke::new(0.5, color(&theme.border)),
            );
        }
    }
    if layout.properties {
        let detail = Rect::from_min_max(Pos2::new(body.right() - prop, body.top()), body.max);
        p.rect_filled(detail, 0, color(&theme.panel));
        p.vline(detail.left(), detail.y_range(), border);
        let tile = Rect::from_min_size(detail.min + Vec2::new(10., 14.), Vec2::splat(44.));
        p.rect_filled(tile, theme.radius, color(&theme.raised));
        art.paint(ui, 0, tile.shrink(7.), C::WHITE);
        for i in 0..5 {
            let y = detail.top() + 77. + i as f32 * 14.;
            p.line_segment(
                [
                    Pos2::new(detail.left() + 9., y),
                    Pos2::new(detail.right() - 9., y),
                ],
                Stroke::new(if i == 0 { 4. } else { 2. }, muted),
            );
        }
    }
    p.rect_filled(
        Rect::from_min_size(
            Pos2::new(rect.left(), rect.bottom() - 12.),
            Vec2::new(rect.width(), 12.),
        ),
        0,
        color(&theme.raised),
    );
    p.rect_stroke(rect, theme.radius, border, egui::StrokeKind::Inside);
}
