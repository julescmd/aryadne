//! Le panneau « Apprendre » : sommaire des parcours, page d'un parcours, page d'une leçon avec ses
//! blocs (essayer, étapes vérifiées, quiz, encadrés, défis). Il vit à côté de la zone de travail :
//! on lit la leçon et on fait les gestes dans le logiciel. La logique est dans `learn.rs`.
use super::*;
use crate::learn::{self, Block, Callout, Check, Progress, Source};

/// Où l'on est dans le panneau.
#[derive(Clone, PartialEq, Default, Debug)]
pub enum Page {
    #[default]
    Home,
    /// Un parcours (« 01-demarrer »)
    Course(String),
    /// Une leçon (« 01-demarrer/02-l-interface »)
    Lesson(String),
}

/// Réponse en cours à un quiz.
#[derive(Clone, Default)]
struct QuizState {
    picked: Vec<bool>,
    answered: bool,
    ok: bool,
}

#[derive(Default)]
pub struct Panel {
    pub open: bool,
    pub page: Page,
    pub progress: Progress,
    /// Coffre dont la progression est chargée
    root: String,
    /// Leçon affichée, déjà découpée en blocs
    lesson: Option<(String, learn::Lesson)>,
    quiz: HashMap<usize, QuizState>,
    /// Défis dont l'indice est déplié
    hints: HashSet<usize>,
    /// Progression à écrire sur le disque
    dirty: bool,
    /// Proposer le parcours Démarrer (premier lancement de ce coffre)
    offer: bool,
    /// Le panneau vient d'être ouvert : son onglet passe devant, sa colonne prend une largeur lisible
    pub focus: bool,
    pub widen: bool,
    /// Revenir en haut de la page au prochain affichage
    top: bool,
}

/// Ce que demande un clic dans le panneau (appliqué après le dessin).
enum Act {
    Go(Page),
    /// Créer (ou rouvrir) le point de départ du bloc `essayer` de ce rang ; vrai : repartir d'une copie neuve
    Try(usize, bool),
    Step(usize, bool),
    Quiz(usize),
    Finish,
    Restart,
    /// Lien cliqué dans le texte (chemin dans le coffre, depuis .app/docs)
    Link(String),
}

const DOCS_PREFIX: &str = ".app/docs/apprendre/";

fn level_color(niveau: &str) -> egui::Color32 {
    let p = theme::pal();
    match learn::level_rank(niveau) {
        0 => p.success,
        1 => p.warning,
        _ => p.missing,
    }
}

/// Puce arrondie : une icône et un mot.
fn chip(ui: &mut egui::Ui, icon: &str, text: &str, col: egui::Color32) -> egui::Response {
    let p = theme::pal();
    egui::Frame::new()
        .fill(col.gamma_multiply(0.14))
        .corner_radius(9)
        .inner_margin(egui::Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.horizontal(|ui| {
                if !icon.is_empty() {
                    ui.label(RichText::new(icon).size(11.0).color(col));
                }
                ui.label(RichText::new(text).size(11.0).color(p.text));
            });
        })
        .response
}

/// Bouton d'icône, avec son texte au survol.
fn icon_button(ui: &mut egui::Ui, icon: &str, tip: &str) -> bool {
    let p = theme::pal();
    let (r, resp) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 6.0, p.raised);
    }
    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(14.0), if resp.hovered() { p.text } else { p.muted });
    resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Barre d'avancement fine.
fn bar(ui: &mut egui::Ui, ratio: f32, col: egui::Color32) {
    let p = theme::pal();
    let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 4.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 2.0, p.secondary);
    if ratio > 0.0 {
        let mut f = r;
        f.set_width((r.width() * ratio.clamp(0.0, 1.0)).max(4.0));
        ui.painter().rect_filled(f, 2.0, col);
    }
}

/// Une carte : fond légèrement relevé, coins arrondis, bord discret (ou coloré).
fn card<R>(ui: &mut egui::Ui, edge: Option<egui::Color32>, body: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    let p = theme::pal();
    egui::Frame::new().fill(p.bg).corner_radius(8).stroke(egui::Stroke::new(1.0, edge.unwrap_or(p.line))).inner_margin(12).show(ui, |ui| {
        ui.set_width(ui.available_width());
        body(ui)
    })
}

/// Texte Markdown d'une leçon, rendu comme une note en lecture. Rend le lien cliqué.
/// Le texte tient dans la largeur du panneau ; un tableau trop large défile de côté sans élargir la page.
fn markdown(ui: &mut egui::Ui, base: &str, salt: impl std::hash::Hash + std::fmt::Debug, text: &str) -> Option<String> {
    thread_local! {
        static NONE: HashSet<String> = HashSet::new();
    }
    let mut out = None;
    let w = ui.available_width();
    ui.push_id(salt, |ui| {
        for (k, (table, part)) in learn::split_tables(text).into_iter().enumerate() {
            let mut show = |ui: &mut egui::Ui| {
                preview::set_base(base);
                let shown = NONE.with(|none| preview::show_doc(ui, &part, none, none));
                preview::set_base("");
                if let Some(preview::Out::Open(t)) = shown {
                    out = Some(t);
                }
            };
            ui.push_id(k, |ui| {
                if table {
                    egui::ScrollArea::horizontal().auto_shrink([false, true]).max_width(w).show(ui, show);
                } else {
                    ui.scope(|ui| {
                        ui.set_max_width(w);
                        show(ui);
                    });
                }
            });
        }
    });
    out
}

/// Les deux-points : précédés d'une espace en français, collés en anglais.
fn colon() -> &'static str {
    if crate::i18n::current() == "fr" { " :" } else { ":" }
}

/// Ce qu'ouvre un bloc `essayer`, en une ligne.
fn source_label(src: &Source) -> String {
    match src {
        Source::Nouveau { ext, .. } => {
            let kind = formats::ALL.iter().find(|f| f.ext == ext).map(|f| crate::i18n::t_mid(f.label)).unwrap_or_else(|| match ext.as_str() {
                "md" => crate::i18n::t_mid("note"),
                e => format!(".{e}"),
            });
            format!("{}{} {kind}", crate::i18n::t("nouveau fichier"), colon())
        }
        Source::Exemple(n) => format!("{}{} {n}", crate::i18n::t("exemple"), colon()),
        Source::Fichier(f) => format!("{}{} {}", crate::i18n::t("fichier fourni"), colon(), f.rsplit('/').next().unwrap_or(f)),
    }
}

impl Panel {
    /// La leçon de cette page, lue une fois.
    fn lesson(&mut self, id: &str) -> Option<&learn::Lesson> {
        if self.lesson.as_ref().is_none_or(|(l, _)| l != id) {
            self.lesson = learn::page_text(id).map(|t| (id.to_string(), learn::Lesson::parse(t)));
            self.quiz.clear();
            self.hints.clear();
        }
        self.lesson.as_ref().map(|(_, l)| l)
    }

    fn save(&mut self) {
        if std::mem::take(&mut self.dirty) && !self.root.is_empty() {
            self.progress.panneau = self.open;
            if let Err(e) = self.progress.save(&self.root) {
                crate::notify(e);
            }
        }
    }
}

impl App {
    /// Charge la progression du coffre ouvert (à chaque changement de coffre).
    pub(super) fn learn_sync(&mut self) {
        let root = self.root();
        if self.learn.root == root {
            return;
        }
        let progress = Progress::load(&root);
        let cat = learn::catalog();
        let offer = progress.fresh && !progress.accueil_propose && cat.iter().any(|c| c.id == learn::FIRST);
        let open = progress.panneau;
        let page = match progress.resume(cat) {
            Some(l) if open => Page::Lesson(l),
            _ => Page::Home,
        };
        self.learn = Panel { open, page, progress, root, offer, focus: open, widen: open, ..Default::default() };
    }

    pub(crate) fn toggle_learn(&mut self) {
        if self.learn.open {
            self.learn.open = false;
        } else {
            self.open_learn(None);
        }
        self.learn.dirty = true;
    }

    /// Ouvre le panneau, sur une page précise ou là où il était.
    pub(crate) fn open_learn(&mut self, page: Option<Page>) {
        self.learn.open = true;
        self.learn.focus = true;
        self.learn.widen = true;
        self.learn.dirty = true;
        if let Some(p) = page {
            self.learn_go(p);
        }
    }

    fn learn_go(&mut self, page: Page) {
        if let Page::Lesson(id) = &page {
            if learn::page_text(id).is_none() {
                return;
            }
            self.learn.progress.derniere = Some(id.clone());
            self.learn.dirty = true;
        }
        self.learn.page = page;
        self.learn.top = true;
    }

    /// Où reprendre, pour l'accueil : (leçon, titre de la leçon, titre du parcours, déjà commencé ?).
    pub(super) fn learn_resume(&self) -> Option<(String, String, String, bool)> {
        let cat = learn::catalog();
        let (id, started) = match self.learn.progress.resume(cat) {
            Some(id) => (id, true),
            None => {
                // Rien de commencé : la première leçon. Tout est fini : rien à proposer.
                if self.learn.progress.derniere.is_some() {
                    return None;
                }
                (cat.first()?.lessons.first()?.id.clone(), false)
            }
        };
        let c = cat.iter().find(|c| c.id == learn::course_of(&id))?;
        let l = c.lessons.iter().find(|l| l.id == id)?;
        Some((id, l.meta.titre.clone(), c.meta.titre.clone(), started))
    }

    /// L'état que lisent les vérifications des étapes, pris sur l'application telle qu'elle est.
    pub(super) fn learn_state(&self) -> learn::State<'_> {
        let mut s = learn::State { mode: self.ws.cur().kind.clone(), tool: self.cur_tool().to_string(), ..Default::default() };
        let ext_of = |rel: &str| formats::of(rel).map(|f| f.ext.to_string()).unwrap_or_else(|| ext(rel));
        if self.home_open {
            return s;
        }
        match self.view() {
            View::Boards => {
                if let Some(rel) = &self.board.rel {
                    s.ext = ext_of(rel);
                    s.saved = self.board.dirty_since.is_none();
                }
            }
            View::Files => {
                let Some(d) = self.current() else { return s };
                s.ext = ext_of(&d.rel);
                let mut dirty = d.dirty_since.is_some();
                match &d.content {
                    Content::Text(t) => s.text = Some(t.as_str()),
                    Content::Nodes(ed) => {
                        let (stats, layers, frame, pending) = ed.learn_state();
                        s.graph = Some(stats);
                        s.mat_layers = layers;
                        s.frame = frame;
                        dirty |= pending;
                    }
                    Content::Model(v) => {
                        if let Some((n, sel, frame)) = v.learn_state() {
                            s.objects = Some(n);
                            s.selection = sel;
                            s.frame = Some(frame);
                            (s.scene_sims, s.scene_mats) = v.learn_scene_extra();
                        }
                        dirty |= v.learn_dirty();
                    }
                    Content::Paint(ed) => {
                        s.layers = Some(ed.doc.layers.len());
                        dirty |= ed.save_due.is_some();
                    }
                    Content::Vector(ed) => {
                        s.layers = Some(ed.doc.shapes.iter().filter(|sh| sh.parent == 0).count());
                        dirty |= ed.save_due.is_some();
                    }
                    Content::Story(ed) => dirty |= ed.save_due.is_some(),
                    _ => {}
                }
                // Un fichier sans titre n'a pas encore sa place dans le coffre (Ctrl+S la lui donne)
                s.saved = !dirty && !untitled::is_untitled(&d.rel);
            }
            _ => {}
        }
        s
    }

    /// Coche les étapes dont la vérification est devenue vraie. Rend les étapes cochées à l'instant.
    fn learn_verify(&mut self) -> Vec<usize> {
        let Page::Lesson(id) = self.learn.page.clone() else { return vec![] };
        if self.learn.lesson(&id).is_none() {
            return vec![];
        }
        let Some((_, lesson)) = self.learn.lesson.as_ref() else { return vec![] };
        let steps: Vec<Option<Check>> = lesson.blocks.iter().filter_map(|b| if let Block::Etape { verifier, .. } = b { Some(verifier.clone()) } else { None }).collect();
        let done: Vec<bool> = (0..steps.len()).map(|k| self.learn.progress.step_done(&id, k)).collect();
        if !steps.iter().zip(&done).any(|(c, d)| !d && c.as_ref().is_some_and(|c| c.automatic())) {
            return vec![];
        }
        // Une étape qui suit un bloc `essayer` attend que son bouton ait servi
        let started = self.learn.progress.lesson(&id).map(|l| l.essais.keys().copied().collect::<Vec<_>>()).unwrap_or_default();
        let ready: Vec<bool> = lesson.step_starts().into_iter().map(|s| s.is_none_or(|k| started.contains(&k))).collect();
        let now = learn::newly_done(&steps, &done, &ready, &self.learn_state());
        for k in &now {
            self.learn.progress.set_step(&id, *k, true);
            self.learn.dirty = true;
        }
        now
    }

    /// Après un changement : la leçon est-elle terminée, et son parcours avec elle ?
    fn learn_after(&mut self, id: &str) {
        let Some((steps, quizzes)) = self.learn.lesson(id).map(|l| l.counts()) else { return };
        if self.learn.progress.update_done(id, steps, quizzes) {
            self.learn_celebrate(id);
        }
    }

    fn learn_celebrate(&mut self, id: &str) {
        self.learn.dirty = true;
        let cat = learn::catalog();
        match cat.iter().find(|c| c.id == learn::course_of(id)).filter(|c| self.learn.progress.course_finished(c)) {
            Some(c) => self.toast(format!("{}  {}{} {}", lucide::TROPHY, crate::i18n::t("parcours terminé"), colon(), c.meta.titre)),
            None => self.toast(format!("{}  {}", lucide::CIRCLE_CHECK, crate::i18n::t("leçon terminée"))),
        }
    }

    /// Bloc `essayer` : crée le point de départ dans <coffre>/Apprendre/ (ou rouvre celui déjà créé) et l'ouvre.
    fn learn_try(&mut self, id: &str, k: usize, fresh: bool) {
        let Some(src) = self.learn.lesson(id).and_then(|l| l.blocks.iter().filter_map(|b| if let Block::Essayer(r) = b { Some(r.clone().ok()) } else { None }).nth(k).flatten()) else { return };
        let root = self.root();
        let known = self.learn.progress.lesson(id).and_then(|l| l.essais.get(&k).cloned()).filter(|rel| std::path::Path::new(&root).join(rel).exists());
        let rel = match known.filter(|_| !fresh) {
            Some(rel) => rel,
            None => {
                self.flush_all();
                let Some(rel) = self.report(learn::start(&root, &src, id)) else { return };
                self.learn.progress.lecons.entry(id.to_string()).or_default().essais.insert(k, rel.clone());
                self.learn.dirty = true;
                self.refresh();
                rel
            }
        };
        self.open(rel);
        // Le fichier s'ouvre dans son espace : le panneau y reste ouvert, à sa place
        self.learn.focus = true;
        self.learn.widen = true;
    }

    /// Lien cliqué dans une leçon : une autre leçon s'ouvre dans le panneau, une page de la documentation dans la zone de travail.
    fn learn_link(&mut self, target: &str) {
        let (page, _) = target.split_once('#').unwrap_or((target, ""));
        if let Some(id) = page.strip_prefix(DOCS_PREFIX).and_then(|p| p.strip_suffix(".md")) {
            if let Some(course) = id.strip_suffix("/index") {
                if learn::catalog().iter().any(|c| c.id == course) {
                    return self.learn_go(Page::Course(course.to_string()));
                }
            }
            if learn::catalog().iter().any(|c| c.lessons.iter().any(|l| l.id == id)) {
                return self.learn_go(Page::Lesson(id.to_string()));
            }
        }
        if let Some(_home) = self.report(crate::docs::install(&self.root())) {
            if std::path::Path::new(&self.root()).join(page).exists() {
                self.actions.push(Action::OpenLink(target.to_string()));
            }
        }
    }

    /// Le panneau, dans sa case de la disposition ou dans sa propre fenêtre.
    pub(super) fn learn_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        if crate::dock::closed("learn") {
            self.learn.open = false;
            self.learn.dirty = true;
            return;
        }
        let fresh_steps = self.learn_verify();
        if !fresh_steps.is_empty() {
            if let Page::Lesson(id) = self.learn.page.clone() {
                self.learn_after(&id);
            }
            ui.ctx().request_repaint();
        }
        // Les images des leçons se lisent dans la copie de la documentation du coffre
        if matches!(self.learn.page, Page::Lesson(_) | Page::Course(_)) && !learn::catalog().is_empty() && !crate::docs::ASSETS.is_empty() {
            let _ = crate::docs::install(&self.root());
        }
        let mut acts: Vec<Act> = vec![];
        let mut panel = std::mem::take(&mut self.learn);
        let root = self.root();
        crate::dock::show_ex(ui, "learn", &crate::i18n::t("apprendre"), crate::dock::Side::Right, 400.0, 300.0, egui::Frame::new().fill(p.panel).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(0), true, |ui| {
            panel.ui(ui, &root, &mut acts);
        });
        self.learn = panel;
        for a in acts {
            match a {
                Act::Go(page) => self.learn_go(page),
                Act::Try(k, fresh) => {
                    if let Page::Lesson(id) = self.learn.page.clone() {
                        self.learn_try(&id, k, fresh);
                    }
                }
                Act::Step(k, on) => {
                    if let Page::Lesson(id) = self.learn.page.clone() {
                        if self.learn.progress.set_step(&id, k, on) {
                            self.learn.dirty = true;
                            self.learn_after(&id);
                        }
                    }
                }
                Act::Quiz(k) => {
                    if let Page::Lesson(id) = self.learn.page.clone() {
                        if self.learn.progress.pass_quiz(&id, k) {
                            self.learn.dirty = true;
                            self.learn_after(&id);
                        }
                    }
                }
                Act::Finish => {
                    if let Page::Lesson(id) = self.learn.page.clone() {
                        if self.learn.progress.finish(&id) {
                            self.learn_celebrate(&id);
                        }
                    }
                }
                Act::Restart => {
                    if let Page::Lesson(id) = self.learn.page.clone() {
                        self.learn.progress.reset_lesson(&id);
                        self.learn.quiz.clear();
                        self.learn.hints.clear();
                        self.learn.dirty = true;
                        self.learn.top = true;
                    }
                }
                Act::Link(t) => self.learn_link(&t),
            }
        }
        self.learn.save();
    }

    /// Premier lancement d'un coffre : une carte discrète, en bas à droite, propose le parcours Démarrer.
    /// Acceptée ou refusée, elle ne revient pas.
    pub(super) fn learn_offer(&mut self, ctx: &egui::Context) {
        if !self.learn.offer || self.compact {
            return;
        }
        let p = theme::pal();
        let Some(course) = learn::catalog().iter().find(|c| c.id == learn::FIRST) else { return };
        let mut answer: Option<bool> = None;
        egui::Area::new(egui::Id::unique("learn-offer")).order(egui::Order::Foreground).anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-18.0, -44.0)).show(ctx, |ui| {
            egui::Frame::new().fill(p.raised).corner_radius(10).stroke(egui::Stroke::new(1.0, p.accent.gamma_multiply(0.6))).inner_margin(14).shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: egui::Color32::from_black_alpha(70) }).show(ui, |ui| {
                ui.set_width(300.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(lucide::GRADUATION_CAP).size(18.0).color(p.accent));
                    ui.label(RichText::new(crate::i18n::t("Nouveau dans Aryadne ?")).size(14.0).family(theme::bold()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, lucide::X, &crate::i18n::t("Non merci (ne plus proposer)")) {
                            answer = Some(false);
                        }
                    });
                });
                ui.add_space(2.0);
                let n = course.lessons.len();
                let mut about = format!("{} · {n} {}", crate::i18n::t("Des leçons pas à pas, à suivre à côté de ton travail"), crate::i18n::t_mid("leçons"));
                if course.minutes() > 0 {
                    about.push_str(&format!(" · {}", learn::duration_text(course.minutes())));
                }
                ui.label(RichText::new(about).size(11.5).color(p.muted));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let go = egui::Button::new(RichText::new(format!("{}  {}", lucide::PLAY, crate::i18n::t("Commencer le parcours Démarrer"))).color(p.on_accent).family(theme::bold())).fill(p.accent).corner_radius(6).min_size(egui::vec2(0.0, 28.0));
                    if ui.add(go).clicked() {
                        answer = Some(true);
                    }
                    if ui.add(egui::Button::new(RichText::new(crate::i18n::t("Plus tard")).color(p.muted)).frame(false)).on_hover_text(crate::i18n::t("Les leçons restent dans Aide › Apprendre")).clicked() {
                        answer = Some(false);
                    }
                });
            });
        });
        if let Some(yes) = answer {
            self.learn.offer = false;
            self.learn.progress.accueil_propose = true;
            self.learn.dirty = true;
            if yes {
                let first = course.lessons.first().map(|l| Page::Lesson(l.id.clone())).unwrap_or(Page::Course(course.id.clone()));
                self.open_learn(Some(first));
            }
            self.learn.save();
        }
    }

    /// Carte de l'écran d'accueil : reprendre les leçons là où on s'est arrêté (ou les commencer).
    pub(super) fn learn_home_card(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let cat = learn::catalog();
        if cat.is_empty() {
            return;
        }
        let total: usize = cat.iter().map(|c| c.lessons.len()).sum();
        let done: usize = cat.iter().map(|c| self.learn.progress.course_done(c)).sum();
        let resume = self.learn_resume();
        let mut go: Option<Page> = None;
        egui::Frame::new().fill(p.panel).corner_radius(10).stroke(egui::Stroke::new(1.0, p.line)).inner_margin(14).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(lucide::GRADUATION_CAP).size(26.0).color(p.accent));
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    // Le bouton de droite garde sa place : le texte se coupe avant lui
                    ui.set_max_width((ui.available_width() - 170.0).max(80.0));
                    let (title, sub) = match &resume {
                        Some((_, lesson, course, true)) => (crate::i18n::t("Continuer là où tu t'es arrêté"), format!("{course} · {lesson}")),
                        Some((_, lesson, course, false)) => (crate::i18n::t("Apprendre Aryadne pas à pas"), format!("{}{} {course} · {lesson}", crate::i18n::t("première leçon"), colon())),
                        None => (crate::i18n::t("Toutes les leçons sont terminées"), crate::i18n::t("Tu peux les revoir quand tu veux")),
                    };
                    ui.add(egui::Label::new(RichText::new(title).size(14.0).family(theme::bold())).truncate());
                    ui.add(egui::Label::new(RichText::new(format!("{sub} · {done}/{total} {}", crate::i18n::t_mid("leçons"))).size(11.5).color(p.muted)).truncate());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = match &resume {
                        Some((_, _, _, true)) => "Continuer",
                        Some(_) => "Commencer",
                        None => "Revoir",
                    };
                    if ui.add(egui::Button::new(RichText::new(format!("{}  {}", lucide::PLAY, crate::i18n::t(label))).color(p.on_accent).family(theme::bold())).fill(p.accent).corner_radius(6).min_size(egui::vec2(0.0, 30.0))).clicked() {
                        go = Some(resume.as_ref().map(|r| Page::Lesson(r.0.clone())).unwrap_or(Page::Home));
                    }
                    if icon_button(ui, lucide::BOOK_OPEN, &crate::i18n::t("Tous les parcours")) {
                        go = Some(Page::Home);
                    }
                });
            });
            if done > 0 {
                ui.add_space(8.0);
                bar(ui, done as f32 / total.max(1) as f32, p.accent);
            }
        });
        if let Some(page) = go {
            self.home_open = false;
            self.open_learn(Some(page));
        }
    }
}

impl Panel {
    fn ui(&mut self, ui: &mut egui::Ui, root: &str, acts: &mut Vec<Act>) {
        let p = theme::pal();
        let cat = learn::catalog();
        // Fil d'Ariane, fixe en haut
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 8, bottom: 6 }).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let crumb = |ui: &mut egui::Ui, text: String, on: bool, tip: &str| -> bool {
                    let t = RichText::new(text).size(12.0).color(if on { p.text } else { p.muted });
                    let r = ui.add(egui::Label::new(if on { t.family(theme::bold()) } else { t }).truncate().sense(if on { egui::Sense::hover() } else { egui::Sense::click() }));
                    if on {
                        false
                    } else {
                        r.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
                    }
                };
                let home = self.page == Page::Home;
                if crumb(ui, format!("{}  {}", lucide::GRADUATION_CAP, crate::i18n::t("apprendre")), home, &crate::i18n::t("Tous les parcours")) {
                    acts.push(Act::Go(Page::Home));
                }
                let course_id = match &self.page {
                    Page::Course(c) => Some(c.clone()),
                    Page::Lesson(l) => Some(learn::course_of(l).to_string()),
                    Page::Home => None,
                };
                if let Some(c) = course_id.as_ref().and_then(|c| cat.iter().find(|x| x.id == *c)) {
                    ui.label(RichText::new(lucide::CHEVRON_RIGHT).size(11.0).color(p.muted));
                    let on = matches!(self.page, Page::Course(_));
                    // Dans une leçon, le parcours cède la place au titre de la leçon
                    ui.scope(|ui| {
                        if !on {
                            ui.set_max_width((ui.available_width() * 0.45).max(60.0));
                        }
                        if crumb(ui, c.meta.titre.clone(), on, &crate::i18n::t("Les leçons de ce parcours")) {
                            acts.push(Act::Go(Page::Course(c.id.clone())));
                        }
                    });
                    if let Page::Lesson(l) = &self.page {
                        if let Some(lr) = c.lessons.iter().find(|x| x.id == *l) {
                            ui.label(RichText::new(lucide::CHEVRON_RIGHT).size(11.0).color(p.muted));
                            crumb(ui, lr.meta.titre.clone(), true, "");
                        }
                    }
                }
            });
        });
        ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), egui::Stroke::new(1.0, p.line));
        // Bas de page d'une leçon : précédente / suivante, fixe en bas
        if let Page::Lesson(id) = self.page.clone() {
            let (prev, next) = learn::neighbours(cat, &id);
            let done = self.progress.done(&id);
            egui::Panel::bottom(ui.scope_id().with("learn-nav")).frame(egui::Frame::new().fill(p.panel).inner_margin(egui::Margin::symmetric(12, 8))).show_separator_line(true).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let b = |txt: String, strong: bool| {
                        let t = RichText::new(txt).size(12.0);
                        if strong {
                            egui::Button::new(t.color(p.on_accent).family(theme::bold())).fill(p.accent).corner_radius(6).min_size(egui::vec2(0.0, 28.0))
                        } else {
                            egui::Button::new(t).corner_radius(6).min_size(egui::vec2(0.0, 28.0))
                        }
                    };
                    if let Some(pv) = prev {
                        if ui.add(b(format!("{}  {}", lucide::ARROW_LEFT, crate::i18n::t("leçon précédente")), false)).clicked() {
                            acts.push(Act::Go(Page::Lesson(pv)));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match next {
                        Some(n) => {
                            if ui.add(b(format!("{}  {}", crate::i18n::t("leçon suivante"), lucide::ARROW_RIGHT), done)).clicked() {
                                acts.push(Act::Go(Page::Lesson(n)));
                            }
                        }
                        None => {
                            if ui.add(b(format!("{}  {}", lucide::BOOK_OPEN, crate::i18n::t("tous les parcours")), done)).clicked() {
                                acts.push(Act::Go(Page::Home));
                            }
                        }
                    });
                });
            });
        }
        let salt = format!("{:?}", self.page);
        let mut scroll = egui::ScrollArea::vertical().id_salt(salt).auto_shrink([false, false]);
        if std::mem::take(&mut self.top) {
            scroll = scroll.vertical_scroll_offset(0.0);
        }
        scroll.show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin { left: 14, right: 14, top: 12, bottom: 28 }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 6.0;
                match self.page.clone() {
                    Page::Home => self.home(ui, acts),
                    Page::Course(c) => self.course(ui, &c, acts),
                    Page::Lesson(l) => self.lesson_page(ui, root, &l, acts),
                }
            });
        });
    }

    /// Le sommaire : reprise, puis les parcours en cartes.
    fn home(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let p = theme::pal();
        let cat = learn::catalog();
        ui.label(RichText::new(crate::i18n::t("Apprendre Aryadne")).size(20.0).family(theme::bold()));
        ui.label(RichText::new(crate::i18n::t("Lis une leçon ici, fais les gestes dans le logiciel : les étapes se cochent toutes seules.")).size(12.0).color(p.muted));
        ui.add_space(8.0);
        if cat.is_empty() {
            ui.label(RichText::new(crate::i18n::t("Aucune leçon n'est embarquée dans cette version.")).color(p.muted));
            return;
        }
        let total: usize = cat.iter().map(|c| c.lessons.len()).sum();
        let done: usize = cat.iter().map(|c| self.progress.course_done(c)).sum();
        // Reprise
        let resume = self.progress.resume(cat).map(|id| (id, true)).or_else(|| if self.progress.derniere.is_none() { cat.first().and_then(|c| c.lessons.first()).map(|l| (l.id.clone(), false)) } else { None });
        if let Some((id, started)) = resume {
            if let Some((c, l)) = cat.iter().find(|c| c.id == learn::course_of(&id)).and_then(|c| c.lessons.iter().find(|l| l.id == id).map(|l| (c, l))) {
                let r = card(ui, Some(p.accent.gamma_multiply(0.7)), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(lucide::PLAY).size(18.0).color(p.accent));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(crate::i18n::t(if started { "Continuer là où tu t'es arrêté" } else { "Commencer ici" })).size(11.0).color(p.accent));
                            ui.add(egui::Label::new(RichText::new(&l.meta.titre).size(14.0).family(theme::bold())).wrap());
                            ui.label(RichText::new(&c.meta.titre).size(11.5).color(p.muted));
                            let ratio = self.progress.lesson_ratio(l);
                            if ratio > 0.0 {
                                ui.add_space(4.0);
                                bar(ui, ratio, p.accent);
                            }
                        });
                    });
                });
                if r.response.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    acts.push(Act::Go(Page::Lesson(id)));
                }
                ui.add_space(6.0);
            }
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new(crate::i18n::t("Parcours")).size(13.0).family(theme::bold()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("{done}/{total} {}", crate::i18n::t_mid("leçons terminées"))).size(11.0).color(p.muted));
            });
        });
        bar(ui, done as f32 / total.max(1) as f32, p.success);
        ui.add_space(4.0);
        for (i, c) in cat.iter().enumerate() {
            let n = c.lessons.len();
            let got = self.progress.course_done(c);
            let finished = got == n;
            let r = card(ui, None, |ui| {
                ui.horizontal(|ui| {
                    // Pastille : le numéro du parcours, ou une coche quand il est terminé
                    let (r, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), 14.0, if finished { p.success.gamma_multiply(0.25) } else { p.secondary });
                    if finished {
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, lucide::CHECK, egui::FontId::proportional(14.0), p.success);
                    } else {
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, format!("{}", i + 1), egui::FontId::new(12.5, theme::bold()), p.text);
                    }
                    ui.vertical(|ui| {
                        ui.add(egui::Label::new(RichText::new(&c.meta.titre).size(13.5).family(theme::bold())).wrap());
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                            if !c.meta.niveau.is_empty() {
                                chip(ui, "", &crate::i18n::t(&c.meta.niveau.to_lowercase()), level_color(&c.meta.niveau));
                            }
                            if c.minutes() > 0 {
                                chip(ui, lucide::CLOCK, &learn::duration_text(c.minutes()), p.muted);
                            }
                            chip(ui, lucide::BOOK_OPEN, &format!("{got}/{n}"), if finished { p.success } else { p.muted }).on_hover_text(format!("{got}/{n} {}", crate::i18n::t_mid("leçons terminées")));
                        });
                        if got > 0 && !finished {
                            ui.add_space(2.0);
                            bar(ui, got as f32 / n.max(1) as f32, p.accent);
                        }
                    });
                });
            });
            let resp = r.response.interact(egui::Sense::click());
            if resp.hovered() {
                ui.painter().rect_stroke(resp.rect, 8.0, egui::Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                acts.push(Act::Go(Page::Course(c.id.clone())));
            }
        }
    }

    /// Un parcours : sa présentation et la liste de ses leçons.
    fn course(&mut self, ui: &mut egui::Ui, id: &str, acts: &mut Vec<Act>) {
        let p = theme::pal();
        let Some(c) = learn::catalog().iter().find(|c| c.id == id) else {
            acts.push(Act::Go(Page::Home));
            return;
        };
        let n = c.lessons.len();
        let got = self.progress.course_done(c);
        ui.add(egui::Label::new(RichText::new(&c.meta.titre).size(20.0).family(theme::bold())).wrap());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            if !c.meta.niveau.is_empty() {
                chip(ui, "", &crate::i18n::t(&c.meta.niveau.to_lowercase()), level_color(&c.meta.niveau));
            }
            if c.minutes() > 0 {
                chip(ui, lucide::CLOCK, &learn::duration_text(c.minutes()), p.muted);
            }
            chip(ui, lucide::BOOK_OPEN, &format!("{n} {}", crate::i18n::t_mid("leçons")), p.muted);
            if got == n {
                chip(ui, lucide::TROPHY, &crate::i18n::t("parcours terminé"), p.success);
            }
        });
        bar(ui, got as f32 / n.max(1) as f32, if got == n { p.success } else { p.accent });
        ui.add_space(4.0);
        if !c.meta.objectifs.is_empty() {
            objectives(ui, &c.meta.objectifs);
        }
        // La présentation du parcours (index.md)
        if let Some(text) = learn::page_text(&format!("{id}/index")) {
            let lesson = learn::Lesson::parse(text);
            for (k, b) in lesson.blocks.iter().enumerate() {
                if let Block::Texte(t) = b {
                    if let Some(link) = markdown(ui, &format!("{DOCS_PREFIX}{id}/index.md"), ("course", k), t) {
                        acts.push(Act::Link(link));
                    }
                }
            }
        }
        ui.add_space(6.0);
        ui.label(RichText::new(crate::i18n::t("Leçons")).size(13.0).family(theme::bold()));
        // La prochaine à faire est mise en avant
        let next = c.lessons.iter().find(|l| !self.progress.done(&l.id)).map(|l| l.id.clone());
        for (i, l) in c.lessons.iter().enumerate() {
            let done = self.progress.done(&l.id);
            let ratio = self.progress.lesson_ratio(l);
            let is_next = next.as_deref() == Some(l.id.as_str());
            let r = card(ui, is_next.then(|| p.accent.gamma_multiply(0.7)), |ui| {
                ui.horizontal(|ui| {
                    let (icon, col) = if done { (lucide::CIRCLE_CHECK, p.success) } else if is_next { (lucide::PLAY, p.accent) } else { (lucide::CIRCLE, p.muted) };
                    ui.label(RichText::new(icon).size(16.0).color(col));
                    ui.vertical(|ui| {
                        ui.add(egui::Label::new(RichText::new(format!("{}. {}", i + 1, l.meta.titre)).size(13.0).family(theme::bold())).wrap());
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                            if !l.meta.niveau.is_empty() && l.meta.niveau != c.meta.niveau {
                                chip(ui, "", &crate::i18n::t(&l.meta.niveau.to_lowercase()), level_color(&l.meta.niveau));
                            }
                            if !l.meta.duree.is_empty() {
                                chip(ui, lucide::CLOCK, &l.meta.duree, p.muted);
                            }
                            if l.steps > 0 {
                                chip(ui, lucide::LIST_CHECKS, &format!("{}", l.steps), p.muted).on_hover_text(format!("{} {}", l.steps, crate::i18n::t_mid("étapes à faire dans le logiciel")));
                            }
                            if l.quizzes > 0 {
                                chip(ui, lucide::CIRCLE_HELP, &format!("{}", l.quizzes), p.muted).on_hover_text(format!("{} {}", l.quizzes, crate::i18n::t_mid("questions")));
                            }
                        });
                        if ratio > 0.0 && !done {
                            ui.add_space(2.0);
                            bar(ui, ratio, p.accent);
                        }
                    });
                });
            });
            let resp = r.response.interact(egui::Sense::click());
            if resp.hovered() {
                ui.painter().rect_stroke(resp.rect, 8.0, egui::Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                acts.push(Act::Go(Page::Lesson(l.id.clone())));
            }
        }
    }

    /// Une leçon : son en-tête, puis ses blocs dans l'ordre.
    fn lesson_page(&mut self, ui: &mut egui::Ui, root: &str, id: &str, acts: &mut Vec<Act>) {
        let p = theme::pal();
        let cat = learn::catalog();
        let Some(lesson) = self.lesson(id).cloned() else {
            acts.push(Act::Go(Page::Home));
            return;
        };
        let base = format!("{DOCS_PREFIX}{id}.md");
        let (steps, quizzes) = lesson.counts();
        let done = self.progress.done(id);
        let title = if lesson.meta.titre.is_empty() { learn::pretty(id) } else { lesson.meta.titre.clone() };
        ui.add(egui::Label::new(RichText::new(title).size(20.0).family(theme::bold())).wrap());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            if !lesson.meta.niveau.is_empty() {
                chip(ui, "", &crate::i18n::t(&lesson.meta.niveau.to_lowercase()), level_color(&lesson.meta.niveau));
            }
            if !lesson.meta.duree.is_empty() {
                chip(ui, lucide::CLOCK, &lesson.meta.duree, p.muted);
            }
            if done {
                chip(ui, lucide::CIRCLE_CHECK, &crate::i18n::t("leçon terminée"), p.success);
            }
        });
        if steps + quizzes > 0 {
            let got = (0..steps).filter(|k| self.progress.step_done(id, *k)).count() + (0..quizzes).filter(|k| self.progress.quiz_done(id, *k)).count();
            bar(ui, if done { 1.0 } else { got as f32 / (steps + quizzes) as f32 }, if done { p.success } else { p.accent });
        }
        // Prérequis : des liens vers les leçons à avoir suivies
        if !lesson.meta.prerequis.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                ui.label(RichText::new(crate::i18n::t("Avant cette leçon :")).size(11.5).color(p.muted));
                for pre in &lesson.meta.prerequis {
                    let known = cat.iter().flat_map(|c| c.lessons.iter()).find(|l| l.id == *pre);
                    let name = known.map(|l| l.meta.titre.clone()).unwrap_or_else(|| learn::pretty(pre));
                    let ok = self.progress.done(pre);
                    let r = chip(ui, if ok { lucide::CIRCLE_CHECK } else { lucide::ARROW_RIGHT }, &name, if ok { p.success } else { p.link });
                    if known.is_some() && r.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(crate::i18n::t("Ouvrir cette leçon")).clicked() {
                        acts.push(Act::Go(Page::Lesson(pre.clone())));
                    }
                }
            });
        }
        if !lesson.meta.objectifs.is_empty() {
            objectives(ui, &lesson.meta.objectifs);
        }
        ui.add_space(4.0);
        // La première étape pas encore cochée : c'est celle « en cours »
        let current = (0..steps).find(|k| !self.progress.step_done(id, *k));
        let starts = lesson.step_starts();
        let started: Vec<usize> = self.progress.lesson(id).map(|l| l.essais.keys().copied().collect()).unwrap_or_default();
        let (mut step_k, mut quiz_k, mut try_k) = (0usize, 0usize, 0usize);
        for (bi, b) in lesson.blocks.iter().enumerate() {
            match b {
                Block::Texte(t) => {
                    if let Some(link) = markdown(ui, &base, ("txt", bi), t) {
                        acts.push(Act::Link(link));
                    }
                }
                Block::Essayer(src) => {
                    let k = try_k;
                    try_k += 1;
                    self.try_block(ui, root, id, k, src, acts);
                }
                Block::Etape { texte, verifier } => {
                    let k = step_k;
                    step_k += 1;
                    let on = self.progress.step_done(id, k);
                    let auto = verifier.as_ref().is_some_and(|c| c.automatic());
                    let edge = if on { Some(p.success.gamma_multiply(0.6)) } else if current == Some(k) { Some(p.accent) } else { None };
                    card(ui, edge, |ui| {
                        ui.horizontal_top(|ui| {
                            let (r, resp) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
                            let (icon, col) = if on { (lucide::SQUARE_CHECK, p.success) } else { (lucide::SQUARE, if resp.hovered() { p.text } else { p.muted }) };
                            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(17.0), col);
                            let tip = if on {
                                crate::i18n::t("Étape faite (clic : décocher)")
                            } else if auto {
                                crate::i18n::t("Se coche toute seule quand c'est fait. Clic : cocher à la main.")
                            } else {
                                crate::i18n::t("Coche quand c'est fait")
                            };
                            if resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                acts.push(Act::Step(k, !on));
                            }
                            ui.vertical(|ui| {
                                ui.label(RichText::new(format!("{} {}/{steps}", crate::i18n::t("étape"), k + 1)).size(10.5).color(if on { p.success } else { p.muted }));
                                if let Some(link) = markdown(ui, &base, ("step", bi), texte) {
                                    acts.push(Act::Link(link));
                                }
                                if auto && !on {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 4.0;
                                        ui.label(RichText::new(lucide::SPARKLES).size(10.5).color(p.muted));
                                        let waiting = starts.get(k).copied().flatten().is_some_and(|t| !started.contains(&t));
                                        let note = if waiting { "Vérifiée automatiquement, après « Essaie par toi-même »" } else { "Vérifiée automatiquement" };
                                        ui.label(RichText::new(crate::i18n::t(note)).size(10.5).color(p.muted));
                                    });
                                }
                            });
                        });
                    });
                }
                Block::Quiz { question, choix, explication } => {
                    let k = quiz_k;
                    quiz_k += 1;
                    self.quiz_block(ui, &base, id, bi, k, question, choix, explication, acts);
                }
                Block::Encadre { genre, texte } => {
                    let (icon, label, col) = match genre {
                        Callout::Astuce => (lucide::LIGHTBULB, "astuce", p.info),
                        Callout::Attention => (lucide::TRIANGLE_ALERT, "attention", p.warning),
                        Callout::Raccourci => (lucide::KEYBOARD, "raccourci", p.accent),
                    };
                    let r = egui::Frame::new().fill(col.gamma_multiply(0.10)).corner_radius(8).inner_margin(egui::Margin { left: 14, right: 12, top: 8, bottom: 8 }).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            ui.label(RichText::new(icon).size(13.0).color(col));
                            ui.label(RichText::new(crate::i18n::t(label)).size(11.5).family(theme::bold()).color(col));
                        });
                        if let Some(link) = markdown(ui, &base, ("box", bi), texte) {
                            acts.push(Act::Link(link));
                        }
                    });
                    let rr = r.response.rect;
                    ui.painter().rect_filled(egui::Rect::from_min_size(rr.min, egui::vec2(3.0, rr.height())), egui::CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }, col);
                }
                Block::Defi { texte, indice } => {
                    card(ui, Some(p.highlight.gamma_multiply(0.7)), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            ui.label(RichText::new(lucide::TROPHY).size(14.0).color(p.highlight));
                            ui.label(RichText::new(crate::i18n::t("défi")).size(12.0).family(theme::bold()).color(p.highlight));
                        });
                        if let Some(link) = markdown(ui, &base, ("defi", bi), texte) {
                            acts.push(Act::Link(link));
                        }
                        if let Some(hint) = indice {
                            let open = self.hints.contains(&bi);
                            let label = format!("{}  {}", if open { lucide::CHEVRON_DOWN } else { lucide::CHEVRON_RIGHT }, crate::i18n::t(if open { "cacher l'indice" } else { "voir un indice" }));
                            if ui.add(egui::Button::new(RichText::new(label).size(11.5).color(p.muted)).frame(false)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                if open {
                                    self.hints.remove(&bi);
                                } else {
                                    self.hints.insert(bi);
                                }
                            }
                            if open {
                                egui::Frame::new().fill(p.secondary).corner_radius(6).inner_margin(8).show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    if let Some(link) = markdown(ui, &base, ("hint", bi), hint) {
                                        acts.push(Act::Link(link));
                                    }
                                });
                            }
                        }
                    });
                }
            }
            ui.add_space(2.0);
        }
        // Fin de la leçon
        ui.add_space(8.0);
        if done {
            card(ui, Some(p.success.gamma_multiply(0.6)), |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(lucide::CIRCLE_CHECK).size(18.0).color(p.success));
                    ui.label(RichText::new(crate::i18n::t("Leçon terminée")).size(13.5).family(theme::bold()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, lucide::ROTATE_CCW, &crate::i18n::t("Refaire la leçon (remet les étapes et les quiz à zéro ; tes fichiers restent)")) {
                            acts.push(Act::Restart);
                        }
                    });
                });
                if let Some(c) = cat.iter().find(|c| c.id == learn::course_of(id)).filter(|c| self.progress.course_finished(c)) {
                    ui.label(RichText::new(format!("{}  {}{} {}", lucide::TROPHY, crate::i18n::t("parcours terminé"), colon(), c.meta.titre)).size(12.0).color(p.success));
                }
            });
        } else if steps + quizzes == 0 {
            if ui.add(egui::Button::new(RichText::new(format!("{}  {}", lucide::CHECK, crate::i18n::t("J'ai fini cette leçon"))).color(p.on_accent).family(theme::bold())).fill(p.accent).corner_radius(6).min_size(egui::vec2(0.0, 30.0))).clicked() {
                acts.push(Act::Finish);
            }
        } else {
            let left = (0..steps).filter(|k| !self.progress.step_done(id, *k)).count() + (0..quizzes).filter(|k| !self.progress.quiz_done(id, *k)).count();
            ui.label(RichText::new(format!("{} {left}", crate::i18n::t("Encore à faire (étapes et questions) :"))).size(11.5).color(p.muted));
        }
    }

    /// Bloc `essayer` : le bouton « Essaie par toi-même ».
    fn try_block(&mut self, ui: &mut egui::Ui, root: &str, id: &str, k: usize, src: &Result<Source, String>, acts: &mut Vec<Act>) {
        let p = theme::pal();
        let problem = match src {
            Ok(s) => learn::target_of(s, id).err(),
            Err(e) => Some(e.clone()),
        };
        card(ui, Some(p.accent.gamma_multiply(0.5)), |ui| match (src, problem) {
            (Ok(src), None) => {
                let mine = self.progress.lesson(id).and_then(|l| l.essais.get(&k).cloned()).filter(|rel| std::path::Path::new(root).join(rel).exists());
                ui.horizontal(|ui| {
                    let label = if mine.is_some() { "Rouvrir mon essai" } else { "Essaie par toi-même" };
                    let b = egui::Button::new(RichText::new(format!("{}  {}", lucide::PLAY, crate::i18n::t(label))).color(p.on_accent).family(theme::bold())).fill(p.accent).corner_radius(6).min_size(egui::vec2(0.0, 30.0));
                    let tip = match &mine {
                        Some(rel) => format!("{} {}", crate::i18n::t("Ouvre"), vault::display_rel(rel)),
                        None => crate::i18n::t("Crée le point de départ dans le dossier Apprendre du coffre et l'ouvre dans le bon mode"),
                    };
                    if ui.add(b).on_hover_text(tip).clicked() {
                        acts.push(Act::Try(k, false));
                    }
                    if mine.is_some() && icon_button(ui, lucide::ROTATE_CCW, &crate::i18n::t("Repartir de zéro (nouvelle copie ; ton essai reste dans le coffre)")) {
                        acts.push(Act::Try(k, true));
                    }
                });
                let sub = match &mine {
                    Some(rel) => vault::display_rel(rel),
                    None => source_label(src),
                };
                ui.label(RichText::new(sub).size(11.0).color(p.muted));
            }
            (_, problem) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(lucide::TRIANGLE_ALERT).color(p.warning));
                    ui.label(RichText::new(format!("{} ({})", crate::i18n::t("Ce point de départ n'est pas disponible"), problem.unwrap_or_default())).size(11.5).color(p.muted));
                });
            }
        });
    }

    /// Bloc `quiz` : choix, correction, explication.
    #[allow(clippy::too_many_arguments)]
    fn quiz_block(&mut self, ui: &mut egui::Ui, base: &str, id: &str, bi: usize, k: usize, question: &str, choix: &[(String, bool)], explication: &str, acts: &mut Vec<Act>) {
        let p = theme::pal();
        let passed = self.progress.quiz_done(id, k);
        let multi = choix.iter().filter(|c| c.1).count() > 1;
        let st = self.quiz.entry(k).or_insert_with(|| QuizState { picked: vec![false; choix.len()], answered: false, ok: false });
        if st.picked.len() != choix.len() {
            st.picked = vec![false; choix.len()];
        }
        // Un quiz déjà réussi (une autre fois) se montre corrigé
        let show = st.answered || passed;
        let edge = if passed { Some(p.success.gamma_multiply(0.6)) } else if st.answered { Some(p.warning.gamma_multiply(0.7)) } else { None };
        card(ui, edge, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.label(RichText::new(lucide::CIRCLE_HELP).size(14.0).color(p.accent));
                ui.label(RichText::new(crate::i18n::t("question")).size(11.5).family(theme::bold()).color(p.accent));
                if multi {
                    ui.label(RichText::new(crate::i18n::t("(plusieurs bonnes réponses)")).size(10.5).color(p.muted));
                }
            });
            if !question.is_empty() {
                if let Some(link) = markdown(ui, base, ("quiz", bi), question) {
                    acts.push(Act::Link(link));
                }
            }
            ui.add_space(2.0);
            for (i, (text, good)) in choix.iter().enumerate() {
                let picked = if passed && !st.answered { *good } else { st.picked[i] };
                let (fill, stroke, icon, icol) = if show {
                    if *good && (picked || passed) {
                        (p.success.gamma_multiply(0.14), p.success, lucide::CIRCLE_CHECK, p.success)
                    } else if picked && !*good {
                        (p.missing.gamma_multiply(0.14), p.missing, lucide::CIRCLE_X, p.missing)
                    } else {
                        (egui::Color32::TRANSPARENT, p.line, lucide::CIRCLE, p.muted)
                    }
                } else if picked {
                    (p.accent.gamma_multiply(0.16), p.accent, if multi { lucide::SQUARE_CHECK } else { lucide::CIRCLE_CHECK }, p.accent)
                } else {
                    (egui::Color32::TRANSPARENT, p.line, if multi { lucide::SQUARE } else { lucide::CIRCLE }, p.muted)
                };
                let r = egui::Frame::new().fill(fill).corner_radius(6).stroke(egui::Stroke::new(1.0, stroke)).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icon).size(14.0).color(icol));
                        ui.add(egui::Label::new(RichText::new(text.replace("**", "")).size(12.5).color(p.text)).wrap().selectable(false));
                    });
                });
                let resp = r.response.interact(egui::Sense::click());
                if !show {
                    if resp.hovered() {
                        ui.painter().rect_stroke(resp.rect, 6.0, egui::Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
                    }
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        if multi {
                            st.picked[i] = !st.picked[i];
                        } else {
                            st.picked.iter_mut().for_each(|x| *x = false);
                            st.picked[i] = true;
                        }
                    }
                }
            }
            ui.add_space(4.0);
            if !show {
                let any = st.picked.iter().any(|x| *x);
                let b = egui::Button::new(RichText::new(format!("{}  {}", lucide::CHECK, crate::i18n::t("vérifier"))).color(if any { p.on_accent } else { p.muted }).family(theme::bold())).fill(if any { p.accent } else { p.secondary }).corner_radius(6).min_size(egui::vec2(0.0, 26.0));
                if ui.add_enabled(any, b).on_disabled_hover_text(crate::i18n::t("Choisis d'abord une réponse")).clicked() {
                    st.answered = true;
                    st.ok = choix.iter().zip(&st.picked).all(|(c, picked)| c.1 == *picked);
                    if st.ok {
                        acts.push(Act::Quiz(k));
                    }
                }
            } else {
                let ok = passed || st.ok;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    if ok {
                        ui.label(RichText::new(lucide::CIRCLE_CHECK).color(p.success));
                        ui.label(RichText::new(crate::i18n::t("Bonne réponse")).size(12.5).family(theme::bold()).color(p.success));
                    } else {
                        ui.label(RichText::new(lucide::CIRCLE_X).color(p.missing));
                        ui.label(RichText::new(crate::i18n::t("Pas tout à fait")).size(12.5).family(theme::bold()).color(p.missing));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new(format!("{}  {}", lucide::ROTATE_CCW, crate::i18n::t("réessayer"))).size(11.5)).corner_radius(6)).clicked() {
                                st.answered = false;
                                st.picked.iter_mut().for_each(|x| *x = false);
                            }
                        });
                    }
                });
                // L'explication ne vient qu'avec la bonne réponse : une erreur laisse chercher
                if ok && !explication.is_empty() {
                    if let Some(link) = markdown(ui, base, ("why", bi), explication) {
                        acts.push(Act::Link(link));
                    }
                }
            }
        });
    }
}

/// « Dans cette leçon » : les objectifs, en liste.
fn objectives(ui: &mut egui::Ui, list: &[String]) {
    let p = theme::pal();
    egui::Frame::new().fill(p.secondary.gamma_multiply(0.6)).corner_radius(8).inner_margin(10).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            ui.label(RichText::new(lucide::TARGET).size(13.0).color(p.accent));
            ui.label(RichText::new(crate::i18n::t("Tu vas apprendre à")).size(11.5).family(theme::bold()));
        });
        for o in list {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new("•").color(p.accent));
                ui.add(egui::Label::new(RichText::new(o).size(12.5)).wrap());
            });
        }
    });
}
