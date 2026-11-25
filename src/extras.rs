//! Fonctions de confort : palette de commandes, tâches, note du jour, modèles,
//! historique des versions, sauvegarde automatique, minuteur, import depuis une URL, tags.
use super::*;

// ---------- dates locales ----------

pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn now_label() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

fn weekday_fr() -> &'static str {
    use chrono::Datelike;
    ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"][chrono::Local::now().weekday().num_days_from_monday() as usize]
}

// ---------- palette de commandes ----------

/// Recherche floue : les lettres de `q` dans l'ordre dans `s`. Plus le score est haut, mieux
/// c'est : lettres collées, débuts de mots, et texte trouvé tel quel valent plus. None si absent.
pub fn fuzzy(q: &str, s: &str) -> Option<i32> {
    let q: Vec<char> = q.chars().filter(|c| !c.is_whitespace()).collect();
    if q.is_empty() {
        return Some(0);
    }
    let s: Vec<char> = s.chars().collect();
    let (mut qi, mut score, mut prev) = (0usize, 0i32, usize::MAX - 1);
    for (i, &c) in s.iter().enumerate() {
        if qi < q.len() && c == q[qi] {
            score += 1;
            if prev.wrapping_add(1) == i {
                score += 5;
            }
            if i == 0 || !s[i - 1].is_alphanumeric() {
                score += 8;
            }
            prev = i;
            qi += 1;
        }
    }
    if qi < q.len() {
        return None;
    }
    let qs: String = q.iter().collect();
    let ss: String = s.iter().collect();
    // Texte trouvé tel quel (à partir de 3 lettres : en dessous, les initiales comptent plus)
    if q.len() >= 3 && ss.contains(&qs) {
        score += 30;
    }
    Some(score - (s.len() as i32) / 8)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Cmd {
    View(View),
    NewNote,
    Daily,
    FromTemplate,
    Import,
    LinkFolder,
    ImportUrl,
    Transfer,
    Settings(settings::Tab),
    Theme,
    Pin,
    Compact,
    Assistant,
    CloseTab,
    History,
    LivePreview,
    Summarize,
    Backup,
    Sync,
    Split,
    Convert,
    OpenVaultFolder,
    ChangeVault,
    MenuEntry,
    Updates,
    Timer(u32),
    StopTimer,
    Organize,
    Reload,
    NewMaterial,
    NewLayered,
    NewScene,
    NewMontage,
    NewMotion,
    NewAudio,
    NewMusic,
    NewFile,
    UpgradeExtensions,
    NewCompositor,
    Stats,
    RunFile,
    NewBoard,
    NewVector,
}

impl App {
    fn commands(&self) -> Vec<(&'static str, &'static str, Cmd)> {
        use settings::Tab;
        let mut v = vec![
            ("aller : fichiers", "", Cmd::View(View::Files)),
            ("aller : cerveau", "Ctrl+G", Cmd::View(View::Brain)),
            ("aller : refs", "", Cmd::View(View::Refs)),
            ("aller : moodboard", "", Cmd::View(View::Boards)),
            ("aller : tâches", "", Cmd::View(View::Tasks)),
            ("aller : chat", "", Cmd::View(View::Chat)),
            ("aller : recherche", "Ctrl+Maj+F", Cmd::View(View::Search)),
            ("nouvelle note", "Ctrl+N", Cmd::NewNote),
            ("note du jour", "Ctrl+J", Cmd::Daily),
            ("nouvelle note depuis un modèle", "", Cmd::FromTemplate),
            ("nouveau board", "", Cmd::NewBoard),
            ("nouveau fichier… (choisir le genre)", "", Cmd::NewFile),
            ("coffre : passer aux nouvelles extensions", "", Cmd::UpgradeExtensions),
            ("nouvelle scène 3D", "", Cmd::NewScene),
            ("nouveau montage vidéo", "", Cmd::NewMontage),
            ("nouvelle composition animée (motion)", "", Cmd::NewMotion),
            ("nouvelle session audio", "", Cmd::NewAudio),
            ("nouveau morceau (séquenceur)", "", Cmd::NewMusic),
            ("nouveau matériau à calques (atelier)", "", Cmd::NewLayered),
            ("nouveau dessin vectoriel", "", Cmd::NewVector),
            ("nouveau matériau (nodes)", "", Cmd::NewMaterial),
            ("nouveau compositing (nodes)", "", Cmd::NewCompositor),
            ("importer des fichiers…", "", Cmd::Import),
            ("lier un dossier…", "", Cmd::LinkFolder),
            ("importer une image depuis une URL…", "", Cmd::ImportUrl),
            ("transfert entre ordinateurs", "", Cmd::Transfer),
            ("assistant IA", "Ctrl+L", Cmd::Assistant),
            ("ranger avec l'IA (cerveau)", "", Cmd::Organize),
            ("changer le thème clair / sombre", "", Cmd::Theme),
            ("épingler au-dessus des autres fenêtres", "", Cmd::Pin),
            ("mode compact", "Ctrl+Maj+M", Cmd::Compact),
            ("minuteur : 25 min de concentration", "", Cmd::Timer(25)),
            ("minuteur : 50 min", "", Cmd::Timer(50)),
            ("minuteur : pause 5 min", "", Cmd::Timer(5)),
            ("sauvegarder le coffre maintenant (zip)", "", Cmd::Backup),
            ("synchroniser avec le cloud", "", Cmd::Sync),
            ("vue côte à côte", "Ctrl+\\", Cmd::Split),
            ("convertir / compresser des fichiers…", "", Cmd::Convert),
            ("réglages : coffre en ligne", "", Cmd::Settings(Tab::Cloud)),
            ("statistiques du coffre", "", Cmd::Stats),
            ("ouvrir le dossier du coffre", "", Cmd::OpenVaultFolder),
            ("changer de coffre…", "", Cmd::ChangeVault),
            ("recharger les fichiers", "", Cmd::Reload),
            ("réglages : apparence", "", Cmd::Settings(Tab::Appearance)),
            ("réglages : éditeur", "", Cmd::Settings(Tab::Editor)),
            ("réglages : fichiers et liens", "", Cmd::Settings(Tab::Files)),
            ("réglages : IA", "", Cmd::Settings(Tab::Ai)),
            ("réglages : raccourcis", "", Cmd::Settings(Tab::Shortcuts)),
            (if cfg!(windows) { "ajouter au menu Démarrer" } else { "ajouter au menu des applications" }, "", Cmd::MenuEntry),
            ("vérifier les mises à jour", "", Cmd::Updates),
        ];
        if self.timer.is_some() {
            v.insert(0, ("arrêter le minuteur", "", Cmd::StopTimer));
        }
        if self.current().is_some() && self.view() == View::Files {
            v.insert(0, ("fichier : historique des versions", "", Cmd::History));
            v.insert(1, ("fichier : résumer avec l'IA", "", Cmd::Summarize));
            v.insert(2, ("fichier : fermer l'onglet", "Ctrl+W", Cmd::CloseTab));
            if self.current().is_some_and(|d| runner_for(&d.rel).is_some()) {
                v.insert(0, ("fichier : exécuter le script", "F5", Cmd::RunFile));
            }
            if self.current().is_some_and(|d| live::is_previewable(&d.rel)) {
                v.insert(0, ("fichier : aperçu live dans le navigateur", "", Cmd::LivePreview));
            }
        }
        v
    }

    pub(super) fn run_cmd(&mut self, cmd: Cmd, ctx: &egui::Context) {
        match cmd {
            Cmd::View(v) => self.view = Some(v),
            Cmd::NewNote => self.new_note(),
            Cmd::Daily => self.daily_note(),
            Cmd::FromTemplate => self.template_dlg = Some(TemplateDlg::default()),
            Cmd::Import => self.pick_import(false),
            Cmd::LinkFolder => self.pick_import(true),
            Cmd::ImportUrl => self.url_dlg = Some(String::new()),
            Cmd::Transfer => self.dialog = Some(Dialog::Transfer { send: !self.send_files.is_empty() }),
            Cmd::Settings(t) => self.open_settings_tab(ctx, t),
            Cmd::Theme => self.set_dark(ctx, !theme::is_dark()),
            Cmd::Pin => self.set_pinned(ctx, !self.pinned),
            Cmd::Compact => self.compact = !self.compact,
            Cmd::Assistant => self.toggle_assistant(),
            Cmd::CloseTab => {
                if let Some(i) = self.active {
                    self.actions.push(Action::Close(i));
                }
            }
            Cmd::History => {
                if let Some(rel) = self.current().map(|d| d.rel.clone()) {
                    self.flush_all();
                    self.history_dlg = Some(HistoryDlg { versions: history_list(&self.root(), &rel), rel, sel: 0, diff: true });
                }
            }
            Cmd::LivePreview => {
                if let Some(rel) = self.current().map(|d| d.rel.clone()) {
                    self.flush_all();
                    let r = live::open(&self.root(), &rel);
                    if let Some(url) = self.report(r) {
                        self.toast = Some((format!("aperçu : {url}"), Instant::now()));
                    }
                }
            }
            Cmd::Summarize => self.actions.push(Action::Summarize),
            Cmd::Backup => self.start_backup(true),
            Cmd::Sync => self.start_sync(true),
            Cmd::Split => self.toggle_split(),
            Cmd::Convert => {
                let cur = self.current().and_then(|d| vault::safe(&self.root(), &d.rel).ok()).filter(|p| convert::kind_of(p).is_some());
                let paths = match cur {
                    Some(p) => vec![p],
                    None => rfd::FileDialog::new().set_title("Fichiers à convertir").pick_files().unwrap_or_default(),
                };
                if !paths.is_empty() {
                    self.convert = convert::Dlg::new(paths);
                }
            }
            Cmd::OpenVaultFolder => {
                let r = settings::open_external(std::path::Path::new(&self.root()));
                self.report(r);
            }
            Cmd::ChangeVault => self.pick_vault(),
            Cmd::Reload => self.refresh(),
            Cmd::MenuEntry => match desktop::install() {
                Ok(()) => {
                    self.prefs.menu_entry = true;
                    self.prefs.save();
                    self.toast = Some(("Aryadne est dans le menu des applications".into(), Instant::now()));
                }
                Err(e) => self.error = Some(e),
            },
            Cmd::Updates => self.dialog = Some(Dialog::Update),
            Cmd::Timer(min) => self.timer = Some(Timer { start: Instant::now(), minutes: min, label: if min <= 10 { "pause" } else { "focus" } }),
            Cmd::StopTimer => self.timer = None,
            Cmd::NewFile => self.new_file = Some((0, String::new())),
            Cmd::UpgradeExtensions => {
                // Renomme les fichiers aux anciennes extensions, puis corrige les fichiers qui les citent
                self.flush_all();
                let root = self.root();
                let open: Vec<String> = self.docs.iter().map(|d| d.rel.clone()).collect();
                let active = self.active.and_then(|i| self.docs.get(i)).map(|d| d.rel.clone());
                let mut renamed: Vec<(String, String)> = vec![];
                for e in self.entries.iter().filter(|e| !e.is_dir) {
                    let Some(new) = crate::formats::upgrade(&e.rel) else { continue };
                    if self.entries.iter().any(|x| x.rel == new) {
                        continue;
                    }
                    if let (Ok(a), Ok(b)) = (vault::safe(&root, &e.rel), vault::safe(&root, &new)) {
                        if std::fs::rename(a, b).is_ok() {
                            renamed.push((e.rel.clone(), new));
                        }
                    }
                }
                if renamed.is_empty() {
                    self.toast = Some((crate::i18n::t("toutes les extensions sont déjà à jour"), Instant::now()));
                } else {
                    let (fixed, _) = crate::formats::relink(std::path::Path::new(&root), &renamed);
                    self.docs.clear();
                    self.active = None;
                    self.split = None;
                    self.refresh();
                    let map = |r: &String| renamed.iter().find(|x| &x.0 == r).map(|x| x.1.clone()).unwrap_or_else(|| r.clone());
                    for r in &open {
                        self.open(map(r));
                    }
                    if let Some(a) = active {
                        self.open(map(&a));
                    }
                    self.toast = Some((format!("{} {} · {fixed} {}", renamed.len(), crate::i18n::t_mid("fichier(s) renommé(s)"), crate::i18n::t_mid("fichier(s) corrigé(s) pour suivre les nouveaux noms")), Instant::now()));
                }
            }
            Cmd::NewMontage => self.new_untitled(crate::formats::ext("seq"), |p| crate::montage::Project::default().write(p)),
            Cmd::NewMusic => self.new_untitled(crate::formats::ext("music"), |p| crate::music::Song::starter().write(p)),
            Cmd::NewAudio => self.new_untitled(crate::formats::ext("audio"), |p| crate::audio::Session::starter().write(p)),
            Cmd::NewMotion => self.new_untitled(crate::formats::ext("motion"), |p| crate::motion::Compo::default().write(p)),
            Cmd::NewScene => self.new_untitled(crate::formats::ext("scene"), |p| crate::scene::Scene::starter().write(p)),
            Cmd::NewMaterial => self.new_untitled(crate::formats::ext("mat"), |p| nodes::Graph::new_material().save(p)),
            Cmd::NewLayered => self.new_untitled(crate::formats::ext("mat"), |p| nodes::Graph::new_layered().save(p)),
            Cmd::NewCompositor => self.new_untitled(crate::formats::ext("comp"), |p| nodes::Graph::new_compositor().save(p)),
            Cmd::NewVector => self.new_untitled(crate::formats::ext("vector"), |p| crate::vector::Doc::starter().save(p)),
            Cmd::NewBoard => {
                self.new_board("Moodboard");
                self.view = Some(View::Boards);
            }
            Cmd::Stats => self.stats = Some(compute_stats(&self.root(), &self.entries)),
            Cmd::RunFile => self.run_current(),
            Cmd::Organize => {
                self.view = Some(View::Brain);
                self.start_organize(ctx);
            }
        }
    }

    /// Ctrl+K ou Ctrl+Maj+P : toutes les actions de l'application et les fichiers du coffre,
    /// recherche floue (« nvscn » trouve « nouvelle scène »), dernières commandes en tête.
    pub(super) fn palette_ui(&mut self, ctx: &egui::Context) {
        let p = theme::pal();
        let Some((mut query, mut sel)) = self.palette.take() else { return };
        let q = crate::chat::fold(&query);
        let all = self.commands();
        let mut scored: Vec<(i32, (&str, &str, Cmd))> = all
            .into_iter()
            .filter_map(|c| {
                let s = fuzzy(&q, &crate::chat::fold(c.0)).max(fuzzy(&q, &crate::chat::fold(&crate::i18n::t(c.0))))?;
                let recent = self.cmd_recent.iter().position(|r| r == c.0).map(|i| 40 - i as i32 * 4).unwrap_or(0);
                Some((s + recent, c))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        // Raccourcis modifiables : ceux de la carte des raccourcis, pas ceux écrits en dur
        let live_keys = |l: &str, k: &str| -> String {
            let id = match l {
                "aller : recherche" => "search",
                "nouvelle note" => "new_note",
                "note du jour" => "daily",
                "assistant IA" => "assistant",
                "mode compact" => "compact",
                "vue côte à côte" => "split",
                "fichier : fermer l'onglet" => "close_tab",
                _ => return k.to_string(),
            };
            crate::keymap::text(id)
        };
        let mut matches: Vec<(String, String, Option<Cmd>)> = scored.into_iter().map(|(_, (l, k, c))| (crate::i18n::t(l), live_keys(l, k), Some(c))).collect();
        let labels: Vec<&'static str> = self.commands().into_iter().map(|c| c.0).collect();
        // Fichiers du coffre : « ouvrir » directement depuis la palette
        let mut files: Vec<(i32, String)> = vec![];
        if q.chars().count() >= 2 {
            files = self.entries.iter().filter(|e| !e.is_dir).filter_map(|e| fuzzy(&q, &crate::chat::fold(&e.rel)).map(|s| (s, e.rel.clone()))).collect();
            files.sort_by(|a, b| b.0.cmp(&a.0));
            files.truncate(8);
            for (_, rel) in &files {
                matches.push((format!("{} {}", crate::i18n::t("ouvrir"), vault::display_rel(rel)), String::new(), None));
            }
        }
        let (up, down, enter, esc) = ctx.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        if down && sel + 1 < matches.len() {
            sel += 1;
        }
        if up && sel > 0 {
            sel -= 1;
        }
        sel = sel.min(matches.len().saturating_sub(1));
        let mut chosen = None;
        let modal = egui::Modal::new(egui::Id::unique("palette")).frame(egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.text)).inner_margin(12)).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("⌘").color(p.accent).family(theme::bold()));
                ui.add(egui::TextEdit::singleline(&mut query).hint_text(crate::i18n::t("chercher une commande ou un fichier…")).frame(egui::Frame::NONE).desired_width(f32::INFINITY)).request_focus();
            });
            ui.separator();
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                for (i, (label, keys, cmd)) in matches.iter().enumerate() {
                    if cmd.is_none() && (i == 0 || matches[i - 1].2.is_some()) {
                        ui.label(RichText::new(crate::i18n::t("fichiers")).color(p.muted).size(10.5));
                    }
                    let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), egui::Sense::click());
                    if i == sel {
                        ui.painter().rect_filled(r, 0.0, p.accent.gamma_multiply(0.15));
                        if down || up {
                            ui.scroll_to_rect(r, None);
                        }
                    }
                    ui.painter().text(r.left_center() + egui::vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, label, egui::FontId::monospace(12.5), if i == sel { p.text } else { p.muted });
                    ui.painter().text(r.right_center() - egui::vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, keys, egui::FontId::monospace(11.0), p.muted);
                    if resp.clicked() {
                        chosen = Some(i);
                    }
                }
                if matches.is_empty() {
                    ui.label(RichText::new(crate::i18n::t("aucune commande")).color(p.muted));
                }
            });
        });
        if enter && !matches.is_empty() {
            chosen = Some(sel);
        }
        if let Some(i) = chosen {
            match matches[i].2 {
                Some(c) => {
                    // Retient la commande (par son libellé d'origine) pour la remonter la prochaine fois
                    if let Some(l) = labels.iter().find(|l| crate::i18n::t(l) == matches[i].0) {
                        self.cmd_recent.retain(|r| r != l);
                        self.cmd_recent.insert(0, l.to_string());
                        self.cmd_recent.truncate(10);
                    }
                    self.run_cmd(c, ctx);
                }
                None => {
                    let k = i - matches.iter().filter(|m| m.2.is_some()).count();
                    if let Some((_, rel)) = files.get(k) {
                        self.view = Some(View::Files);
                        self.open(rel.clone());
                    }
                }
            }
        } else if !(esc || modal.should_close()) {
            self.palette = Some((query, sel));
        }
    }

    // ---------- note du jour et modèles ----------

    pub(super) fn daily_note(&mut self) {
        let base = if self.notes_dir().is_empty() { "journal".to_string() } else { format!("{}/journal", self.notes_dir()) };
        let rel = format!("{base}/{}.md", today());
        let root = self.root();
        if !self.entries.iter().any(|e| e.rel == rel) {
            let _ = vault::create_dir(&root, &base);
            let text = format!("# {} {}\n\n## À faire\n- [ ] \n\n## Notes\n\n", weekday_fr(), today());
            if self.report(vault::write(&root, &rel, &text)).is_none() {
                return;
            }
            self.refresh();
        }
        self.view = Some(View::Files);
        self.open(rel);
    }

    pub(super) fn template_ui(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.template_dlg.take() else { return };
        let p = theme::pal();
        let root = self.root();
        ensure_templates(&root);
        let list = templates(&self.entries, &root);
        let mut close = false;
        let mut create = false;
        egui::Modal::new(egui::Id::unique("template")).frame(egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.text)).inner_margin(16)).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(caps(&crate::i18n::t("nouvelle note depuis un modèle")));
            ui.add_space(6.0);
            for (i, (name, _)) in list.iter().enumerate() {
                ui.radio_value(&mut d.sel, i, name.as_str());
            }
            ui.label(RichText::new(crate::i18n::t("Les modèles sont des notes du dossier modèles/ : modifie-les ou ajoutes-en. {{titre}}, {{date}} et {{heure}} sont remplacés.")).color(p.muted).size(11.0));
            ui.add_space(8.0);
            let r = ui.add(egui::TextEdit::singleline(&mut d.name).hint_text(crate::i18n::t("titre de la note")).desired_width(f32::INFINITY));
            if !r.has_focus() && d.name.is_empty() {
                r.request_focus();
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                create = true;
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                create |= ui.button(format!("{}  {}", crate::lucide::PLUS, crate::i18n::t("créer"))).clicked();
                close = ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("annuler"))).clicked() || ui.input(|i| i.key_pressed(Key::Escape));
            });
        });
        if create && !d.name.trim().is_empty() {
            if let Some((_, tpl_rel)) = list.get(d.sel) {
                let title = d.name.trim().to_string();
                let body = vault::read(&root, tpl_rel).unwrap_or_default().replace("{{titre}}", &title).replace("{{date}}", &today()).replace("{{heure}}", &now_label());
                let dir = self.target_dir(&self.notes_dir());
                let rel = vault::join(&dir, &format!("{}.md", title.replace(['/', '\\'], "-")));
                if self.entries.iter().any(|e| e.rel == rel) {
                    self.error = Some(format!("« {rel} » existe déjà."));
                } else if self.report(vault::write(&root, &rel, &body)).is_some() {
                    self.refresh();
                    self.view = Some(View::Files);
                    self.open(rel);
                }
            }
            return;
        }
        if !close {
            self.template_dlg = Some(d);
        }
    }

    // ---------- historique des versions ----------

    pub(super) fn history_ui(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.history_dlg.take() else { return };
        let p = theme::pal();
        let root = self.root();
        let mut close = false;
        let mut restore: Option<String> = None;
        egui::Modal::new(egui::Id::unique("history")).frame(egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.text)).inner_margin(16)).show(ctx, |ui| {
            ui.set_width(820.0);
            ui.label(caps(&format!("historique · {}", vault::display_rel(&d.rel))));
            ui.add_space(6.0);
            if d.versions.is_empty() {
                ui.label(RichText::new(crate::i18n::t("Aucune version enregistrée pour l'instant. Aryadne en garde une à chaque enregistrement (au plus une toutes les 2 minutes, les 40 dernières).")).color(p.muted));
            } else {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(200.0);
                        egui::ScrollArea::vertical().id_salt("hist-list").max_height(420.0).show(ui, |ui| {
                            for (i, (label, _)) in d.versions.iter().enumerate() {
                                ui.selectable_value(&mut d.sel, i, label.as_str());
                            }
                        });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        let text = d.versions.get(d.sel).and_then(|(_, f)| std::fs::read_to_string(f).ok()).unwrap_or_default();
                        let current = self.docs.iter().find(|x| x.rel == d.rel).and_then(|x| if let Content::Text(t) = &x.content { Some(t.clone()) } else { None }).unwrap_or_default();
                        let (added, removed) = line_diff(&current, &text);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("par rapport à maintenant : +{added} / −{removed} lignes")).color(p.muted).size(11.0));
                            ui.checkbox(&mut d.diff, crate::i18n::t("voir les différences"));
                        });
                        egui::ScrollArea::both().id_salt("hist-view").max_height(400.0).show(ui, |ui| {
                            if d.diff {
                                // − : dans cette version seulement ; + : ajouté depuis
                                for (k, l) in diff_lines(&text, &current) {
                                    let (prefix, col, bg) = match k {
                                        -1 => ("− ", p.missing, p.missing.gamma_multiply(0.12)),
                                        1 => ("+ ", egui::Color32::from_rgb(0x22, 0xa0, 0x55), egui::Color32::from_rgba_unmultiplied(0x22, 0xa0, 0x55, 30)),
                                        _ => ("  ", p.muted, egui::Color32::TRANSPARENT),
                                    };
                                    egui::Frame::new().fill(bg).show(ui, |ui| {
                                        ui.add(egui::Label::new(RichText::new(format!("{prefix}{l}")).monospace().size(12.0).color(col)).wrap_mode(egui::TextWrapMode::Extend));
                                    });
                                }
                            } else {
                                ui.add(egui::Label::new(RichText::new(&text).monospace().size(12.0)).wrap_mode(egui::TextWrapMode::Extend));
                            }
                        });
                        if ui.button(format!("{}  {}", crate::lucide::HISTORY, crate::i18n::t("restaurer cette version"))).clicked() {
                            restore = Some(text);
                        }
                    });
                });
            }
            ui.add_space(8.0);
            close = ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("fermer"))).clicked() || ui.input(|i| i.key_pressed(Key::Escape));
        });
        if let Some(text) = restore {
            if let Some(i) = self.docs.iter().position(|x| x.rel == d.rel) {
                if let Content::Text(t) = &mut self.docs[i].content {
                    *t = text;
                    self.docs[i].dirty_since = Some(Instant::now());
                }
            } else {
                let r = vault::write(&root, &d.rel, &text);
                self.report(r);
            }
            self.toast = Some(("version restaurée (l'ancienne reste dans l'historique)".into(), Instant::now()));
            return;
        }
        if !close {
            self.history_dlg = Some(d);
        }
    }

    // ---------- sauvegarde automatique ----------

    pub(super) fn start_backup(&mut self, forced: bool) {
        if self.backup_rx.is_some() || self.vault.is_none() {
            return;
        }
        if !forced {
            if !self.prefs.backup_daily {
                return;
            }
            let last = load_config("last_backup").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            if unix_now().saturating_sub(last) < 20 * 3600 {
                return;
            }
        }
        let root = self.root();
        let dest = backup_dir(&self.prefs);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(make_backup(&root, &dest));
        });
        self.backup_rx = Some(rx);
    }

    pub(super) fn poll_backup(&mut self) {
        if let Some(rx) = &self.backup_rx {
            if let Ok(r) = rx.try_recv() {
                self.backup_rx = None;
                match r {
                    Ok(path) => {
                        save_config("last_backup", &unix_now().to_string());
                        self.toast = Some((format!("coffre sauvegardé : {}", path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()), Instant::now()));
                    }
                    Err(e) => self.error = Some(format!("Sauvegarde impossible : {e}")),
                }
            }
        }
    }

    // ---------- minuteur ----------

    pub(super) fn timer_widget(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        match &self.timer {
            Some(t) => {
                let total = t.minutes as u64 * 60;
                let el = t.start.elapsed().as_secs();
                if el >= total {
                    let label = t.label;
                    self.timer = None;
                    self.toast = Some((if label == "pause" { "pause terminée, on s'y remet".into() } else { format!("{} min de {label} terminées : prends une pause", total / 60) }, Instant::now()));
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
                    return;
                }
                let left = total - el;
                let txt = format!("◷ {} {:02}:{:02}", t.label, left / 60, left % 60);
                if ui.add(egui::Button::new(RichText::new(txt).color(p.accent).size(11.0)).frame(false)).on_hover_text(crate::i18n::t("cliquer pour arrêter")).clicked() {
                    self.timer = None;
                }
                ui.ctx().request_repaint_after(Duration::from_secs(1));
            }
            None => {
                let r = ui.add(egui::Button::new(RichText::new(crate::i18n::t("◷ minuteur")).color(p.muted).size(11.0)).frame(false));
                egui::Popup::menu(&r).show(|ui| {
                    for (m, l) in [(25, "25 min de concentration"), (50, "50 min"), (5, "pause 5 min"), (15, "pause 15 min")] {
                        if ui.button(crate::i18n::t(l)).clicked() {
                            self.timer = Some(Timer { start: Instant::now(), minutes: m, label: if m <= 15 { "pause" } else { "focus" } });
                        }
                    }
                });
            }
        }
    }

    // ---------- image depuis une URL ----------

    pub(super) fn url_ui(&mut self, ctx: &egui::Context) {
        let Some(mut url) = self.url_dlg.take() else { return };
        let p = theme::pal();
        let mut close = false;
        let mut go = false;
        egui::Modal::new(egui::Id::unique("url")).frame(egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.text)).inner_margin(16)).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(caps(&crate::i18n::t("importer une image depuis une URL")));
            ui.add_space(6.0);
            let r = ui.add(egui::TextEdit::singleline(&mut url).hint_text("https://…/image.jpg").desired_width(f32::INFINITY));
            r.request_focus();
            go = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            ui.label(RichText::new(crate::i18n::t("L'image est enregistrée dans refs/ (Ctrl+V d'une URL dans les refs fait la même chose).")).color(p.muted).size(11.0));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                go |= ui.button(format!("{}  {}", crate::lucide::UPLOAD, crate::i18n::t("importer"))).clicked();
                close = ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("annuler"))).clicked() || ui.input(|i| i.key_pressed(Key::Escape));
            });
        });
        if go && !url.trim().is_empty() {
            self.import_url(url.trim().to_string(), ctx);
            return;
        }
        if !close {
            self.url_dlg = Some(url);
        }
    }

    pub(super) fn import_url(&mut self, url: String, ctx: &egui::Context) {
        let root = self.root();
        let dir = if self.entries.iter().any(|e| e.rel == "refs") { "refs".to_string() } else { String::new() };
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = download_image(&root, &dir, &url);
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.url_rx = Some(rx);
        self.toast = Some(("téléchargement…".into(), Instant::now()));
    }

    pub(super) fn poll_url(&mut self) {
        if let Some(rx) = &self.url_rx {
            if let Ok(r) = rx.try_recv() {
                self.url_rx = None;
                if let Some(rel) = self.report(r) {
                    self.refresh();
                    self.toast = Some((format!("importé : {rel}"), Instant::now()));
                    self.place_imported(vec![rel]);
                }
            }
        }
    }

    // ---------- tâches ----------

    pub(super) fn tasks_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let root = self.root();
        let stale = self.tasks.loaded.is_none_or(|t| t.elapsed() > Duration::from_secs(4)) || self.tasks.dirty;
        if stale {
            self.tasks.items = collect_tasks(&root, &self.entries, &self.docs);
            self.tasks.loaded = Some(Instant::now());
            self.tasks.dirty = false;
        }
        let mut toggle: Option<(String, usize)> = None;
        let mut open: Option<String> = None;
        let mut add = false;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg).inner_margin(egui::Margin::symmetric(24, 16))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(caps(&crate::i18n::t("tâches")));
                let open_n = self.tasks.items.iter().filter(|t| !t.done).count();
                ui.label(RichText::new(format!("{open_n} à faire · {} faites", self.tasks.items.len() - open_n)).color(p.muted).size(11.5));
                ui.add_space(12.0);
                ui.selectable_value(&mut self.tasks.show_done, false, crate::i18n::t("à faire"));
                ui.selectable_value(&mut self.tasks.show_done, true, crate::i18n::t("tout"));
                ui.add_space(12.0);
                ui.add(egui::TextEdit::singleline(&mut self.tasks.filter).hint_text(crate::i18n::t("filtrer…")).desired_width(180.0));
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut self.tasks.input).hint_text(crate::i18n::t("nouvelle tâche (ajoutée à tâches.md) · Entrée")).desired_width(420.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    add = true;
                }
                add |= ui.add(egui::Button::new(egui::RichText::new(crate::lucide::PLUS).size(14.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("ajouter")).clicked();
            });
            ui.add_space(4.0);
            ui.label(RichText::new(crate::i18n::t("Toutes les cases « - [ ] » de tes notes, regroupées par note. Cocher ici coche dans la note. Ajoute « @2026-12-01 » pour une échéance.")).color(p.muted).size(11.0));
            // Échéances à venir, toutes notes confondues
            let mut dated: Vec<&Task> = self.tasks.items.iter().filter(|t| !t.done && t.due.is_some()).collect();
            dated.sort_by(|a, b| a.due.cmp(&b.due));
            if !dated.is_empty() {
                ui.add_space(8.0);
                ui.label(caps(&crate::i18n::t("échéances")));
                for t in dated.iter().take(8) {
                    let d = t.due.clone().unwrap_or_default();
                    let late = d.as_str() < today().as_str();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&d).color(if late { p.missing } else { p.accent }).size(11.5));
                        ui.label(RichText::new(&t.text).size(12.5));
                        ui.label(RichText::new(vault::display_rel(&t.rel).trim_end_matches(".md").to_string()).color(p.muted).size(11.0));
                    });
                }
            }
            ui.add_space(10.0);
            let f = crate::chat::fold(&self.tasks.filter);
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                let mut last = String::new();
                let mut any = false;
                for t in &self.tasks.items {
                    if (t.done && !self.tasks.show_done) || (!f.is_empty() && !crate::chat::fold(&t.text).contains(&f) && !crate::chat::fold(&t.rel).contains(&f)) {
                        continue;
                    }
                    any = true;
                    if t.rel != last {
                        ui.add_space(8.0);
                        let name = vault::display_rel(&t.rel);
                        if ui.add(egui::Button::new(RichText::new(name.trim_end_matches(".md")).color(p.link).family(theme::bold())).frame(false)).clicked() {
                            open = Some(t.rel.clone());
                        }
                        last = t.rel.clone();
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(4.0 + t.indent as f32 * 16.0);
                        let mut done = t.done;
                        if ui.checkbox(&mut done, "").changed() {
                            toggle = Some((t.rel.clone(), t.line));
                        }
                        let txt = RichText::new(&t.text).size(13.0);
                        ui.label(if t.done { txt.strikethrough().color(p.muted) } else { txt });
                        if let Some(d) = &t.due {
                            let late = !t.done && d.as_str() < today().as_str();
                            let soon = !t.done && d.as_str() == today().as_str();
                            let col = if late { p.missing } else if soon { p.accent } else { p.muted };
                            ui.label(RichText::new(if late { format!("en retard · {d}") } else if soon { "aujourd'hui".to_string() } else { d.clone() }).color(col).size(11.0));
                        }
                    });
                }
                if !any {
                    ui.label(RichText::new(if self.tasks.items.is_empty() { "Aucune tâche. Écris « - [ ] quelque chose » dans une note, ou ajoute-en une ci-dessus." } else { "Rien à afficher avec ce filtre." }).color(p.muted));
                }
            });
        });
        if add && !self.tasks.input.trim().is_empty() {
            let dir = self.notes_dir();
            let rel = vault::join(&dir, "tâches.md");
            let line = format!("- [ ] {}\n", self.tasks.input.trim());
            if let Some(i) = self.docs.iter().position(|d| d.rel == rel) {
                if let Content::Text(t) = &mut self.docs[i].content {
                    if !t.ends_with('\n') && !t.is_empty() {
                        t.push('\n');
                    }
                    t.push_str(&line);
                    self.docs[i].dirty_since = Some(Instant::now());
                }
            } else {
                let mut t = vault::read(&root, &rel).unwrap_or_else(|_| "# Tâches\n\n".into());
                if !t.ends_with('\n') {
                    t.push('\n');
                }
                t.push_str(&line);
                if self.report(vault::write(&root, &rel, &t)).is_some() && !self.entries.iter().any(|e| e.rel == rel) {
                    self.refresh();
                }
            }
            self.tasks.input.clear();
            self.tasks.dirty = true;
        }
        if let Some((rel, line)) = toggle {
            let flip = |t: &str| -> String {
                let mut lines: Vec<String> = t.split('\n').map(String::from).collect();
                if let Some(l) = lines.get_mut(line) {
                    if let Some(i) = l.find("[ ]") {
                        l.replace_range(i..i + 3, "[x]");
                    } else if let Some(i) = l.find("[x]").or_else(|| l.find("[X]")) {
                        l.replace_range(i..i + 3, "[ ]");
                    }
                }
                lines.join("\n")
            };
            if let Some(i) = self.docs.iter().position(|d| d.rel == rel) {
                if let Content::Text(t) = &mut self.docs[i].content {
                    *t = flip(t);
                    self.docs[i].dirty_since = Some(Instant::now());
                }
            } else if let Ok(t) = vault::read(&root, &rel) {
                let r = vault::write(&root, &rel, &flip(&t));
                self.report(r);
            }
            self.tasks.dirty = true;
        }
        if let Some(rel) = open {
            self.view = Some(View::Files);
            self.open(rel);
        }
    }
}

// ---------- types ----------

#[derive(Default)]
pub struct TemplateDlg {
    pub name: String,
    pub sel: usize,
}

pub struct HistoryDlg {
    pub rel: String,
    pub versions: Vec<(String, PathBuf)>,
    pub sel: usize,
    pub diff: bool,
}

/// Différence ligne par ligne (plus longue sous-séquence commune) : -1 retiré, 0 identique, 1 ajouté.
pub fn diff_lines<'a>(old: &'a str, new: &'a str) -> Vec<(i8, &'a str)> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    // Préfixe et suffixe communs d'abord (rapide)
    let pre = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suf = a[pre..].iter().rev().zip(b[pre..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (am, bm) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
    let mut out: Vec<(i8, &str)> = a[..pre].iter().map(|l| (0, *l)).collect();
    if am.len() * bm.len() > 4_000_000 {
        out.extend(am.iter().map(|l| (-1, *l)));
        out.extend(bm.iter().map(|l| (1, *l)));
    } else {
        let (n, m) = (am.len(), bm.len());
        let mut t = vec![0u32; (n + 1) * (m + 1)];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                t[i * (m + 1) + j] = if am[i] == bm[j] { t[(i + 1) * (m + 1) + j + 1] + 1 } else { t[(i + 1) * (m + 1) + j].max(t[i * (m + 1) + j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n && j < m {
            if am[i] == bm[j] {
                out.push((0, am[i]));
                i += 1;
                j += 1;
            } else if t[(i + 1) * (m + 1) + j] >= t[i * (m + 1) + j + 1] {
                out.push((-1, am[i]));
                i += 1;
            } else {
                out.push((1, bm[j]));
                j += 1;
            }
        }
        out.extend(am[i..].iter().map(|l| (-1, *l)));
        out.extend(bm[j..].iter().map(|l| (1, *l)));
    }
    out.extend(a[a.len() - suf..].iter().map(|l| (0, *l)));
    out
}

pub struct Timer {
    pub start: Instant,
    pub minutes: u32,
    pub label: &'static str,
}

#[derive(Default)]
pub struct TasksState {
    pub items: Vec<Task>,
    pub loaded: Option<Instant>,
    pub dirty: bool,
    pub show_done: bool,
    pub filter: String,
    pub input: String,
}

pub struct Task {
    pub rel: String,
    pub line: usize,
    pub indent: usize,
    pub text: String,
    pub done: bool,
    /// Échéance « @AAAA-MM-JJ » écrite dans la tâche
    pub due: Option<String>,
}

fn collect_tasks(root: &str, entries: &[Entry], docs: &[Doc]) -> Vec<Task> {
    let mut out = vec![];
    for e in entries.iter().filter(|e| !e.is_dir && vault::is_md(&e.rel)).take(3000) {
        // Le texte ouvert (peut-être pas encore enregistré) a priorité.
        let text = docs.iter().find(|d| d.rel == e.rel).and_then(|d| if let Content::Text(t) = &d.content { Some(t.clone()) } else { None }).or_else(|| vault::read(root, &e.rel).ok());
        let Some(text) = text else { continue };
        if !text.contains("- [") && !text.contains("* [") {
            continue;
        }
        for (i, l) in text.split('\n').enumerate() {
            let t = l.trim_start();
            let indent = (l.len() - t.len()) / 2;
            let rest = t.strip_prefix("- ").or_else(|| t.strip_prefix("* "));
            let Some(rest) = rest else { continue };
            let (done, body) = if let Some(b) = rest.strip_prefix("[ ]") {
                (false, b)
            } else if let Some(b) = rest.strip_prefix("[x]").or_else(|| rest.strip_prefix("[X]")) {
                (true, b)
            } else {
                continue;
            };
            let body = body.trim();
            if body.is_empty() {
                continue;
            }
            let due = body.split_whitespace().find_map(|w| {
                let d = w.strip_prefix('@')?;
                (d.len() == 10 && d.as_bytes()[4] == b'-' && d.as_bytes()[7] == b'-' && d.chars().filter(|c| c.is_ascii_digit()).count() == 8).then(|| d.to_string())
            });
            out.push(Task { rel: e.rel.clone(), line: i, indent, text: body.to_string(), done, due });
        }
    }
    out
}

fn ensure_templates(root: &str) {
    let dir = std::path::Path::new(root).join("modèles");
    if dir.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(&dir);
    let files = [
        ("Fiche personnage.md", "# {{titre}}\n\n**Rôle :** \n**Âge :** \n**Silhouette :** \n\n## Personnalité\n- \n\n## Références visuelles\n- \n\n## Palette\n- \n\n## Notes de modélisation\n- [ ] blocking\n- [ ] sculpt\n- [ ] retopo\n- [ ] UV\n- [ ] textures\n- [ ] rig\n"),
        ("Plan de shot.md", "# {{titre}}\n\n| shot | durée | cadrage | action | notes |\n|---|---|---|---|---|\n| 010 |  |  |  |  |\n| 020 |  |  |  |  |\n\n## À faire\n- [ ] layout\n- [ ] animation\n- [ ] lumière\n- [ ] rendu\n- [ ] compo\n"),
        ("Réunion.md", "# {{titre}}\n\n**Date :** {{date}}\n**Avec :** \n\n## Points\n- \n\n## Décisions\n- \n\n## Suivi\n- [ ] \n"),
        ("Projet.md", "# {{titre}}\n\nCréé le {{date}}.\n\n## Intention\n\n## Références\n- \n\n## Étapes\n- [ ] \n\n## Liens\n- \n"),
        ("Critique.md", "# {{titre}}\n\n**Date :** {{date}}\n\n## Ce qui marche\n- \n\n## À corriger\n- [ ] \n\n## Idées\n- \n"),
    ];
    for (n, t) in files {
        let _ = crate::vault::atomic_write(dir.join(n), t);
    }
}

fn templates(entries: &[Entry], root: &str) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = entries.iter().filter(|e| !e.is_dir && e.rel.starts_with("modèles/") && vault::is_md(&e.rel)).map(|e| (e.name.trim_end_matches(".md").to_string(), e.rel.clone())).collect();
    if v.is_empty() {
        // Juste créés : on lit le dossier directement.
        if let Ok(rd) = std::fs::read_dir(std::path::Path::new(root).join("modèles")) {
            for f in rd.flatten() {
                let n = f.file_name().to_string_lossy().to_string();
                if n.ends_with(".md") {
                    v.push((n.trim_end_matches(".md").to_string(), format!("modèles/{n}")));
                }
            }
        }
    }
    v.sort();
    v
}

// ---------- historique ----------

fn history_dir(root: &str, rel: &str) -> PathBuf {
    let safe: String = rel.chars().map(|c| if c == '/' || c == '\\' || c == ':' { '_' } else { c }).collect();
    std::path::Path::new(root).join(".app/history").join(safe)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Garde une copie à chaque enregistrement (une par tranche de 2 minutes, 40 au plus).
pub fn snapshot(root: &str, rel: &str, text: &str) {
    if text.len() > 2_000_000 {
        return;
    }
    let dir = history_dir(root, rel);
    let _ = std::fs::create_dir_all(&dir);
    let mut files: Vec<(u64, PathBuf)> = std::fs::read_dir(&dir)
        .map(|rd| rd.flatten().filter_map(|f| f.file_name().to_string_lossy().trim_end_matches(".txt").parse::<u64>().ok().map(|t| (t, f.path()))).collect())
        .unwrap_or_default();
    files.sort();
    let now = unix_now();
    if let Some((t, f)) = files.last() {
        if std::fs::read_to_string(f).is_ok_and(|old| old == text) {
            return;
        }
        if now.saturating_sub(*t) < 120 {
            let _ = crate::vault::atomic_write(f, text);
            return;
        }
    }
    let _ = crate::vault::atomic_write(dir.join(format!("{now}.txt")), text);
    while files.len() >= 40 {
        let (_, f) = files.remove(0);
        let _ = std::fs::remove_file(f);
    }
}

pub fn history_list(root: &str, rel: &str) -> Vec<(String, PathBuf)> {
    let dir = history_dir(root, rel);
    let mut files: Vec<(u64, PathBuf)> = std::fs::read_dir(&dir)
        .map(|rd| rd.flatten().filter_map(|f| f.file_name().to_string_lossy().trim_end_matches(".txt").parse::<u64>().ok().map(|t| (t, f.path()))).collect())
        .unwrap_or_default();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files
        .into_iter()
        .map(|(t, f)| {
            let label = chrono::DateTime::from_timestamp(t as i64, 0).map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default();
            (label, f)
        })
        .collect()
}

fn line_diff(a: &str, b: &str) -> (usize, usize) {
    let sa: HashMap<&str, usize> = a.lines().fold(HashMap::new(), |mut m, l| {
        *m.entry(l).or_default() += 1;
        m
    });
    let sb: HashMap<&str, usize> = b.lines().fold(HashMap::new(), |mut m, l| {
        *m.entry(l).or_default() += 1;
        m
    });
    let added = sb.iter().map(|(l, n)| n.saturating_sub(*sa.get(l).unwrap_or(&0))).sum();
    let removed = sa.iter().map(|(l, n)| n.saturating_sub(*sb.get(l).unwrap_or(&0))).sum();
    (added, removed)
}

// ---------- sauvegarde ----------

pub fn backup_dir(prefs: &settings::Prefs) -> PathBuf {
    if !prefs.backup_dir.trim().is_empty() {
        return PathBuf::from(prefs.backup_dir.trim());
    }
    dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from(".")).join("Aryadne-sauvegardes")
}

/// Zip du coffre (sans la corbeille ni les fichiers de plus de 50 Mo), 7 derniers gardés.
fn make_backup(root: &str, dest: &std::path::Path) -> Result<PathBuf, String> {
    use std::io::Write;
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let name = std::path::Path::new(root).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "coffre".into());
    let out = dest.join(format!("{name}-{}.zip", chrono::Local::now().format("%Y-%m-%d_%H%M")));
    let mut z = zip::ZipWriter::new(std::fs::File::create(&out).map_err(|e| e.to_string())?);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
    let base = std::path::Path::new(root);
    let mut stack = vec![base.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for f in rd.flatten() {
            let path = f.path();
            let rel = path.strip_prefix(base).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
            if rel.starts_with(".app/trash") || rel.starts_with(".app/cache") || rel.starts_with(".git") {
                continue;
            }
            let Ok(meta) = f.metadata() else { continue };
            if meta.is_dir() {
                stack.push(path);
            } else if meta.len() <= 50 * 1024 * 1024 {
                if let Ok(bytes) = std::fs::read(&path) {
                    z.start_file(rel, opts).map_err(|e| e.to_string())?;
                    z.write_all(&bytes).map_err(|e| e.to_string())?;
                }
            }
        }
    }
    z.finish().map_err(|e| e.to_string())?;
    // On garde les 7 plus récentes de ce coffre.
    let mut olds: Vec<PathBuf> = std::fs::read_dir(dest).map(|rd| rd.flatten().map(|f| f.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&format!("{name}-")) && n.to_string_lossy().ends_with(".zip"))).collect()).unwrap_or_default();
    olds.sort();
    while olds.len() > 7 {
        let _ = std::fs::remove_file(olds.remove(0));
    }
    Ok(out)
}

// ---------- téléchargement ----------

fn download_image(root: &str, dir: &str, url: &str) -> Result<String, String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("L'adresse doit commencer par http:// ou https://".into());
    }
    let mut resp = crate::ollama::agent(30).get(url).call().map_err(|e| format!("Téléchargement impossible : {e}"))?;
    let bytes = resp.body_mut().with_config().limit(80 * 1024 * 1024).read_to_vec().map_err(|e| e.to_string())?;
    let fmt = image::guess_format(&bytes).map_err(|_| "Ce lien ne mène pas à une image.".to_string())?;
    let ext = fmt.extensions_str().first().copied().unwrap_or("png");
    let stem: String = url.split(['?', '#']).next().unwrap_or("").rsplit('/').next().unwrap_or("image").rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or_else(|| "image".into());
    let stem: String = stem.chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_').take(60).collect();
    let stem = if stem.is_empty() { "image".to_string() } else { stem };
    let mut rel = vault::join(dir, &format!("{stem}.{ext}"));
    let mut i = 2;
    while vault::safe(root, &rel).map(|p| p.exists()).unwrap_or(false) {
        rel = vault::join(dir, &format!("{stem}-{i}.{ext}"));
        i += 1;
    }
    let path = vault::safe(root, &rel)?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    crate::vault::atomic_write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(rel)
}

// ---------- tags ----------

/// Tags #mot des notes, avec leur nombre d'apparitions.
pub fn collect_tags(root: &str, entries: &[Entry]) -> Vec<(String, usize)> {
    let re = regex::Regex::new(r"(?:^|\s)#([\p{L}\p{N}_][\p{L}\p{N}_/-]*)").unwrap();
    let mut count: HashMap<String, usize> = HashMap::new();
    for e in entries.iter().filter(|e| !e.is_dir && vault::is_md(&e.rel)).take(3000) {
        let Ok(t) = vault::read(root, &e.rel) else { continue };
        for tag in vault::front_tags(&t) {
            *count.entry(tag.to_lowercase()).or_default() += 1;
        }
        let skip = vault::frontmatter(&t).map(|f| f.1).unwrap_or(0);
        let mut in_code = false;
        for l in t.lines().skip(skip) {
            if l.trim_start().starts_with("```") {
                in_code = !in_code;
            }
            if in_code {
                continue;
            }
            for c in re.captures_iter(l) {
                let tag = c[1].to_string();
                let hex = matches!(tag.len(), 3 | 6 | 8) && tag.chars().any(|c| c.is_ascii_digit()) && tag.chars().all(|c| c.is_ascii_hexdigit());
                if !hex {
                    *count.entry(tag.to_lowercase()).or_default() += 1;
                }
            }
        }
    }
    let mut v: Vec<(String, usize)> = count.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taches_historique_tags() {
        let dir = std::env::temp_dir().join(format!("aryadne-extras-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("notes")).unwrap();
        let root = dir.to_string_lossy().to_string();
        crate::vault::atomic_write(dir.join("notes/a.md"), "# A #perso\n- [ ] sculpter #3d\n  - [x] blocking\n- pas une tâche\ncouleur #ff5a1f\n").unwrap();
        let entries = vault::scan(&root);
        let t = collect_tasks(&root, &entries, &[]);
        assert_eq!(t.len(), 2);
        assert!(!t[0].done && t[1].done && t[1].indent == 1 && t[0].line == 1);
        let tags = collect_tags(&root, &entries);
        assert!(tags.iter().any(|(t, _)| t == "perso") && tags.iter().any(|(t, _)| t == "3d"));
        assert!(!tags.iter().any(|(t, _)| t == "ff5a1f"), "les couleurs ne sont pas des tags");
        snapshot(&root, "notes/a.md", "v1");
        snapshot(&root, "notes/a.md", "v2");
        let h = history_list(&root, "notes/a.md");
        assert_eq!(h.len(), 1, "deux enregistrements rapprochés = une seule version");
        assert_eq!(std::fs::read_to_string(&h[0].1).unwrap(), "v2");
        assert_eq!(line_diff("a\nb\nc", "a\nc\nd\ne"), (2, 1));
        assert_eq!(diff_lines("a\nb\nc", "a\nc\nd"), vec![(0, "a"), (-1, "b"), (0, "c"), (1, "d")]);
        let z = make_backup(&root, &dir.join("_sauv")).unwrap();
        assert!(z.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ---------- barre du chat : pièces jointes, modèle, arrêt ----------

/// Ce que la barre demande à l'application.
#[derive(Default)]
pub struct ChatBarOut {
    pub model: Option<(String, String)>,
    pub manage_models: bool,
    pub think: Option<bool>,
}

/// Colle l'image du presse-papiers comme pièce jointe ; faux s'il n'y en a pas.
pub fn paste_image(chat: &mut chat::Chat) -> bool {
    let Some(img) = arboard::Clipboard::new().ok().and_then(|mut c| c.get_image().ok()) else { return false };
    let Some(buf) = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned()) else { return false };
    let n = chat.attachments.iter().filter(|a| matches!(a, chat::Attachment::Image { .. })).count();
    let name = if n == 0 { "presse-papiers.png".to_string() } else { format!("presse-papiers {}.png", n + 1) };
    chat.attachments.push(chat::Attachment::from_image(image::DynamicImage::ImageRgba8(buf), &name));
    true
}

/// Texture d'une vignette JPEG en base64, gardée en cache pour la session.
pub fn thumb_texture(ctx: &egui::Context, b64: &str) -> Option<egui::TextureHandle> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b64.hash(&mut h);
    let id = egui::Id::unique(("chat-thumb", h.finish()));
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(t);
    }
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
    let t = ctx.load_texture(format!("chat-thumb-{}", h.finish()), ci, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, t.clone()));
    Some(t)
}

/// Pièces jointes (avec ×), « joindre ▾ », choix du modèle et bouton d'arrêt.
pub fn chat_bar(ui: &mut egui::Ui, chat: &mut chat::Chat, local: &ollama::Local, settings: &ai::Settings, open_file: Option<(String, PathBuf)>) -> ChatBarOut {
    let p = theme::pal();
    let mut out = ChatBarOut::default();
    ui.horizontal_wrapped(|ui| {
        egui::containers::menu::MenuButton::new(crate::i18n::t("＋ joindre ▾")).ui(ui, |ui| {
            ui.set_min_width(220.0);
            if ui.button(crate::i18n::t("fichiers ou images…")).clicked() {
                if let Some(files) = rfd::FileDialog::new().set_title("Joindre des fichiers").pick_files() {
                    for f in files {
                        chat.attachments.extend(chat::Attachment::from_path(&f));
                    }
                }
            }
            if ui.button(crate::i18n::t("un dossier…")).clicked() {
                if let Some(d) = rfd::FileDialog::new().set_title("Joindre un dossier").pick_folder() {
                    chat.attachments.extend(chat::Attachment::from_path(&d));
                }
            }
            if let Some((rel, path)) = &open_file {
                if ui.button(format!("le fichier ouvert ({})", vault::display_rel(rel))).clicked() {
                    chat.attachments.extend(chat::Attachment::from_path(path));
                }
            }
            if ui.button(format!("{}   Ctrl+V", crate::i18n::t("l'image du presse-papiers"))).clicked() {
                paste_image(chat);
            }
            ui.label(RichText::new(crate::i18n::t("Tu peux aussi glisser des fichiers dans la fenêtre (vue chat).")).color(p.muted).size(10.5));
        });
        let mut remove = None;
        for (i, a) in chat.attachments.iter().enumerate() {
            if let chat::Attachment::Image { thumb, name, .. } = a {
                // Aperçu : la vignette, l'image plus grande au survol, × pour retirer
                if let Some(tex) = thumb_texture(ui.ctx(), thumb) {
                    let sz = tex.size_vec2();
                    let h = 44.0;
                    let r = ui.add(egui::Image::new((tex.id(), egui::vec2(sz.x / sz.y * h, h))).corner_radius(4).sense(egui::Sense::click()));
                    let r = r.on_hover_ui(|ui| {
                        ui.add(egui::Image::new((tex.id(), sz)));
                        ui.label(RichText::new(format!("{name} · {}", crate::i18n::t("clic : retirer"))).size(10.5).color(p.muted));
                    });
                    if r.clicked() {
                        remove = Some(i);
                    }
                    continue;
                }
            }
            let icon = if matches!(a, chat::Attachment::Image { .. }) { "▣" } else { "▤" };
            if ui.add(egui::Button::new(RichText::new(format!("{icon} {}  ×", a.name())).size(11.0)).fill(p.panel)).on_hover_text(crate::i18n::t("retirer")).clicked() {
                remove = Some(i);
            }
        }
        if let Some(i) = remove {
            chat.attachments.remove(i);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(v) = chat.last_speed.filter(|_| !chat.busy()) {
                ui.label(RichText::new(format!("{v:.0} jetons/s")).color(p.muted).size(10.5)).on_hover_text(crate::i18n::t("vitesse de la dernière réponse"));
            }
            if chat.busy() {
                let b = egui::Button::new(RichText::new(crate::i18n::t("■ arrêter")).color(p.on_accent)).fill(p.missing).stroke(egui::Stroke::NONE);
                if ui.add(b).on_hover_text(crate::i18n::t("Arrêter la réponse")).clicked() {
                    chat.stop();
                }
            }
            // Choix du modèle (avec avertissement si le modèle dépasse la mémoire)
            let st = local.snapshot();
            if ollama::is_local(&settings.url) {
                let mut think = settings.think;
                if ui.toggle_value(&mut think, crate::i18n::t("réflexion")).on_hover_text(crate::i18n::t("Les modèles comme Qwen 3.x réfléchissent avant de répondre : plus juste, mais bien plus lent. Désactivé = réponse directe.")).changed() {
                    out.think = Some(think);
                }
                let size_gb = st.sizes.iter().find(|(m, _)| m.trim_end_matches(":latest") == settings.model.trim_end_matches(":latest")).map(|(_, b)| *b as f32 / 1e9);
                if let (Some(gb), Some(ram)) = (size_gb, ollama::total_ram_gb()) {
                    if gb * 1.2 > ram * 0.7 {
                        ui.label(RichText::new(crate::i18n::t("⚠ lent")).color(p.missing).size(10.5)).on_hover_text(format!("Ce modèle fait {gb:.0} Go pour {ram:.0} Go de mémoire : il tournera très lentement. Un modèle de 4 à 8 milliards de paramètres (3 à 6 Go) répond beaucoup plus vite."));
                    }
                }
            }
            let pid = settings.provider_id();
            let label = match ai::provider(pid) {
                Some(pr) if pid != "ollama" => format!("{} · {}", pr.name.split(' ').next().unwrap_or(pr.name), settings.model),
                _ => settings.model.clone(),
            };
            egui::containers::menu::MenuButton::new(RichText::new(format!("{label} ▾")).size(11.0)).ui(ui, |ui| {
                ui.set_min_width(260.0);
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for pr in settings.connected() {
                        let models: Vec<String> = if pr.id == "ollama" { st.models.clone() } else if pr.id == "lmstudio" { continue } else { ai::models_of(pr.id) };
                        if models.is_empty() {
                            continue;
                        }
                        ui.label(RichText::new(pr.name).color(p.muted).size(10.5));
                        for m in models {
                            let cur = pid == pr.id && (m == settings.model || m.trim_end_matches(":latest") == settings.model);
                            if ui.selectable_label(cur, format!("  {m}")).clicked() {
                                out.model = Some((pr.id.to_string(), m.clone()));
                                ui.close();
                            }
                        }
                        ui.add_space(4.0);
                    }
                });
                ui.separator();
                if ui.button(format!("{}  {}", crate::lucide::PLUS, crate::i18n::t("ajouter Claude, ChatGPT, DeepSeek…"))).clicked() {
                    out.manage_models = true;
                    ui.close();
                }
            });
        });
    });
    out
}

impl App {
    /// Écrit un fichier proposé par l'IA (en gardant l'ancienne version dans l'historique).
    pub(super) fn apply_ai_file(&mut self, rel: String, content: String) {
        let root = self.root();
        let rel = rel.trim().trim_start_matches("./").trim_start_matches('/').to_string();
        if let Ok(old) = vault::read(&root, &rel) {
            snapshot(&root, &rel, &old);
        }
        if let Some(parent) = vault::safe(&root, &rel).ok().and_then(|p| p.parent().map(|p| p.to_path_buf())) {
            let _ = std::fs::create_dir_all(parent);
        }
        match vault::write(&root, &rel, &content) {
            Ok(()) => {
                self.docs.retain(|d| d.rel != rel);
                self.active = if self.docs.is_empty() { None } else { Some(self.active.unwrap_or(0).min(self.docs.len() - 1)) };
                self.refresh();
                self.open(rel);
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub(super) fn apply_chat_bar(&mut self, out: ChatBarOut, ctx: &egui::Context) {
        if let Some((pid, m)) = out.model {
            let mut s = self.ai.clone().unwrap_or_else(ai::Settings::load);
            s.use_model(&pid, &m);
            s.save();
            self.ai = Some(s);
        }
        if let Some(t) = out.think {
            let mut s = self.ai.clone().unwrap_or_else(ai::Settings::load);
            s.think = t;
            s.save();
            self.ai = Some(s);
        }
        if out.manage_models {
            self.open_settings_tab(ctx, settings::Tab::Ai);
        }
    }

    pub(super) fn open_file_path(&self) -> Option<(String, PathBuf)> {
        let d = self.current()?;
        vault::safe(&self.root(), &d.rel).ok().map(|p| (d.rel.clone(), p))
    }
}

/// Boutons de réponse rapide quand l'IA pose une question (« ?> a | b »).
/// Fichiers proposés par l'IA (blocs ```fichier:chemin) : bouton pour les appliquer.
pub fn file_buttons(ui: &mut egui::Ui, text: &str, exists: &dyn Fn(&str) -> bool) -> Option<(String, String)> {
    let p = theme::pal();
    let mut picked = None;
    for (path, content) in chat::file_blocks(text) {
        ui.horizontal_wrapped(|ui| {
            let verb = if exists(&path) { "remplacer" } else { "créer" };
            ui.label(RichText::new(format!("▤ {path}")).color(p.muted).size(11.0));
            if ui.add(egui::Button::new(RichText::new(format!("{verb} le fichier")).color(p.on_accent)).fill(p.accent).stroke(egui::Stroke::NONE)).on_hover_text(crate::i18n::t("L'ancienne version reste dans l'historique du fichier")).clicked() {
                picked = Some((path.clone(), content.clone()));
            }
        });
    }
    picked
}

pub fn choice_buttons(ui: &mut egui::Ui, choices: &[String]) -> Option<String> {
    let p = theme::pal();
    let mut picked = None;
    if choices.is_empty() {
        return None;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(crate::i18n::t("répondre :")).color(p.muted).size(11.0));
        for c in choices {
            if ui.add(egui::Button::new(RichText::new(c).color(p.accent)).stroke(egui::Stroke::new(1.0, p.accent))).clicked() {
                picked = Some(c.clone());
            }
        }
    });
    picked
}


// ---------- statistiques du coffre ----------

pub struct Stats {
    pub files: usize,
    pub dirs: usize,
    pub total: u64,
    pub by_kind: Vec<(String, usize, u64)>,
    pub biggest: Vec<(String, u64)>,
    pub words: usize,
    pub linked: usize,
}

fn kind_of(rel: &str) -> &'static str {
    // Les formats d'Aryadne d'abord, anciennes extensions comprises
    match crate::formats::of(rel).map(|f| f.id) {
        Some("board") => return "boards",
        Some("mat") | Some("comp") => return "nodes",
        Some("scene") => return "3d",
        Some("seq") => return "vidéos",
        Some("image") => return "images",
        _ => {}
    }
    let e = rel.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    match e.as_str() {
        "md" | "txt" => "notes",
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tga" | "tif" | "tiff" | "exr" | "hdr" | "psd" | "kra" => "images",
        "obj" | "fbx" | "glb" | "gltf" | "stl" | "3ds" | "ply" | "ma" | "mb" | "max" | "blend" | "abc" | "usd" | "usdz" => "3d",
        "mp4" | "mov" | "webm" | "mkv" | "avi" => "vidéos",
        "pptx" | "docx" | "xlsx" | "odp" | "odt" | "ods" | "pdf" => "documents",
        "rs" | "py" | "js" | "ts" | "html" | "css" | "c" | "cpp" | "h" | "glsl" | "lua" | "sh" | "json" | "toml" | "yaml" | "yml" | "gd" => "code",
        "wav" | "mp3" | "ogg" | "flac" => "audio",
        _ => "autres",
    }
}

fn compute_stats(root: &str, entries: &[Entry]) -> Stats {
    let mut by: HashMap<&str, (usize, u64)> = HashMap::new();
    let mut biggest = vec![];
    let (mut files, mut dirs, mut total, mut words, mut linked) = (0, 0, 0u64, 0usize, 0);
    for e in entries {
        if e.is_dir {
            dirs += 1;
            continue;
        }
        files += 1;
        if vault::is_external(&e.rel) {
            linked += 1;
        }
        let size = vault::safe(root, &e.rel).ok().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()).unwrap_or(0);
        total += size;
        let k = by.entry(kind_of(&e.rel)).or_default();
        k.0 += 1;
        k.1 += size;
        biggest.push((e.rel.clone(), size));
        if vault::is_md(&e.rel) && size < 2_000_000 {
            words += vault::read(root, &e.rel).map(|t| t.split_whitespace().count()).unwrap_or(0);
        }
    }
    biggest.sort_by(|a, b| b.1.cmp(&a.1));
    biggest.truncate(12);
    let mut by_kind: Vec<(String, usize, u64)> = by.into_iter().map(|(k, (n, s))| (k.to_string(), n, s)).collect();
    by_kind.sort_by(|a, b| b.2.cmp(&a.2));
    Stats { files, dirs, total, by_kind, biggest, words, linked }
}

pub fn human(n: u64) -> String {
    let f = n as f64;
    if f >= 1e9 {
        format!("{:.1} Go", f / 1e9)
    } else if f >= 1e6 {
        format!("{:.1} Mo", f / 1e6)
    } else if f >= 1e3 {
        format!("{:.0} Ko", f / 1e3)
    } else {
        format!("{n} o")
    }
}

impl App {
    pub(super) fn stats_ui(&mut self, ctx: &egui::Context) {
        let Some(st) = &self.stats else { return };
        let p = theme::pal();
        let mut close = false;
        let mut open = None;
        egui::Modal::new(egui::Id::unique("stats")).frame(egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.text)).inner_margin(16)).show(ctx, |ui| {
            ui.set_width(620.0);
            ui.label(caps(&crate::i18n::t("statistiques du coffre")));
            ui.add_space(6.0);
            ui.label(format!("{} fichiers · {} dossiers · {} au total · {} fichiers liés · {} mots dans les notes", st.files, st.dirs, human(st.total), st.linked, st.words));
            ui.add_space(10.0);
            let max = st.by_kind.iter().map(|k| k.2).max().unwrap_or(1).max(1);
            for (k, n, s) in &st.by_kind {
                ui.horizontal(|ui| {
                    ui.add_sized([90.0, 18.0], egui::Label::new(RichText::new(k.as_str()).size(12.0)));
                    let (r, _) = ui.allocate_exact_size(egui::vec2(280.0, 12.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 0.0, p.line);
                    ui.painter().rect_filled(egui::Rect::from_min_size(r.min, egui::vec2(280.0 * *s as f32 / max as f32, 12.0)), 0.0, p.accent);
                    ui.label(RichText::new(format!("{n} · {}", human(*s))).color(p.muted).size(11.0));
                });
            }
            ui.add_space(10.0);
            ui.label(caps(&crate::i18n::t("les plus lourds")));
            for (rel, s) in &st.biggest {
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new(vault::display_rel(rel)).size(11.5)).frame(false)).clicked() {
                        open = Some(rel.clone());
                    }
                    ui.label(RichText::new(human(*s)).color(p.muted).size(11.0));
                });
            }
            ui.add_space(8.0);
            close = ui.button(format!("{}  {}", crate::lucide::X, crate::i18n::t("fermer"))).clicked() || ui.input(|i| i.key_pressed(Key::Escape));
        });
        if let Some(r) = open {
            self.stats = None;
            self.open(r);
            return;
        }
        if close {
            self.stats = None;
        }
    }

    // ---------- exécuter un script ----------

    pub(super) fn run_current(&mut self) {
        let Some(rel) = self.current().map(|d| d.rel.clone()) else { return };
        let Some((prog, args)) = runner_for(&rel) else {
            self.error = Some("Pas de façon connue d'exécuter ce type de fichier.".into());
            return;
        };
        self.run_with(prog, args);
    }

    /// Lance le fichier ouvert avec `prog args… fichier`.
    pub(super) fn run_with(&mut self, prog: String, args: Vec<String>) {
        let Some(rel) = self.current().map(|d| d.rel.clone()) else { return };
        self.flush_all();
        let Ok(path) = vault::safe(&self.root(), &rel) else { return };
        let out = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut cmd = std::process::Command::new(&prog);
        cmd.args(&args).arg(&path).current_dir(path.parent().unwrap_or(std::path::Path::new("."))).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).stdin(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        match cmd.spawn() {
            Ok(mut child) => {
                let (o, d) = (out.clone(), done.clone());
                let ctx = self.ctx.clone();
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();
                let child = std::sync::Arc::new(std::sync::Mutex::new(child));
                let ch = child.clone();
                std::thread::spawn(move || {
                    use std::io::BufRead;
                    let o2 = o.clone();
                    let c2 = ctx.clone();
                    let t = std::thread::spawn(move || {
                        if let Some(e) = stderr {
                            for l in std::io::BufReader::new(e).lines().map_while(Result::ok) {
                                if let Ok(mut s) = o2.lock() {
                                    s.push_str(&l);
                                    s.push('\n');
                                }
                                if let Some(c) = &c2 {
                                    c.request_repaint();
                                }
                            }
                        }
                    });
                    if let Some(so) = stdout {
                        for l in std::io::BufReader::new(so).lines().map_while(Result::ok) {
                            if let Ok(mut s) = o.lock() {
                                s.push_str(&l);
                                s.push('\n');
                            }
                            if let Some(c) = &ctx {
                                c.request_repaint();
                            }
                        }
                    }
                    let _ = t.join();
                    let code = ch.lock().ok().and_then(|mut c| c.wait().ok()).and_then(|s| s.code());
                    if let Ok(mut s) = o.lock() {
                        s.push_str(&format!("\n— terminé (code {}) —\n", code.map(|c| c.to_string()).unwrap_or("?".into())));
                    }
                    d.store(true, std::sync::atomic::Ordering::Relaxed);
                    if let Some(c) = &ctx {
                        c.request_repaint();
                    }
                });
                self.run = Some(Run { rel, out, done, child: Some(child) });
            }
            Err(e) => self.error = Some(format!("Impossible de lancer « {prog} » : {e}. Est-il installé ?")),
        }
    }

    /// Panneau de sortie sous l'éditeur.
    pub(super) fn run_panel(&mut self, ui: &mut egui::Ui) {
        let Some(r) = &self.run else { return };
        let p = theme::pal();
        let mut close = false;
        let mut stop = false;
        let mut again = false;
        let text = r.out.lock().map(|s| s.clone()).unwrap_or_default();
        let running = !r.done.load(std::sync::atomic::Ordering::Relaxed);
        egui::Panel::bottom("run-output").resizable(true).default_size(180.0).min_size(60.0).max_size((ui.available_height() * 0.7).max(80.0)).frame(egui::Frame::new().fill(egui::Color32::from_gray(22)).inner_margin(8)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("sortie · {}", vault::display_rel(&r.rel))).color(egui::Color32::from_gray(170)).size(11.0));
                if running {
                    ui.add(egui::Spinner::new().size(10.0));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = ui.small_button("×").on_hover_text(crate::i18n::t("fermer")).clicked();
                    if running {
                        stop = ui.add(egui::Button::new(egui::RichText::new(crate::lucide::SQUARE).size(13.0)).min_size(egui::vec2(24.0, 20.0))).on_hover_text(crate::i18n::t("arrêter")).clicked();
                    } else {
                        again = ui.small_button(crate::i18n::t("relancer")).clicked();
                    }
                });
            });
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink(false).show(ui, |ui| {
                ui.label(RichText::new(if text.is_empty() { "…" } else { text.as_str() }).monospace().size(12.0).color(egui::Color32::from_gray(225)));
            });
        });
        let _ = p;
        if stop {
            if let Some(c) = r.child.as_ref() {
                if let Ok(mut c) = c.lock() {
                    let _ = c.kill();
                }
            }
        }
        if close {
            if running {
                if let Some(c) = r.child.as_ref() {
                    if let Ok(mut c) = c.lock() {
                        let _ = c.kill();
                    }
                }
            }
            self.run = None;
        }
        if again {
            self.run_current();
        }
    }
}

pub struct Run {
    pub rel: String,
    pub out: std::sync::Arc<std::sync::Mutex<String>>,
    pub done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub child: Option<std::sync::Arc<std::sync::Mutex<std::process::Child>>>,
}

/// Programme qui exécute ce fichier (et ses arguments avant le chemin).
pub fn runner_for(rel: &str) -> Option<(String, Vec<String>)> {
    let e = rel.rsplit_once('.').map(|(_, e)| e.to_lowercase())?;
    let py = if cfg!(windows) { "python" } else { "python3" };
    Some(match e.as_str() {
        "py" => (py.into(), vec!["-u".into()]),
        "js" | "mjs" => ("node".into(), vec![]),
        "sh" => ("bash".into(), vec![]),
        "lua" => ("lua".into(), vec![]),
        "rb" => ("ruby".into(), vec![]),
        "ps1" => ("powershell".into(), vec!["-NoProfile".into(), "-File".into()]),
        "bat" | "cmd" => ("cmd".into(), vec!["/C".into()]),
        _ => return None,
    })
}

// ---------- code : formatage, mots du projet ----------

impl App {
    pub(super) fn format_current(&mut self) {
        let Some(i) = self.active else { return };
        let rel = self.docs[i].rel.clone();
        let lang = ext(&rel);
        let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
        let Content::Text(t) = &self.docs[i].content else { return };
        match code::format(t, &lang, &name) {
            Ok(new) => {
                if new != *t {
                    if let Content::Text(t) = &mut self.docs[i].content {
                        *t = new;
                    }
                    self.docs[i].dirty_since = Some(Instant::now());
                    self.toast = Some(("fichier formaté (Ctrl+Z pour annuler)".into(), Instant::now()));
                } else {
                    self.toast = Some(("déjà bien formaté".into(), Instant::now()));
                }
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Mots des autres fichiers du même langage, pour les suggestions (rafraîchi toutes les 30 s).
    pub(super) fn feed_project_words(&mut self) {
        let Some(i) = self.active else { return };
        if !matches!(self.docs[i].content, Content::Text(_)) {
            return;
        }
        let lang = ext(&self.docs[i].rel);
        if lang.is_empty() || lang == "md" || lang == "txt" {
            return;
        }
        let fresh = self.proj_words.get(&lang).is_some_and(|(t, _)| t.elapsed() < Duration::from_secs(30));
        if !fresh {
            let root = self.root();
            let me = self.docs[i].rel.clone();
            let mut words: HashSet<String> = HashSet::new();
            let re = regex::Regex::new(r"[A-Za-z_][A-Za-z0-9_]{3,40}").unwrap();
            for e in self.entries.iter().filter(|e| !e.is_dir && e.rel != me && ext(&e.rel) == lang).take(200) {
                if let Ok(t) = vault::read(&root, &e.rel) {
                    for m in re.find_iter(&t[..t.len().min(300_000)]) {
                        if words.len() > 8000 {
                            break;
                        }
                        words.insert(m.as_str().to_string());
                    }
                }
            }
            let mut v: Vec<String> = words.into_iter().collect();
            v.sort();
            self.proj_words.insert(lang.clone(), (Instant::now(), std::sync::Arc::new(v)));
        }
        if let Some((_, w)) = self.proj_words.get(&lang) {
            self.docs[i].code.project = w.clone();
        }
    }

    pub(super) fn toggle_terminal(&mut self) {
        if self.terminal.is_some() {
            self.terminal = None;
        } else {
            let cwd = self.current().and_then(|d| vault::safe(&self.root(), &vault::parent(&d.rel)).ok()).unwrap_or_else(|| PathBuf::from(self.root()));
            self.terminal = Some(Terminal::new(cwd));
        }
    }

    pub(super) fn terminal_panel(&mut self, ui: &mut egui::Ui) {
        let Some(t) = &mut self.terminal else { return };
        let mut close = false;
        let bg = egui::Color32::from_gray(20);
        if crate::dock::closed("terminal") {
            self.terminal = None;
            return;
        }
        crate::dock::show_ex(ui, "terminal", "terminal", crate::dock::Side::Bottom, 420.0, 200.0, egui::Frame::new().fill(bg).inner_margin(8), true, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("terminal · {}", t.cwd.display())).color(egui::Color32::from_gray(160)).size(11.0));
                if t.busy() {
                    ui.add(egui::Spinner::new().size(10.0));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = ui.small_button("×").on_hover_text(crate::i18n::t("Ctrl+`")).clicked();
                    if ui.small_button(crate::i18n::t("effacer")).clicked() {
                        if let Ok(mut o) = t.out.lock() {
                            o.clear();
                        }
                    }
                    if t.busy() && ui.small_button(crate::i18n::t("arrêter (Ctrl+C)")).clicked() {
                        t.kill();
                    }
                });
            });
            let text = t.out.lock().map(|o| o.clone()).unwrap_or_default();
            egui::Panel::bottom("terminal-input").frame(egui::Frame::NONE).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("❯").color(theme::pal().accent).monospace());
                    let r = ui.add(egui::TextEdit::singleline(&mut t.input).font(egui::FontId::monospace(12.5)).text_color(egui::Color32::from_gray(235)).frame(egui::Frame::NONE).desired_width(f32::INFINITY).hint_text(crate::i18n::t("commande… (↑ ↓ historique)")));
                    if t.focus {
                        r.request_focus();
                        t.focus = false;
                    }
                    if r.has_focus() {
                        let (up, down, ctrl_c) = ui.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.modifiers.command && i.key_pressed(Key::C) && !i.events.iter().any(|e| matches!(e, egui::Event::Copy))));
                        if up {
                            t.history_step(true);
                        }
                        if down {
                            t.history_step(false);
                        }
                        if ctrl_c && t.busy() {
                            t.kill();
                        }
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        let cmd = std::mem::take(&mut t.input);
                        t.run(cmd, ui.ctx());
                        t.focus = true;
                    }
                });
            });
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink(false).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(text).monospace().size(12.0).color(egui::Color32::from_gray(225))).wrap_mode(egui::TextWrapMode::Wrap));
            });
        });
        if close {
            if let Some(t) = &mut self.terminal {
                t.kill();
            }
            self.terminal = None;
        }
    }
}

/// Terminal simple : chaque ligne est lancée dans le shell du système, dans le dossier courant.
pub struct Terminal {
    pub cwd: PathBuf,
    pub out: std::sync::Arc<std::sync::Mutex<String>>,
    pub input: String,
    history: Vec<String>,
    hist: Option<usize>,
    child: Option<std::sync::Arc<std::sync::Mutex<std::process::Child>>>,
    done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    focus: bool,
}

impl Terminal {
    pub fn new(cwd: PathBuf) -> Self {
        let shell = if cfg!(windows) { "cmd" } else { "bash" };
        Terminal {
            out: std::sync::Arc::new(std::sync::Mutex::new(format!("Terminal ({shell}) dans {}\ncd, clear, et ↑ ↓ pour l'historique. Les programmes interactifs (vim, python sans script) ne sont pas pris en charge.\n\n", cwd.display()))),
            cwd,
            input: String::new(),
            history: vec![],
            hist: None,
            child: None,
            done: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            focus: true,
        }
    }

    fn busy(&self) -> bool {
        !self.done.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn push(&self, s: &str) {
        if let Ok(mut o) = self.out.lock() {
            o.push_str(s);
            if o.len() > 400_000 {
                let cut = o.len() - 300_000;
                let cut = (cut..o.len()).find(|i| o.is_char_boundary(*i)).unwrap_or(0);
                o.drain(..cut);
            }
        }
    }

    fn history_step(&mut self, up: bool) {
        if self.history.is_empty() {
            return;
        }
        let n = self.history.len();
        let i = match (self.hist, up) {
            (None, true) => n - 1,
            (None, false) => return,
            (Some(i), true) => i.saturating_sub(1),
            (Some(i), false) => {
                if i + 1 >= n {
                    self.hist = None;
                    self.input.clear();
                    return;
                }
                i + 1
            }
        };
        self.hist = Some(i);
        self.input = self.history[i].clone();
    }

    fn kill(&mut self) {
        if let Some(c) = &self.child {
            if let Ok(mut c) = c.lock() {
                let _ = c.kill();
            }
        }
    }

    fn run(&mut self, cmd: String, ctx: &egui::Context) {
        let cmd = cmd.trim().to_string();
        if cmd.is_empty() {
            return;
        }
        self.history.retain(|h| *h != cmd);
        self.history.push(cmd.clone());
        self.hist = None;
        self.push(&format!("❯ {cmd}\n"));
        if cmd == "clear" || cmd == "cls" {
            if let Ok(mut o) = self.out.lock() {
                o.clear();
            }
            return;
        }
        if let Some(dir) = cmd.strip_prefix("cd").filter(|r| r.is_empty() || r.starts_with(' ')) {
            let dir = dir.trim().trim_matches('"');
            let target = if dir.is_empty() || dir == "~" { dirs::home_dir().unwrap_or(self.cwd.clone()) } else { self.cwd.join(dir) };
            match target.canonicalize() {
                Ok(p) if p.is_dir() => self.cwd = p,
                _ => self.push(&format!("cd : dossier introuvable : {dir}\n")),
            }
            return;
        }
        if self.busy() {
            self.push("(une commande tourne déjà : arrête-la d'abord)\n");
            return;
        }
        let mut c = if cfg!(windows) {
            let mut c = std::process::Command::new("cmd");
            c.args(["/C", &cmd]);
            c
        } else {
            let mut c = std::process::Command::new("bash");
            c.args(["-c", &cmd]);
            c
        };
        c.current_dir(&self.cwd).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).env("TERM", "dumb").env("NO_COLOR", "1");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        match c.spawn() {
            Ok(mut child) => {
                self.done.store(false, std::sync::atomic::Ordering::Relaxed);
                let (so, se) = (child.stdout.take(), child.stderr.take());
                let child = std::sync::Arc::new(std::sync::Mutex::new(child));
                self.child = Some(child.clone());
                let (out, done, ctx) = (self.out.clone(), self.done.clone(), ctx.clone());
                std::thread::spawn(move || {
                    use std::io::Read;
                    let pump = |mut r: Box<dyn Read + Send>, out: std::sync::Arc<std::sync::Mutex<String>>, ctx: egui::Context| {
                        let mut buf = [0u8; 4096];
                        while let Ok(n) = r.read(&mut buf) {
                            if n == 0 {
                                break;
                            }
                            if let Ok(mut o) = out.lock() {
                                o.push_str(&String::from_utf8_lossy(&buf[..n]));
                            }
                            ctx.request_repaint();
                        }
                    };
                    let (o2, c2) = (out.clone(), ctx.clone());
                    let t = se.map(|e| std::thread::spawn(move || pump(Box::new(e), o2, c2)));
                    if let Some(s) = so {
                        pump(Box::new(s), out.clone(), ctx.clone());
                    }
                    if let Some(t) = t {
                        let _ = t.join();
                    }
                    let code = child.lock().ok().and_then(|mut c| c.wait().ok()).and_then(|s| s.code());
                    if let Ok(mut o) = out.lock() {
                        if code.is_some_and(|c| c != 0) {
                            o.push_str(&format!("(code {})\n", code.unwrap()));
                        }
                    }
                    done.store(true, std::sync::atomic::Ordering::Relaxed);
                    ctx.request_repaint();
                });
            }
            Err(e) => self.push(&format!("impossible de lancer : {e}\n")),
        }
    }
}


/// Cherche Blender : PATH, puis les emplacements d'installation habituels.
pub fn find_blender() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "blender.exe" } else { "blender" };
    if let Some(paths) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&paths) {
            let p = d.join(exe);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let mut roots: Vec<PathBuf> = vec![];
    if cfg!(windows) {
        roots.push(PathBuf::from(r"C:\Program Files\Blender Foundation"));
        if let Some(l) = dirs::data_local_dir() {
            roots.push(l.join("Programs").join("Blender Foundation"));
        }
        if let Some(h) = dirs::home_dir() {
            roots.push(h.join("scoop").join("apps").join("blender").join("current"));
        }
    } else {
        for p in ["/snap/bin/blender", "/usr/bin/blender", "/usr/local/bin/blender", "/var/lib/flatpak/exports/bin/org.blender.Blender"] {
            if std::path::Path::new(p).exists() {
                return Some(PathBuf::from(p));
            }
        }
        if let Some(h) = dirs::home_dir() {
            roots.push(h.clone());
            roots.push(h.join("Applications"));
            roots.push(h.join(".local/share/flatpak/exports/bin"));
        }
        roots.push(PathBuf::from("/opt"));
    }
    // Dossiers « blender* » (versions portables), le plus récent d'abord
    let mut found: Vec<PathBuf> = vec![];
    for r in roots {
        if r.join(exe).is_file() {
            found.push(r.join(exe));
        }
        if let Ok(rd) = std::fs::read_dir(&r) {
            for f in rd.flatten() {
                let n = f.file_name().to_string_lossy().to_lowercase();
                if n.starts_with("blender") && f.path().join(exe).is_file() {
                    found.push(f.path().join(exe));
                }
            }
        }
    }
    found.sort();
    found.pop()
}

#[cfg(test)]
mod palette_tests {
    #[test]
    fn recherche_floue() {
        use super::fuzzy;
        assert!(fuzzy("nvscn", "nouvelle scene 3d").is_some());
        assert!(fuzzy("xyz", "nouvelle scene").is_none());
        // Texte trouvé tel quel et débuts de mots passent devant
        assert!(fuzzy("scene", "nouvelle scene 3d").unwrap() > fuzzy("scene", "sauvegarder le coffre maintenant").unwrap_or(-99));
        assert!(fuzzy("ns", "nouvelle scene").unwrap() > fuzzy("ns", "transfert").unwrap_or(-99));
    }
}
