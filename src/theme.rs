//! Style « éditeur de code » : monospace partout, sombre, angles droits.
//! Pour changer le look, modifie les couleurs ci-dessous.
use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

pub const BG: Color32 = Color32::from_rgb(0x0d, 0x0e, 0x10);
pub const PANEL: Color32 = Color32::from_rgb(0x12, 0x13, 0x16);
pub const LINE: Color32 = Color32::from_rgb(0x25, 0x27, 0x2c);
pub const TEXT: Color32 = Color32::from_rgb(0xc8, 0xcb, 0xd0);
pub const MUTED: Color32 = Color32::from_rgb(0x6a, 0x6f, 0x79);
pub const ACCENT: Color32 = Color32::from_rgb(0x9e, 0xce, 0x6a);
pub const LINK: Color32 = Color32::from_rgb(0x7a, 0xa2, 0xf7);
pub const MISSING: Color32 = Color32::from_rgb(0xc0, 0x6c, 0x6c);
pub const CODE_BG: Color32 = Color32::from_rgb(0x17, 0x19, 0x1d);

pub fn apply(ctx: &egui::Context) {
    let mono = |size: f32| FontId::new(size, FontFamily::Monospace);
    ctx.global_style_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, mono(11.0)),
            (TextStyle::Body, mono(13.5)),
            (TextStyle::Button, mono(13.5)),
            (TextStyle::Monospace, mono(13.5)),
            (TextStyle::Heading, mono(18.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 4.0);
        style.spacing.button_padding = egui::vec2(6.0, 2.0);

        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = PANEL;
        v.window_fill = PANEL;
        v.extreme_bg_color = BG;
        v.faint_bg_color = CODE_BG;
        v.override_text_color = Some(TEXT);
        v.hyperlink_color = LINK;
        v.selection.bg_fill = ACCENT.linear_multiply(0.25);
        v.selection.stroke = Stroke::new(1.0, ACCENT);
        v.window_stroke = Stroke::new(1.0, LINE);
        v.window_corner_radius = CornerRadius::ZERO;
        v.menu_corner_radius = CornerRadius::ZERO;
        v.text_cursor.stroke = Stroke::new(2.0, ACCENT);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = CornerRadius::ZERO;
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        v.widgets.inactive.bg_fill = BG;
        v.widgets.hovered.weak_bg_fill = LINE;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, MUTED);
        v.widgets.active.weak_bg_fill = LINE;
        v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    });
}
