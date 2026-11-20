//! Interface du coffre en ligne : réglages, bouton de la barre du bas, synchro automatique.
use super::*;



impl App {
    fn cloud_config(&mut self) -> cloud::Config {
        self.cloud_cfg.get_or_insert_with(cloud::Config::load).clone()
    }

    /// Lance la synchro (après avoir enregistré les fichiers ouverts).
    pub(crate) fn start_sync(&mut self, manual: bool) {
        let cfg = self.cloud_config();
        if !cfg.ready() {
            if manual {
                if let Some(ctx) = self.ctx.clone() {
                    self.open_settings_tab(&ctx, crate::settings::Tab::Cloud);
                }
            }
            return;
        }
        if self.vault.is_none() {
            return;
        }
        for i in 0..self.docs.len() {
            self.flush(i);
        }
        let root = self.root();
        if let Some(ctx) = self.ctx.clone() {
            self.cloud.start(cfg, root.clone(), cloud::prefix_for(&root), ctx);
        }
    }

    /// Appelé à chaque image : synchro périodique et rechargement après une synchro.
    pub(crate) fn cloud_tick(&mut self) {
        let cfg = self.cloud_config();
        if !cfg.ready() || !cfg.full_sync || self.vault.is_none() {
            return;
        }
        let snap = self.cloud.snapshot();
        if let Some(t) = snap.finished_at {
            if self.cloud_seen != Some(t) {
                self.cloud_seen = Some(t);
                if snap.down > 0 || snap.deleted > 0 || !snap.conflicts.is_empty() {
                    self.reload_from_disk();
                }
                if !snap.conflicts.is_empty() {
                    self.toast = Some((format!("synchro : {} conflit(s), les deux versions sont gardées", snap.conflicts.len()), Instant::now()));
                }
            }
        }
        if cfg.every_min > 0 && !snap.running {
            let due = self.cloud_next.is_none_or(|t| Instant::now() >= t);
            if due {
                self.cloud_next = Some(Instant::now() + Duration::from_secs(cfg.every_min as u64 * 60));
                self.start_sync(false);
            }
            if let Some(ctx) = &self.ctx {
                ctx.request_repaint_after(Duration::from_secs(30));
            }
        }
    }

    /// Recharge les fichiers ouverts non modifiés après une synchro.
    fn reload_from_disk(&mut self) {
        let root = self.root();
        self.refresh();
        for d in &mut self.docs {
            if d.dirty_since.is_some() {
                continue;
            }
            if let Content::Text(t) = &mut d.content {
                if let Ok(new) = crate::vault::read(&root, &d.rel) {
                    if *t != new {
                        *t = new;
                    }
                }
            }
        }
    }

    pub(crate) fn cloud_widget(&mut self, ui: &mut egui::Ui) {
        let cfg = self.cloud_config();
        if !cfg.ready() || !cfg.full_sync {
            return;
        }
        let p = theme::pal();
        let s = self.cloud.snapshot();
        if s.running {
            ui.add(egui::Spinner::new().size(10.0));
            let txt = if s.total > 0 { format!("synchro {}/{}", s.done, s.total) } else { "synchro…".into() };
            if ui.add(egui::Button::new(RichText::new(txt).color(p.muted).size(11.0)).frame(false)).on_hover_text(format!("{}\nclic : arrêter", s.step)).clicked() {
                self.cloud.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            return;
        }
        let (label, tip) = match (&s.error, s.finished_at) {
            (Some(e), _) => (RichText::new("☁ erreur").color(p.missing), format!("Synchro échouée : {e}\nclic : réessayer")),
            (None, Some(t)) => {
                let mins = t.elapsed().as_secs() / 60;
                let when = if mins == 0 { "à l'instant".to_string() } else { format!("il y a {mins} min") };
                (RichText::new("☁ synchronisé").color(p.muted), format!("Dernière synchro {when} : {} envoyé(s), {} reçu(s), {} supprimé(s)\nclic : synchroniser", s.up, s.down, s.deleted))
            }
            _ => (RichText::new("☁ synchroniser").color(p.muted), "Envoyer et recevoir les changements".to_string()),
        };
        if ui.add(egui::Button::new(label.size(11.0)).frame(false)).on_hover_text(tip).clicked() {
            self.start_sync(true);
        }
    }

    pub(crate) fn tab_cloud(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let p = theme::pal();
        let mut cfg = self.cloud_config();
        let before = cfg.clone();
        ui.label(caps("transfert par internet et coffre en ligne"));
        ui.label(RichText::new("Branche ton propre stockage compatible S3 : Cloudflare R2 (10 Go gratuits, téléchargement gratuit), Backblaze B2, AWS S3, MinIO… Les identifiants restent sur cet ordinateur, dans le dossier de config d'Aryadne : ils ne sont jamais copiés dans le coffre.").color(p.muted).size(11.5));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut cfg.full_sync, false, "relais temporaire");
            ui.selectable_value(&mut cfg.full_sync, true, "synchro complète du coffre");
        });
        let mode = if cfg.full_sync {
            "Le coffre entier est gardé en ligne et synchronisé entre tes ordinateurs. Occupe l'espace du coffre dans le bucket."
        } else {
            "Les fichiers passent par le bucket seulement le temps du transfert (fenêtre « transfert ») et sont effacés dès que l'autre ordinateur les reçoit. Ce qui n'est pas récupéré est effacé après 48 h. Coût : rien en pratique."
        };
        ui.label(RichText::new(mode).color(p.muted).size(11.0));
        ui.add_space(8.0);
        egui::CollapsingHeader::new("comment brancher Cloudflare R2").default_open(!cfg.ready()).show(ui, |ui| {
            for l in [
                "1. Tableau de bord Cloudflare › R2 › créer un bucket (ex. aryadne).",
                "2. R2 › « Manage API tokens » › créer un jeton « Object Read & Write » limité à ce bucket.",
                "3. Copie ici l'adresse S3 (https://<id-du-compte>.r2.cloudflarestorage.com), l'Access Key ID et le Secret Access Key.",
            ] {
                ui.label(RichText::new(l).size(11.5));
            }
            ui.hyperlink_to(RichText::new("ouvrir R2 dans Cloudflare ↗").size(11.0), "https://dash.cloudflare.com/?to=/:account/r2/overview");
        });
        ui.add_space(6.0);
        egui::Grid::new("cloud-grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("adresse S3");
            ui.add(egui::TextEdit::singleline(&mut cfg.endpoint).hint_text("https://….r2.cloudflarestorage.com").desired_width(380.0));
            ui.end_row();
            ui.label("bucket");
            ui.add(egui::TextEdit::singleline(&mut cfg.bucket).hint_text("aryadne").desired_width(380.0));
            ui.end_row();
            ui.label("access key id");
            ui.add(egui::TextEdit::singleline(&mut cfg.access_key).desired_width(380.0));
            ui.end_row();
            ui.label("secret access key");
            ui.add(egui::TextEdit::singleline(&mut cfg.secret_key).password(true).desired_width(380.0));
            ui.end_row();
            ui.label("région");
            ui.add(egui::TextEdit::singleline(&mut cfg.region).hint_text("auto (R2)").desired_width(120.0));
            ui.end_row();
            if cfg.full_sync {
                ui.label("automatique");
                ui.horizontal(|ui| {
                    for (v, l) in [(0, "à la main"), (5, "5 min"), (15, "15 min"), (60, "1 h")] {
                        ui.selectable_value(&mut cfg.every_min, v, l);
                    }
                });
                ui.end_row();
            }
        });
        if cfg != before {
            cfg.save();
            self.cloud_cfg = Some(cfg.clone());
            self.cloud_next = None;
        }
        ui.add_space(10.0);
        if self.vault.is_some() && cfg.full_sync {
            let root = self.root();
            ui.label(RichText::new(format!("Ce coffre va dans {}{}", cfg.bucket.trim(), format_args!("/{}", cloud::prefix_for(&root)))).color(p.muted).size(11.0));
        }
        ui.horizontal(|ui| {
            let busy = self.cloud.busy();
            if ui.add_enabled(cfg.ready() && !busy, egui::Button::new("tester la connexion")).clicked() {
                let c = cfg.clone();
                let ctx = ctx.clone();
                let prog = self.cloud.progress.clone();
                std::thread::spawn(move || {
                    let r = cloud::test(&c);
                    if let Ok(mut p) = prog.lock() {
                        p.error = r.err();
                        p.step = if p.error.is_none() { "connexion ok".into() } else { String::new() };
                    }
                    ctx.request_repaint();
                });
            }
            if cfg.full_sync && ui.add_enabled(cfg.ready() && !busy, egui::Button::new("synchroniser maintenant")).clicked() {
                self.start_sync(true);
            }
            if cfg.full_sync && ui.button("oublier l'état de synchro").on_hover_text("La prochaine synchro compare tout comme la première fois (sans rien supprimer)").clicked() && self.vault.is_some() {
                cloud::reset(&self.root());
            }
        });
        let s = self.cloud.snapshot();
        if let Some(e) = &s.error {
            ui.label(RichText::new(format!("✗ {e}")).color(p.missing).size(11.5));
        } else if s.step == "connexion ok" {
            ui.label(RichText::new("✓ connexion ok").color(p.accent).size(11.5));
        } else if s.finished_at.is_some() {
            ui.label(RichText::new(format!("✓ {} envoyé(s), {} reçu(s), {} supprimé(s), {} conflit(s)", s.up, s.down, s.deleted, s.conflicts.len())).color(p.muted).size(11.5));
        }
        ui.add_space(8.0);
        if cfg.full_sync {
        ui.label(RichText::new("Fichiers modifiés des deux côtés : les deux versions sont gardées (« (conflit …) »). Un fichier supprimé d'un côté va dans la corbeille de l'autre. Les dossiers liés de l'extérieur ne sont pas envoyés.").color(p.muted).size(11.0));
        }
    }

    fn relay_progress(&self, ui: &mut egui::Ui) {
        let p = theme::pal();
        let s = self.relay.snapshot();
        if s.running {
            ui.horizontal(|ui| {
                ui.spinner();
                let f = if s.total > 0 { s.done as f32 / s.total as f32 } else { 0.0 };
                ui.add(egui::ProgressBar::new(f).corner_radius(0.0).fill(p.accent).desired_width(260.0).text(RichText::new(format!("{} / {} · {}", s.done, s.total, s.step)).size(11.0)));
                if ui.small_button("annuler").clicked() {
                    self.relay.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            });
        } else if let Some(e) = &s.error {
            ui.label(RichText::new(format!("✗ {e}")).color(p.missing).size(11.5));
        } else if s.finished_at.is_some() {
            let t = if s.up > 0 { format!("✓ {} fichier(s) envoyé(s). Sur l'autre ordinateur : transfert › recevoir › par internet.", s.up) } else { format!("✓ {} fichier(s) reçu(s) dans reçus/, effacés du bucket.", s.down) };
            ui.label(RichText::new(t).color(p.accent).size(11.5));
        }
    }

    fn refresh_relay_list(&self, ctx: &egui::Context) {
        let (cfg, list, ctx) = (self.cloud_cfg.clone().unwrap_or_default(), self.relay_list.clone(), ctx.clone());
        if let Ok(mut l) = list.lock() {
            *l = None;
        }
        std::thread::spawn(move || {
            let r = cloud::relay_pending(&cfg);
            if let Ok(mut l) = list.lock() {
                *l = Some(r);
            }
            ctx.request_repaint();
        });
    }

    /// Section « par internet » de l'onglet recevoir du transfert.
    pub(crate) fn relay_receive_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let p = theme::pal();
        let cfg = self.cloud_config();
        ui.add_space(10.0);
        ui.separator();
        ui.label(caps("par internet"));
        if !cfg.ready() {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Pour recevoir d'un ordinateur ailleurs, branche un stockage (R2…).").color(p.muted).size(11.5));
                if ui.small_button("réglages").clicked() {
                    self.open_settings_tab(ctx, settings::Tab::Cloud);
                }
            });
            return;
        }
        let state = self.relay_list.lock().ok().and_then(|l| l.clone());
        if state.is_none() && !self.relay.busy() && self.relay.snapshot().finished_at.is_none() && !self.relay_asked {
            self.relay_asked = true;
            self.refresh_relay_list(ctx);
        }
        let mut take = None;
        match state {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("recherche des colis en attente…").color(p.muted).size(11.5));
                });
            }
            Some(Err(e)) => {
                ui.label(RichText::new(format!("✗ {e}")).color(p.missing).size(11.5));
            }
            Some(Ok(list)) if list.is_empty() => {
                ui.label(RichText::new("Aucun colis en attente.").color(p.muted).size(11.5));
            }
            Some(Ok(list)) => {
                for pk in list {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{} · {} fichier(s) · {} · {}", pk.from, pk.files, crate::transfer::human(pk.size), pk.date)).size(11.5));
                        if ui.add_enabled(!self.relay.busy(), egui::Button::new("recevoir")).clicked() {
                            take = Some(pk.id.clone());
                        }
                    });
                }
            }
        }
        if ui.small_button("actualiser").clicked() {
            self.refresh_relay_list(ctx);
        }
        if let Some(id) = take {
            let (cfg, root) = (cfg.clone(), self.root());
            let list = self.relay_list.clone();
            self.relay.run_job(ctx.clone(), move |prog, _stop, ctx| {
                let r = cloud::relay_receive(&cfg, &root, &id, prog, ctx);
                if let Ok(mut l) = list.lock() {
                    *l = Some(cloud::relay_pending(&cfg));
                }
                r
            });
            self.cloud_seen = None;
        }
        self.relay_progress(ui);
        if self.relay.snapshot().finished_at.is_some_and(|t| Some(t) != self.relay_done_seen) {
            self.relay_done_seen = self.relay.snapshot().finished_at;
            self.refresh();
        }
    }

    /// Bouton « envoyer par internet » de l'onglet envoyer.
    pub(crate) fn relay_send_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let p = theme::pal();
        let cfg = self.cloud_config();
        ui.add_space(10.0);
        ui.separator();
        ui.label(caps("ou par internet"));
        if !cfg.ready() {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Pour envoyer vers un ordinateur ailleurs, branche un stockage (R2…).").color(p.muted).size(11.5));
                if ui.small_button("réglages").clicked() {
                    self.open_settings_tab(ctx, settings::Tab::Cloud);
                }
            });
            return;
        }
        ui.label(RichText::new("Les fichiers passent par ton bucket et sont effacés dès que l'autre ordinateur les reçoit.").color(p.muted).size(11.0));
        let ok = !self.send_files.is_empty() && !self.relay.busy();
        if ui.add_enabled(ok, egui::Button::new("envoyer par internet")).clicked() {
            self.flush_all();
            let (root, files) = (self.root(), self.send_files.clone());
            self.relay.run_job(ctx.clone(), move |prog, stop, ctx| cloud::relay_send(&cfg, &root, &files, prog, stop, ctx));
        }
        self.relay_progress(ui);
    }
}
