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
    OpenVaultFolder,
    ChangeVault,
    MenuEntry,
    Updates,
    Timer(u32),
    StopTimer,
    Organize,
    Reload,
}

impl App {
    fn commands(&self) -> Vec<(&'static str, &'static str, Cmd)> {
        use settings::Tab;
        let mut v = vec![
            ("aller : fichiers", "Ctrl+1", Cmd::View(View::Files)),
            ("aller : cerveau", "Ctrl+2", Cmd::View(View::Brain)),
            ("aller : refs", "Ctrl+3", Cmd::View(View::Refs)),
            ("aller : boards", "Ctrl+4", Cmd::View(View::Boards)),
            ("aller : tâches", "Ctrl+5", Cmd::View(View::Tasks)),
            ("aller : chat", "Ctrl+6", Cmd::View(View::Chat)),
            ("aller : recherche", "Ctrl+Maj+F", Cmd::View(View::Search)),
            ("nouvelle note", "Ctrl+N", Cmd::NewNote),
            ("note du jour", "Ctrl+J", Cmd::Daily),
            ("nouvelle note depuis un modèle", "", Cmd::FromTemplate),
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
            Cmd::Assistant => self.assistant_open = !self.assistant_open,
            Cmd::CloseTab => {
                if let Some(i) = self.active {
                    self.actions.push(Action::Close(i));
                }
            }
            Cmd::History => {
                if let Some(rel) = self.current().map(|d| d.rel.clone()) {
                    self.flush_all();
                    self.history_dlg = Some(HistoryDlg { versions: history_list(&self.root(), &rel), rel, sel: 0 });
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
            Cmd::Organize => {
                self.view = Some(View::Brain);
                self.start_organize(ctx);
            }
        }
    }

    /// Ctrl+Maj+P : toutes les actions de l'application, filtrables au clavier.
    pub(super) fn palette_ui(&mut self, ctx: &egui::Context) {
        let p = theme::pal();
        let Some((mut query, mut sel)) = self.palette.take() else { return };
        let q = crate::chat::fold(&query);
        let all = self.commands();
        let words: Vec<&str> = q.split_whitespace().collect();
        let matches: Vec<(&str, &str, Cmd)> = all.into_iter().filter(|(l, _, _)| {
            let f = crate::chat::fold(l);
            words.iter().all(|w| f.contains(w))
        }).collect();
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
                ui.add(egui::TextEdit::singleline(&mut query).hint_text("chercher une commande…").frame(egui::Frame::NONE).desired_width(f32::INFINITY)).request_focus();
            });
            ui.separator();
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                for (i, (label, keys, cmd)) in matches.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), egui::Sense::click());
                    if i == sel {
                        ui.painter().rect_filled(r, 0.0, p.accent.gamma_multiply(0.15));
                        if down || up {
                            ui.scroll_to_rect(r, None);
                        }
                    }
                    ui.painter().text(r.left_center() + egui::vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, *label, egui::FontId::monospace(12.5), if i == sel { p.text } else { p.muted });
                    ui.painter().text(r.right_center() - egui::vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, *keys, egui::FontId::monospace(11.0), p.muted);
                    if resp.clicked() {
                        chosen = Some(*cmd);
                    }
                }
                if matches.is_empty() {
                    ui.label(RichText::new("aucune commande").color(p.muted));
                }
            });
        });
        if enter {
            chosen = matches.get(sel).map(|m| m.2);
        }
        if let Some(c) = chosen {
            self.run_cmd(c, ctx);
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
            ui.label(caps("nouvelle note depuis un modèle"));
            ui.add_space(6.0);
            for (i, (name, _)) in list.iter().enumerate() {
                ui.radio_value(&mut d.sel, i, name.as_str());
            }
            ui.label(RichText::new("Les modèles sont des notes du dossier modèles/ : modifie-les ou ajoutes-en. {{titre}}, {{date}} et {{heure}} sont remplacés.").color(p.muted).size(11.0));
            ui.add_space(8.0);
            let r = ui.add(egui::TextEdit::singleline(&mut d.name).hint_text("titre de la note").desired_width(f32::INFINITY));
            if !r.has_focus() && d.name.is_empty() {
                r.request_focus();
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                create = true;
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                create |= ui.button("créer").clicked();
                close = ui.button("annuler").clicked() || ui.input(|i| i.key_pressed(Key::Escape));
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
                ui.label(RichText::new("Aucune version enregistrée pour l'instant. Aryadne en garde une à chaque enregistrement (au plus une toutes les 2 minutes, les 40 dernières).").color(p.muted));
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
                        ui.label(RichText::new(format!("par rapport à maintenant : +{added} / −{removed} lignes")).color(p.muted).size(11.0));
                        egui::ScrollArea::both().id_salt("hist-view").max_height(400.0).show(ui, |ui| {
                            ui.add(egui::Label::new(RichText::new(&text).monospace().size(12.0)).wrap_mode(egui::TextWrapMode::Extend));
                        });
                        if ui.button("restaurer cette version").clicked() {
                            restore = Some(text);
                        }
                    });
                });
            }
            ui.add_space(8.0);
            close = ui.button("fermer").clicked() || ui.input(|i| i.key_pressed(Key::Escape));
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
                if ui.add(egui::Button::new(RichText::new(txt).color(p.accent).size(11.0)).frame(false)).on_hover_text("cliquer pour arrêter").clicked() {
                    self.timer = None;
                }
                ui.ctx().request_repaint_after(Duration::from_secs(1));
            }
            None => {
                let r = ui.add(egui::Button::new(RichText::new("◷ minuteur").color(p.muted).size(11.0)).frame(false));
                egui::Popup::menu(&r).show(|ui| {
                    for (m, l) in [(25, "25 min de concentration"), (50, "50 min"), (5, "pause 5 min"), (15, "pause 15 min")] {
                        if ui.button(l).clicked() {
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
            ui.label(caps("importer une image depuis une URL"));
            ui.add_space(6.0);
            let r = ui.add(egui::TextEdit::singleline(&mut url).hint_text("https://…/image.jpg").desired_width(f32::INFINITY));
            r.request_focus();
            go = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            ui.label(RichText::new("L'image est enregistrée dans refs/ (Ctrl+V d'une URL dans les refs fait la même chose).").color(p.muted).size(11.0));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                go |= ui.button("importer").clicked();
                close = ui.button("annuler").clicked() || ui.input(|i| i.key_pressed(Key::Escape));
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
                ui.label(caps("tâches"));
                let open_n = self.tasks.items.iter().filter(|t| !t.done).count();
                ui.label(RichText::new(format!("{open_n} à faire · {} faites", self.tasks.items.len() - open_n)).color(p.muted).size(11.5));
                ui.add_space(12.0);
                ui.selectable_value(&mut self.tasks.show_done, false, "à faire");
                ui.selectable_value(&mut self.tasks.show_done, true, "tout");
                ui.add_space(12.0);
                ui.add(egui::TextEdit::singleline(&mut self.tasks.filter).hint_text("filtrer…").desired_width(180.0));
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut self.tasks.input).hint_text("nouvelle tâche (ajoutée à tâches.md) · Entrée").desired_width(420.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    add = true;
                }
                add |= ui.button("ajouter").clicked();
            });
            ui.add_space(4.0);
            ui.label(RichText::new("Toutes les cases « - [ ] » de tes notes, regroupées par note. Cocher ici coche dans la note.").color(p.muted).size(11.0));
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
            out.push(Task { rel: e.rel.clone(), line: i, indent, text: body.to_string(), done });
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
        let _ = std::fs::write(dir.join(n), t);
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
            let _ = std::fs::write(f, text);
            return;
        }
    }
    let _ = std::fs::write(dir.join(format!("{now}.txt")), text);
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
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(rel)
}

// ---------- tags ----------

/// Tags #mot des notes, avec leur nombre d'apparitions.
pub fn collect_tags(root: &str, entries: &[Entry]) -> Vec<(String, usize)> {
    let re = regex::Regex::new(r"(?:^|\s)#([\p{L}\p{N}_][\p{L}\p{N}_/-]*)").unwrap();
    let mut count: HashMap<String, usize> = HashMap::new();
    for e in entries.iter().filter(|e| !e.is_dir && vault::is_md(&e.rel)).take(3000) {
        let Ok(t) = vault::read(root, &e.rel) else { continue };
        let mut in_code = false;
        for l in t.lines() {
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
        std::fs::write(dir.join("notes/a.md"), "# A #perso\n- [ ] sculpter #3d\n  - [x] blocking\n- pas une tâche\ncouleur #ff5a1f\n").unwrap();
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
        let z = make_backup(&root, &dir.join("_sauv")).unwrap();
        assert!(z.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
