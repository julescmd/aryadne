//! Look plat, angles droits, bordures fines. Mode clair par défaut, mode sombre en option.
//! Pour ajuster les couleurs, modifie LIGHT et DARK ci-dessous.
use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,      // zone d'édition
    pub panel: Color32,   // panneaux latéraux et barres
    pub line: Color32,    // bordures
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,  // couleur de la marque
    pub on_accent: Color32,
    pub link: Color32,
    pub missing: Color32, // lien vers une note inexistante
    pub code_bg: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const LIGHT: Palette = Palette {
    bg: rgb(0xffffff),
    panel: rgb(0xf3f3f1),
    line: rgb(0xdcdcd7),
    text: rgb(0x18181b),
    muted: rgb(0x7a7a80),
    accent: rgb(0xff4f1f),
    on_accent: rgb(0xffffff),
    link: rgb(0x1d4ed8),
    missing: rgb(0xb42318),
    code_bg: rgb(0xf3f3f1),
};

pub const DARK: Palette = Palette {
    bg: rgb(0x141416),
    panel: rgb(0x1b1b1e),
    line: rgb(0x2e2e33),
    text: rgb(0xe6e6e8),
    muted: rgb(0x8a8a92),
    accent: rgb(0xff6a3d),
    on_accent: rgb(0x141416),
    link: rgb(0x7aa2f7),
    missing: rgb(0xf07167),
    code_bg: rgb(0x1f1f23),
};

static IS_DARK: AtomicBool = AtomicBool::new(false);
/// Couleur d'accent choisie dans les réglages (0 = celle du thème).
static ACCENT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn set_accent(c: Option<Color32>) {
    let v = c.map(|c| 0x0100_0000 | (c.r() as u32) << 16 | (c.g() as u32) << 8 | c.b() as u32).unwrap_or(0);
    ACCENT.store(v, Ordering::Relaxed);
}

pub fn pal() -> Palette {
    let mut p = if IS_DARK.load(Ordering::Relaxed) { DARK } else { LIGHT };
    let a = ACCENT.load(Ordering::Relaxed);
    if a != 0 {
        p.accent = rgb(a & 0xff_ffff);
        let lum = 0.2126 * p.accent.r() as f32 + 0.7152 * p.accent.g() as f32 + 0.0722 * p.accent.b() as f32;
        p.on_accent = if lum > 150.0 { rgb(0x18181b) } else { rgb(0xffffff) };
    }
    p
}

pub fn is_dark() -> bool {
    IS_DARK.load(Ordering::Relaxed)
}

/// JetBrains Mono (OFL) intégrée à l'exécutable, utilisée partout. « bold » = JetBrains Mono Bold.
pub fn install_fonts(ctx: &egui::Context) {
    use std::sync::Arc;
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("jbm".into(), Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"))));
    fonts.font_data.insert("jbm-bold".into(), Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"))));
    for fam in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.get_mut(&fam).unwrap().insert(0, "jbm".into());
    }
    let mut bold = vec!["jbm-bold".to_string()];
    bold.extend(fonts.families[&FontFamily::Proportional].iter().skip(1).cloned());
    fonts.families.insert(FontFamily::Name("bold".into()), bold);
    ctx.set_fonts(fonts);
}

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn apply(ctx: &egui::Context, dark: bool) {
    IS_DARK.store(dark, Ordering::Relaxed);
    let p = pal();
    let prop = |size: f32| FontId::new(size, FontFamily::Proportional);
    ctx.global_style_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, prop(11.0)),
            (TextStyle::Body, prop(13.0)),
            (TextStyle::Button, prop(12.5)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
            (TextStyle::Heading, prop(18.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        style.spacing.interact_size.y = 26.0;

        let v = &mut style.visuals;
        *v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = p.panel;
        v.window_fill = p.bg;
        v.extreme_bg_color = p.bg;
        v.faint_bg_color = p.code_bg;
        v.override_text_color = Some(p.text);
        v.hyperlink_color = p.link;
        v.selection.bg_fill = p.accent.linear_multiply(0.28);
        v.selection.stroke = Stroke::new(1.0, p.accent);
        v.window_stroke = Stroke::new(1.0, p.line);
        v.window_shadow = egui::Shadow::NONE;
        v.popup_shadow = egui::Shadow::NONE;
        v.window_corner_radius = CornerRadius::ZERO;
        v.menu_corner_radius = CornerRadius::ZERO;
        v.text_cursor.stroke = Stroke::new(2.0, p.accent);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = CornerRadius::ZERO;
            w.expansion = 0.0;
            w.fg_stroke.color = p.text;
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
        v.widgets.noninteractive.fg_stroke.color = p.text;
        v.widgets.inactive.bg_fill = p.line;
        v.slider_trailing_fill = true;
        v.widgets.inactive.weak_bg_fill = p.bg;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.line);
        v.widgets.hovered.bg_fill = p.panel;
        v.widgets.hovered.weak_bg_fill = p.panel;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.text);
        v.widgets.active.bg_fill = p.line;
        v.widgets.active.weak_bg_fill = p.line;
        v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    });
}
