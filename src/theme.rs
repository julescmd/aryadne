//! Look plat, angles droits, bordures fines. Mode clair par défaut, mode sombre en option.
//! Les couleurs viennent de la charte graphique : THEMES ci-dessous.
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
    /// Champs et boutons (un cran sous les panneaux, comme dans Blender et Premiere)
    pub field: Color32,
    /// Menus déroulants, infobulles, fenêtres flottantes
    pub raised: Color32,
    /// Fonds discrets : pastilles actives, barres de progression
    pub secondary: Color32,
    /// Mise en avant ponctuelle : nouveauté, élément épinglé, repère
    pub highlight: Color32,
    /// Fond de la sélection
    pub selection: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub info: Color32,
}

/// Couleur des interstices entre les panneaux (comme Blender : des zones arrondies sur fond sombre).
pub fn gutter() -> Color32 {
    let p = pal();
    let k = |c: Color32, f: f32| Color32::from_rgb((c.r() as f32 * f) as u8, (c.g() as f32 * f) as u8, (c.b() as f32 * f) as u8);
    if p.bg.r() < 128 { k(p.panel, 0.42) } else { k(p.line, 0.82) }
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Les thèmes de la charte graphique (voir la charte « Aryadne »). Chacun part de cinq couleurs
/// (texte, fond, primaire, secondaire, accent) ; les couleurs d'état et d'espace gardent leur teinte
/// d'un thème à l'autre, seule leur clarté s'ajuste pour rester lisible (4,5:1 sur les panneaux).
pub struct ThemeDef {
    pub id: &'static str,
    pub name: &'static str,
    pub dark: bool,
    pub p: Palette,
    /// notes, 3D, 2D, vidéo, code, IA, matériaux
    pub ws: [Color32; 7],
}

pub const THEMES: [ThemeDef; 6] = [
    ThemeDef { id: "rose", name: "Rose poudré", dark: true, p: Palette { bg: rgb(0x1a1417), panel: rgb(0x231d20), line: rgb(0x463130), text: rgb(0xf4eef1), muted: rgb(0x948e91), accent: rgb(0xccacb9), on_accent: rgb(0x100a0d), link: rgb(0xbea197), missing: rgb(0xd88983), code_bg: rgb(0x231d20), field: rgb(0x1b1518), raised: rgb(0x2e282b), secondary: rgb(0x63413d), highlight: rgb(0xa67f6f), selection: rgb(0x52454b), success: rgb(0x88bf9a), warning: rgb(0xd1af75), info: rgb(0x94b0d1) }, ws: [rgb(0xc4b68d), rgb(0x8ba8c6), rgb(0xcb9ab2), rgb(0xaf9cc9), rgb(0x83b99e), rgb(0x7fb8bd), rgb(0xc69580)] },
    ThemeDef { id: "sauge", name: "Sauge", dark: true, p: Palette { bg: rgb(0x161412), panel: rgb(0x201d1b), line: rgb(0x3e4430), text: rgb(0xf3efed), muted: rgb(0x928f8d), accent: rgb(0xc6b3a6), on_accent: rgb(0x0c0a08), link: rgb(0xa2b995), missing: rgb(0xd88983), code_bg: rgb(0x201d1b), field: rgb(0x181614), raised: rgb(0x2a2826), secondary: rgb(0x576441), highlight: rgb(0x779c66), selection: rgb(0x4e4742), success: rgb(0x88bf9a), warning: rgb(0xd1af75), info: rgb(0x94b0d1) }, ws: [rgb(0xc4b68d), rgb(0x8ba8c6), rgb(0xcb9ab2), rgb(0xaf9cc9), rgb(0x83b99e), rgb(0x7fb8bd), rgb(0xc69580)] },
    ThemeDef { id: "lagon", name: "Lagon", dark: true, p: Palette { bg: rgb(0x0f1515), panel: rgb(0x181f1f), line: rgb(0x4b2d41), text: rgb(0xe6f3f2), muted: rgb(0x889291), accent: rgb(0x9acdcb), on_accent: rgb(0x050b0b), link: rgb(0xc09f8d), missing: rgb(0xd88983), code_bg: rgb(0x181f1f), field: rgb(0x111818), raised: rgb(0x222929), secondary: rgb(0x75395c), highlight: rgb(0xb48067), selection: rgb(0x3c504f), success: rgb(0x88bf9a), warning: rgb(0xd1af75), info: rgb(0x94b0d1) }, ws: [rgb(0xc4b68d), rgb(0x8ba8c6), rgb(0xcb9ab2), rgb(0xaf9cc9), rgb(0x83b99e), rgb(0x7fb8bd), rgb(0xc69580)] },
    ThemeDef { id: "argile", name: "Argile", dark: true, p: Palette { bg: rgb(0x181515), panel: rgb(0x211e1e), line: rgb(0x453532), text: rgb(0xefecec), muted: rgb(0x908e8e), accent: rgb(0xc3b0ad), on_accent: rgb(0x0e0b0b), link: rgb(0xc3a7a3), missing: rgb(0xd88983), code_bg: rgb(0x211e1e), field: rgb(0x191616), raised: rgb(0x2b2828), secondary: rgb(0x634743), highlight: rgb(0xab827b), selection: rgb(0x4e4746), success: rgb(0x88bf9a), warning: rgb(0xd1af75), info: rgb(0x94b0d1) }, ws: [rgb(0xc4b68d), rgb(0x8ba8c6), rgb(0xcb9ab2), rgb(0xaf9cc9), rgb(0x83b99e), rgb(0x7fb8bd), rgb(0xc69580)] },
    ThemeDef { id: "graphite", name: "Graphite", dark: true, p: Palette { bg: rgb(0x1d1e21), panel: rgb(0x26272b), line: rgb(0x393b41), text: rgb(0xe4e5e9), muted: rgb(0xa0a2ab), accent: rgb(0xff6a3d), on_accent: rgb(0x141416), link: rgb(0x7aa2f7), missing: rgb(0xd88983), code_bg: rgb(0x26272b), field: rgb(0x18191b), raised: rgb(0x2e2f34), secondary: rgb(0x3a3c44), highlight: rgb(0xff8a65), selection: rgb(0x633a30), success: rgb(0x88bf9a), warning: rgb(0xd1af75), info: rgb(0x94b0d1) }, ws: [rgb(0xc4b68d), rgb(0x8ba8c6), rgb(0xcb9ab2), rgb(0xaf9cc9), rgb(0x83b99e), rgb(0x7fb8bd), rgb(0xc69580)] },
    ThemeDef { id: "clair", name: "Clair", dark: false, p: Palette { bg: rgb(0xfaf8f7), panel: rgb(0xf1eded), line: rgb(0xd9d0cf), text: rgb(0x1c1517), muted: rgb(0x716568), accent: rgb(0x8e5e70), on_accent: rgb(0xffffff), link: rgb(0x7f5546), missing: rgb(0xa52f27), code_bg: rgb(0xf1eded), field: rgb(0xffffff), raised: rgb(0xffffff), secondary: rgb(0xe3d3d6), highlight: rgb(0x836254), selection: rgb(0xe0d0d6), success: rgb(0x317548), warning: rgb(0x856021), info: rgb(0x336399) }, ws: [rgb(0x7c6932), rgb(0x396693), rgb(0x913b66), rgb(0x5f3d8f), rgb(0x337353), rgb(0x307076), rgb(0x975235)] },
];

/// Thème choisi (indice dans THEMES)
static THEME: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Choisit le thème par son identifiant (« rose », « sauge », « lagon », « argile », « graphite », « clair »).
pub fn set_theme(id: &str) {
    let i = THEMES.iter().position(|t| t.id == id).unwrap_or(0);
    THEME.store(i, Ordering::Relaxed);
    IS_DARK.store(THEMES[i].dark, Ordering::Relaxed);
}

pub fn theme() -> &'static ThemeDef {
    &THEMES[THEME.load(Ordering::Relaxed).min(THEMES.len() - 1)]
}

/// Couleur d'un genre d'espace de travail dans le thème courant (notes, 3d, 2d, video, code, ai, mat)
pub fn ws_color(kind: &str) -> Option<Color32> {
    // Motion n'a pas de case dans les thèmes : la teinte « avertissement » (ambre) lui va
    if kind == "motion" {
        return Some(theme().p.warning);
    }
    let k = ["notes", "3d", "2d", "video", "code", "ai", "mat"].iter().position(|x| *x == kind)?;
    Some(theme().ws[k])
}

static IS_DARK: AtomicBool = AtomicBool::new(false);
/// Contraste élevé : textes secondaires, bordures et champs bien plus marqués (WCAG AA partout)
static HIGH_CONTRAST: AtomicBool = AtomicBool::new(false);
/// Animations réduites (préférence d'accessibilité)
static REDUCE_MOTION: AtomicBool = AtomicBool::new(false);

pub fn set_access(high_contrast: bool, reduce_motion: bool) {
    HIGH_CONTRAST.store(high_contrast, Ordering::Relaxed);
    REDUCE_MOTION.store(reduce_motion, Ordering::Relaxed);
}

pub fn high_contrast() -> bool {
    HIGH_CONTRAST.load(Ordering::Relaxed)
}

pub fn reduce_motion() -> bool {
    REDUCE_MOTION.load(Ordering::Relaxed)
}

/// Luminance relative (WCAG 2.1)
fn luminance(c: Color32) -> f32 {
    let f = |v: u8| {
        let x = v as f32 / 255.0;
        if x <= 0.039_28 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
}

/// Rapport de contraste WCAG entre deux couleurs (1 à 21)
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Texte sur une couleur pleine : noir ou blanc, celui qui contraste le plus
pub fn text_on(c: Color32) -> Color32 {
    let (dark, light) = (rgb(0x18181b), rgb(0xffffff));
    if contrast(dark, c) >= contrast(light, c) { dark } else { light }
}

/// Assombrit (thème clair) ou éclaircit (sombre) une couleur jusqu'au contraste voulu sur `bg`
pub fn readable(c: Color32, bg: Color32, min: f32) -> Color32 {
    let mut x = c;
    let dark_bg = luminance(bg) < 0.2;
    for _ in 0..40 {
        if contrast(x, bg) >= min {
            break;
        }
        x = if dark_bg { x.lerp_to_gamma(Color32::WHITE, 0.08) } else { x.lerp_to_gamma(Color32::BLACK, 0.08) };
    }
    x
}
/// Couleur d'accent choisie dans les réglages (0 = celle du thème).
static ACCENT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn set_accent(c: Option<Color32>) {
    let v = c.map(|c| 0x0100_0000 | (c.r() as u32) << 16 | (c.g() as u32) << 8 | c.b() as u32).unwrap_or(0);
    ACCENT.store(v, Ordering::Relaxed);
}

/// Couleur de l'espace de travail en cours : passe avant celle des réglages (0 = aucune).
static WS_ACCENT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn set_workspace_accent(c: Option<[u8; 3]>) {
    WS_ACCENT.store(c.map(|c| 0x0100_0000 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32).unwrap_or(0), Ordering::Relaxed);
}

pub fn pal() -> Palette {
    let mut p = theme().p;
    let w = WS_ACCENT.load(Ordering::Relaxed);
    let a = if w != 0 { w } else { ACCENT.load(Ordering::Relaxed) };
    if a != 0 {
        p.accent = rgb(a & 0xff_ffff);
    }
    // Le texte posé sur l'accent : noir ou blanc selon le contraste réel (le blanc sur l'orange
    // du thème clair n'atteignait que 3,3:1)
    p.on_accent = text_on(p.accent);
    if HIGH_CONTRAST.load(Ordering::Relaxed) {
        let dark = p.bg.r() < 128;
        p.muted = if dark { rgb(0xd0d2d8) } else { rgb(0x3f3f46) };
        p.line = if dark { rgb(0x8a8d96) } else { rgb(0x6b6b70) };
        p.text = if dark { rgb(0xffffff) } else { rgb(0x000000) };
        p.field = if dark { rgb(0x000000) } else { rgb(0xffffff) };
        p.bg = if dark { rgb(0x111113) } else { rgb(0xffffff) };
        p.link = readable(p.link, p.bg, 7.0);
        p.missing = readable(p.missing, p.bg, 7.0);
        p.accent = readable(p.accent, p.panel, 4.5);
        p.on_accent = text_on(p.accent);
    }
    p
}

pub fn is_dark() -> bool {
    IS_DARK.load(Ordering::Relaxed)
}

/// Polices intégrées : nom → (normal, gras, italique, gras italique).
const BUILTIN: [(&str, &str, &[u8], &[u8], &[u8], &[u8]); 3] = [
    ("inter", "Inter", include_bytes!("../assets/fonts/Inter-Regular.ttf"), include_bytes!("../assets/fonts/Inter-SemiBold.ttf"), include_bytes!("../assets/fonts/Inter-Italic.ttf"), include_bytes!("../assets/fonts/Inter-BoldItalic.ttf")),
    ("jetbrains", "JetBrains Mono", include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"), include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"), include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"), include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf")),
    ("lato", "Lato", include_bytes!("../assets/fonts/Lato-Regular.ttf"), include_bytes!("../assets/fonts/Lato-Bold.ttf"), include_bytes!("../assets/fonts/Lato-Regular.ttf"), include_bytes!("../assets/fonts/Lato-Bold.ttf")),
];

/// Les polices choisies : interface, chiffres et code, notes. Une police est un nom intégré ou le chemin d'un .ttf/.otf.
#[derive(Clone, PartialEq, Default)]
pub struct Fonts {
    pub ui: String,
    pub mono: String,
    pub notes: String,
}

static FONTS: std::sync::Mutex<Option<Fonts>> = std::sync::Mutex::new(None);
/// Polices demandées en plus (textes des boards, titres du montage…)
static EXTRA: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
/// Polices réellement chargées (une famille inconnue ferait planter egui)
static LOADED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Cette police est-elle chargée dans l'image en cours (utilisable avec `font_family`) ?
pub fn has_font(ui: &egui::Ui, spec: &str) -> bool {
    let fam = font_family(spec, false, false);
    ui.fonts(|f| f.definitions().families.contains_key(&fam))
}

/// Les quatre variantes d'une police (gras, italique) : intégrée, ou fichier et ses voisins (« -Bold », « -Italic »…).
fn variants(spec: &str) -> Option<[Vec<u8>; 4]> {
    if let Some(b) = BUILTIN.iter().find(|b| b.0 == spec) {
        return Some([b.2.to_vec(), b.3.to_vec(), b.4.to_vec(), b.5.to_vec()]);
    }
    let path = std::path::Path::new(spec);
    let regular = std::fs::read(path).ok()?;
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let ext = path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_else(|| "ttf".into());
    let base = stem.trim_end_matches("-Regular").trim_end_matches("Regular").trim_end_matches('-').to_string();
    let dir = path.parent()?;
    let find = |names: &[&str]| -> Option<Vec<u8>> { names.iter().find_map(|n| std::fs::read(dir.join(format!("{base}{n}.{ext}"))).ok()) };
    let bold = find(&["-Bold", "-SemiBold", "Bold", "-bold", "bd"]).unwrap_or_else(|| regular.clone());
    let italic = find(&["-Italic", "Italic", "-italic", "-Oblique", "i"]).unwrap_or_else(|| regular.clone());
    let bi = find(&["-BoldItalic", "BoldItalic", "-bolditalic", "bi"]).unwrap_or_else(|| bold.clone());
    Some([regular, bold, italic, bi])
}

/// Nom lisible d'une police choisie.
pub fn font_label(spec: &str) -> String {
    BUILTIN.iter().find(|b| b.0 == spec).map(|b| b.1.to_string()).unwrap_or_else(|| std::path::Path::new(spec).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| spec.to_string()))
}

/// Les polices intégrées (nom, libellé).
pub fn builtin_fonts() -> Vec<(String, String)> {
    BUILTIN.iter().map(|b| (b.0.to_string(), b.1.to_string())).collect()
}

/// Les polices installées sur l'ordinateur (libellé, chemin), lues une fois.
pub fn system_fonts() -> Vec<(String, String)> {
    static LIST: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| {
        let mut dirs: Vec<std::path::PathBuf> = vec!["/usr/share/fonts".into(), "/usr/local/share/fonts".into(), "/Library/Fonts".into(), "/System/Library/Fonts".into(), "C:\\Windows\\Fonts".into()];
        if let Some(h) = dirs::home_dir() {
            dirs.push(h.join(".local/share/fonts"));
            dirs.push(h.join(".fonts"));
            dirs.push(h.join("Library/Fonts"));
            dirs.push(h.join("AppData/Local/Microsoft/Windows/Fonts"));
        }
        let mut out = vec![];
        fn walk(d: &std::path::Path, out: &mut Vec<(String, String)>, depth: u8) {
            let Ok(rd) = std::fs::read_dir(d) else { return };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() && depth < 4 {
                    walk(&p, out, depth + 1);
                } else if p.extension().is_some_and(|x| matches!(x.to_string_lossy().to_lowercase().as_str(), "ttf" | "otf")) {
                    let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    // Une entrée par famille : les variantes grasses et italiques se retrouvent toutes seules
                    let low = name.to_lowercase();
                    if ["bold", "italic", "oblique", "light", "thin", "medium", "black", "heavy", "semi", "extra", "condensed"].iter().any(|w| low.contains(w)) && !low.ends_with("regular") {
                        continue;
                    }
                    out.push((name.trim_end_matches("-Regular").to_string(), p.to_string_lossy().to_string()));
                }
            }
        }
        for d in dirs {
            walk(&d, &mut out, 0);
        }
        out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        out.dedup_by(|a, b| a.0 == b.0);
        out
    })
    .clone()
}

/// Nom de famille d'une police (et de sa variante) pour un texte : `font_family("…", true, false)`.
pub fn font_family(spec: &str, bold: bool, italic: bool) -> FontFamily {
    let v = match (bold, italic) {
        (false, false) => "r",
        (true, false) => "b",
        (false, true) => "i",
        (true, true) => "bi",
    };
    FontFamily::Name(format!("f:{spec}#{v}").into())
}

/// Ajoute une police à celles chargées (pour un texte qui la demande). Vrai si elle vient d'être ajoutée.
pub fn ensure_font(ctx: &egui::Context, spec: &str) -> bool {
    if spec.is_empty() {
        return false;
    }
    {
        let mut e = EXTRA.lock().unwrap_or_else(|e| e.into_inner());
        if e.iter().any(|x| x == spec) {
            return false;
        }
        e.push(spec.to_string());
    }
    let f = FONTS.lock().ok().and_then(|g| g.clone()).unwrap_or_default();
    install_fonts(ctx, &f);
    true
}

/// Charge les polices : Inter pour l'interface, JetBrains Mono pour les chiffres et le code (par défaut).
pub fn install_fonts(ctx: &egui::Context, chosen: &Fonts) {
    use std::sync::Arc;
    if let Ok(mut g) = FONTS.lock() {
        *g = Some(chosen.clone());
    }
    let mut fonts = egui::FontDefinitions::default();
    if let Ok(mut l) = LOADED.lock() {
        l.clear();
    }
    // Les polices de secours d'egui (symboles, émojis) restent derrière chaque famille
    let mut fallback: Vec<String> = fonts.families[&FontFamily::Proportional].clone();
    // Icônes Lucide : en secours de toutes les familles (zone privée de l'Unicode, aucun conflit)
    fonts.font_data.insert("lucide".into(), Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/Lucide.ttf"))));
    fallback.insert(0, "lucide".into());
    let add = |fonts: &mut egui::FontDefinitions, spec: &str| -> Option<[String; 4]> {
        let v = variants(spec)?;
        if let Ok(mut l) = LOADED.lock() {
            l.push(spec.to_string());
        }
        let keys = ["r", "b", "i", "bi"].map(|k| format!("f:{spec}#{k}"));
        for (k, bytes) in keys.iter().zip(v) {
            fonts.font_data.insert(k.clone(), Arc::new(egui::FontData::from_owned(bytes)));
            fonts.families.insert(FontFamily::Name(k.as_str().into()), std::iter::once(k.clone()).chain(fallback.iter().cloned()).collect());
        }
        Some(keys)
    };
    let pick = |s: &str, d: &str| if s.is_empty() { d.to_string() } else { s.to_string() };
    let ui = add(&mut fonts, &pick(&chosen.ui, "inter")).or_else(|| add(&mut fonts, "inter")).unwrap();
    let mono = add(&mut fonts, &pick(&chosen.mono, "jetbrains")).or_else(|| add(&mut fonts, "jetbrains")).unwrap();
    // Notes : Inter par défaut (la charte et la maquette), modifiable dans les réglages
    let notes = add(&mut fonts, &pick(&chosen.notes, "inter")).or_else(|| add(&mut fonts, "inter")).unwrap();
    for b in BUILTIN {
        add(&mut fonts, b.0);
    }
    for e in EXTRA.lock().map(|e| e.clone()).unwrap_or_default() {
        add(&mut fonts, &e);
    }
    let chain = |first: &str| -> Vec<String> { std::iter::once(first.to_string()).chain(fallback.iter().cloned()).collect() };
    fonts.families.insert(FontFamily::Proportional, chain(&ui[0]));
    fonts.families.insert(FontFamily::Monospace, chain(&mono[0]));
    fonts.families.insert(FontFamily::Name("bold".into()), chain(&ui[1]));
    fonts.families.insert(FontFamily::Name("italic".into()), chain(&ui[2]));
    fonts.families.insert(FontFamily::Name("mono-bold".into()), chain(&mono[1]));
    fonts.families.insert(FontFamily::Name("notes".into()), chain(&notes[0]));
    // Diapos PowerPoint : Lato, plus proche du rendu d'origine
    fonts.families.insert(FontFamily::Name("slide".into()), chain("f:lato#r"));
    fonts.families.insert(FontFamily::Name("slide-bold".into()), chain("f:lato#b"));
    ctx.set_fonts(fonts);
}

/// Les polices actuellement chargées.
pub fn fonts() -> Fonts {
    FONTS.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
}

/// Police des notes.
pub fn notes() -> FontFamily {
    FontFamily::Name("notes".into())
}

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn apply(ctx: &egui::Context, _dark: bool) {
    let dark = theme().dark;
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
        // Densité d'un logiciel de création : rangées de 22 px, champs compacts
        style.spacing.item_spacing = egui::vec2(6.0, 5.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
        style.spacing.interact_size.y = 22.0;

        let v = &mut style.visuals;
        *v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = p.panel;
        v.window_fill = p.raised;
        v.extreme_bg_color = p.bg;
        v.faint_bg_color = p.code_bg;
        v.override_text_color = Some(p.text);
        v.hyperlink_color = p.link;
        v.selection.bg_fill = p.selection;
        v.selection.stroke = Stroke::new(1.0, p.accent);
        v.window_stroke = Stroke::new(1.0, p.line);
        v.window_shadow = egui::Shadow::NONE;
        v.popup_shadow = egui::Shadow::NONE;
        v.window_corner_radius = CornerRadius::ZERO;
        v.menu_corner_radius = CornerRadius::ZERO;
        v.text_cursor.stroke = Stroke::new(2.0, p.accent);
        // Focus clavier bien visible (Tab) : contour à la couleur de l'accent
        v.selection.stroke = Stroke::new(if high_contrast() { 2.0 } else { 1.0 }, p.accent);
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
        // Comme Blender : les panneaux d'un éditeur sont séparés par un sillon sombre
        let k = |c: Color32, f: f32| Color32::from_rgb((c.r() as f32 * f) as u8, (c.g() as f32 * f) as u8, (c.b() as f32 * f) as u8);
        let groove = if p.bg.r() < 128 { k(p.panel, 0.42) } else { k(p.line, 0.82) };
        v.widgets.noninteractive.bg_stroke = Stroke::new(3.0, groove);
        v.widgets.noninteractive.fg_stroke.color = p.text;
        // Boutons secondaires et champs : un fond plein un cran sous le panneau, sans contour ;
        // le survol éclaircit, l'appui souligne à la couleur de l'espace
        v.widgets.inactive.bg_fill = p.line;
        v.slider_trailing_fill = true;
        // Comme la maquette : un fond à peine plus sombre que le panneau, cerné d'un filet (pas un bloc noir)
        v.widgets.inactive.weak_bg_fill = p.field;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, if p.bg.r() < 128 { p.line.gamma_multiply(0.75) } else { p.line });
        v.widgets.hovered.bg_fill = p.line;
        v.widgets.hovered.weak_bg_fill = p.line;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.line);
        v.widgets.active.bg_fill = p.line;
        v.widgets.active.weak_bg_fill = p.line;
        v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
        v.widgets.open.weak_bg_fill = p.line;
        v.widgets.open.bg_stroke = Stroke::new(1.0, p.line);
        v.extreme_bg_color = p.field;
        // Coins légèrement adoucis sur les commandes ; panneaux et cases restent nets
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = CornerRadius::same(2);
        }
        if high_contrast() {
            // Contraste élevé : chaque commande a un contour visible (3:1 au moins)
            v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.line);
            v.widgets.hovered.bg_stroke = Stroke::new(1.5, p.text);
            v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
            v.window_stroke = Stroke::new(1.5, p.text);
        }
        // Animations réduites : plus de fondus ni de défilement animé
        style.animation_time = if reduce_motion() { 0.0 } else { 1.0 / 12.0 };
        style.scroll_animation = if reduce_motion() { egui::style::ScrollAnimation::none() } else { egui::style::ScrollAnimation::default() };
    });
}

/// Le logo Aryadne (le A d'un seul fil, sur sa tuile), adapté au thème.
pub fn logo(ui: &mut egui::Ui, height: f32) -> egui::Response {
    let (uri, bytes): (&str, &'static [u8]) = if is_dark() {
        ("bytes://logo-dark.png", include_bytes!("../assets/logo-dark.png"))
    } else {
        ("bytes://logo-light.png", include_bytes!("../assets/logo-light.png"))
    };
    ui.add(egui::Image::from_bytes(uri, bytes).fit_to_exact_size(egui::vec2(height, height)))
}

/// Le motif seul (le A d'un seul fil, sans tuile), de la couleur d'accent choisie.
pub fn mark(ui: &mut egui::Ui, height: f32) -> egui::Response {
    let img = egui::Image::from_bytes("bytes://mark.png", include_bytes!("../assets/mark.png"))
        .tint(pal().accent)
        .fit_to_exact_size(egui::vec2(height, height));
    ui.add(img)
}

/// Icône de la fenêtre.
pub fn window_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../assets/icon.png")).ok()?.to_rgba8();
    Some(egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() })
}

/// Titre de section : casse normale (première lettre en capitale), demi-gras, couleur du texte.
pub fn section(text: &str) -> egui::RichText {
    let low = if text.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase()) { text.to_lowercase() } else { text.to_string() };
    let mut c = low.chars();
    let t = match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    };
    egui::RichText::new(t).size(12.0).color(pal().text).family(bold())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG AA : textes à 4,5:1, bordures en contraste élevé à 3:1, dans chaque thème
    #[test]
    fn contrastes_aa() {
        for t in &THEMES {
            let dark = t.id;
            for hc in [false, true] {
                set_theme(t.id);
                set_access(hc, false);
                let p = pal();
                for (nom, fg) in [("texte", p.text), ("secondaire", p.muted), ("lien", p.link), ("manquant", p.missing), ("réussi", p.success), ("attention", p.warning), ("info", p.info)] {
                    for bg in [p.bg, p.panel] {
                        assert!(contrast(fg, bg) >= 4.5, "{nom} sombre={dark} hc={hc} : {:.2}", contrast(fg, bg));
                    }
                }
                assert!(contrast(p.on_accent, p.accent) >= 4.5, "sur l'accent sombre={dark} hc={hc}");
                if hc {
                    assert!(contrast(p.line, p.bg) >= 3.0 && contrast(p.line, p.panel) >= 3.0, "bordures sombre={dark}");
                }
            }
        }
        set_access(false, false);
        set_theme("rose");
    }
}
