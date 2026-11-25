//! L'IA dans l'éditeur de texte (le modèle choisi dans les réglages, local avec Ollama par défaut) :
//! modifier une sélection d'après une consigne (avec la différence à valider), relire une note,
//! remplir ses propriétés, expliquer une erreur de code. L'IA propose, tu acceptes ou non.

use eframe::egui;
use std::sync::mpsc::Receiver;

#[derive(Clone)]
pub enum Kind {
    /// Modifier la sélection (plage en octets) d'après une consigne
    Edit { start: usize, end: usize, original: String },
    /// Relecture : fautes, répétitions, tournures
    Proofread,
    /// Propriétés YAML déduites du texte
    Props,
    /// Expliquer une erreur de code et proposer un correctif
    Explain { error: String },
}

pub struct Task {
    pub kind: Kind,
    /// La consigne (pour « modifier »)
    pub instruction: String,
    pub rx: Option<Receiver<Result<String, String>>>,
    pub result: Option<Result<String, String>>,
    /// Corrections de relecture déjà appliquées ou écartées
    pub done: Vec<usize>,
    focus: bool,
}

impl Task {
    pub fn new(kind: Kind) -> Task {
        Task { kind, instruction: String::new(), rx: None, result: None, done: vec![], focus: true }
    }
}

/// Ce dont une tâche a besoin pour appeler le modèle
#[derive(Clone)]
pub struct Ctx {
    pub settings: crate::ai::Settings,
    pub local: crate::ollama::Local,
    pub lang: String,
}

const EDIT: &str = "Tu modifies un extrait de texte ou de code selon la consigne de l'utilisateur. Réponds uniquement avec l'extrait modifié, sans explication, sans balises de code, en gardant la langue, le style et l'indentation d'origine.";
const PROOF: &str = "Tu relis un texte en français ou en anglais, comme Antidote : fautes d'orthographe et de grammaire, accords, répétitions, tournures lourdes. Ne réécris pas le texte. Réponds avec une correction par ligne, au format exact : extrait fautif => correction | raison courte. L'extrait fautif doit être copié mot pour mot du texte. Si tout est correct, réponds : RIEN";
const PROPS: &str = "Tu déduis des propriétés pour l'en-tête YAML d'une note Markdown d'un artiste 3D et cinéaste. Réponds uniquement avec des lignes YAML (sans ---) : tags (liste de 2 à 5 mots-clés en minuscules), type (note, personnage, décor, plan, idée, réunion, tâche ou référence), résumé (une phrase), et statut si le texte le suggère. Garde la langue du texte.";
const EXPLAIN: &str = "Tu expliques une erreur de compilation ou d'exécution à un artiste qui code un peu. En trois à six phrases simples : ce que l'erreur veut dire, la cause probable dans le code donné, puis le correctif (un court extrait de code si utile). Réponds en français.";

fn spawn(c: &Ctx, system: &'static str, user: String, ctx: &egui::Context) -> Receiver<Result<String, String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let (settings, local, ctx) = (c.settings.clone(), c.local.clone(), ctx.clone());
    std::thread::spawn(move || {
        let ready = if crate::ollama::is_local(&settings.url) { local.ensure_running(&settings.url) } else { Ok(()) };
        let r = ready.and_then(|_| crate::ai::complete(&settings, system, &user)).map(|t| strip_fences(&t));
        let _ = tx.send(r);
        ctx.request_repaint();
    });
    rx
}

/// Retire les balises ``` qu'un modèle ajoute parfois
fn strip_fences(t: &str) -> String {
    let t = t.trim();
    // Les modèles qui réfléchissent : on garde ce qui suit la réflexion
    let t = t.rsplit_once("</think>").map(|x| x.1.trim()).unwrap_or(t);
    if let Some(rest) = t.strip_prefix("```") {
        let rest = rest.split_once('\n').map(|x| x.1).unwrap_or("");
        return rest.trim_end().trim_end_matches("```").trim_end().to_string();
    }
    t.to_string()
}

/// Lance une tâche qui n'attend pas de consigne
pub fn start(task: &mut Task, text: &str, c: &Ctx, ctx: &egui::Context) {
    let user = match &task.kind {
        Kind::Edit { original, .. } => format!("Consigne : {}\n\nExtrait :\n{original}", task.instruction.trim()),
        Kind::Proofread => text.to_string(),
        Kind::Props => text.chars().take(6000).collect(),
        Kind::Explain { error } => format!("Langage : {}\nErreur : {error}\n\nCode :\n{}", c.lang, text.chars().take(8000).collect::<String>()),
    };
    let system = match task.kind {
        Kind::Edit { .. } => EDIT,
        Kind::Proofread => PROOF,
        Kind::Props => PROPS,
        Kind::Explain { .. } => EXPLAIN,
    };
    task.result = None;
    task.rx = Some(spawn(c, system, user, ctx));
}

/// Les corrections de relecture : (extrait, correction, raison)
pub fn parse_proof(t: &str) -> Vec<(String, String, String)> {
    t.lines()
        .filter_map(|l| {
            let l = l.trim().trim_start_matches(['-', '*', '•', ' ']);
            let (bad, rest) = l.split_once("=>")?;
            let (good, why) = rest.split_once('|').unwrap_or((rest, ""));
            let (bad, good) = (bad.trim().trim_matches(['"', '«', '»', ' ']).to_string(), good.trim().trim_matches(['"', '«', '»', ' ']).to_string());
            (!bad.is_empty() && bad != good).then(|| (bad, good, why.trim().to_string()))
        })
        .collect()
}

/// Fusionne des lignes YAML dans l'en-tête de la note (créé s'il manque) ; les clés existantes ne bougent pas
pub fn merge_props(text: &str, yaml: &str) -> String {
    let new: Vec<&str> = yaml.lines().map(|l| l.trim_end()).filter(|l| !l.trim().is_empty() && l.trim() != "---").collect();
    let key = |l: &str| l.split_once(':').map(|(k, _)| k.trim().to_string()).filter(|k| !k.starts_with('-') && !k.is_empty());
    if let Some(rest) = text.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let head = &rest[..end];
            let have: Vec<String> = head.lines().filter(|l| !l.starts_with(' ') && !l.starts_with('-')).filter_map(key).collect();
            // Une clé déjà là garde sa valeur (et les lignes de liste qui la suivent sont écartées)
            let mut add = vec![];
            let mut skip = false;
            for l in &new {
                if !l.starts_with(' ') && !l.trim_start().starts_with('-') {
                    skip = key(l).is_some_and(|k| have.contains(&k));
                }
                if !skip {
                    add.push(*l);
                }
            }
            if add.is_empty() {
                return text.to_string();
            }
            return format!("---\n{head}\n{}{}", add.join("\n"), &rest[end..]);
        }
    }
    format!("---\n{}\n---\n\n{text}", new.join("\n"))
}

/// La barre de la tâche, au-dessus de l'éditeur. Vrai si le texte a changé.
pub fn bar(ui: &mut egui::Ui, slot: &mut Option<Task>, text: &mut String, c: &Ctx) -> bool {
    let Some(task) = slot.as_mut() else { return false };
    let p = crate::theme::pal();
    if let Some(rx) = &task.rx {
        if let Ok(r) = rx.try_recv() {
            task.result = Some(r);
            task.rx = None;
        }
    }
    let mut changed = false;
    let mut close = false;
    egui::Frame::new().fill(p.accent.gamma_multiply(0.10)).inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let title = match task.kind {
            Kind::Edit { .. } => "modifier la sélection avec l'IA",
            Kind::Proofread => "relecture par l'IA",
            Kind::Props => "propriétés proposées par l'IA",
            Kind::Explain { .. } => "explication de l'erreur",
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{}  {}", crate::lucide::SPARKLES, crate::i18n::t(title))).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button(crate::lucide::X).on_hover_text(crate::i18n::t("fermer (Échap)")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close = true;
                }
                if task.rx.is_some() {
                    ui.spinner();
                }
            });
        });
        // Consigne
        if let Kind::Edit { .. } = task.kind {
            if task.result.is_none() && task.rx.is_none() {
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut task.instruction).hint_text(crate::i18n::t("ex. : plus court, ton plus chaleureux, traduis en anglais, ajoute des commentaires…")).desired_width(ui.available_width() - 40.0));
                    if std::mem::take(&mut task.focus) {
                        r.request_focus();
                    }
                    let go = (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || ui.button(crate::lucide::SEND).on_hover_text(crate::i18n::t("envoyer (Entrée)")).clicked();
                    if go && !task.instruction.trim().is_empty() {
                        start(task, text, c, ui.ctx());
                    }
                });
            }
        }
        let (res, kind) = (task.result.clone(), task.kind.clone());
        let (mut retry, mut done) = (false, vec![]);
        match &res {
            None => {}
            Some(Err(e)) => {
                ui.label(egui::RichText::new(e).color(p.missing));
            }
            Some(Ok(out)) => match &kind {
                Kind::Edit { start, end, original } => {
                    // La différence : l'original barré, la proposition en vert
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        ui.label(egui::RichText::new(original).strikethrough().color(p.missing).monospace());
                        ui.label(egui::RichText::new(out).color(egui::Color32::from_rgb(90, 190, 120)).monospace());
                    });
                    ui.horizontal(|ui| {
                        let still = text.get(*start..*end) == Some(original.as_str());
                        if ui.add_enabled(still, egui::Button::new(format!("{}  {}", crate::lucide::CHECK, crate::i18n::t("accepter")))).on_disabled_hover_text(crate::i18n::t("le texte a changé depuis")).clicked() {
                            text.replace_range(*start..*end, out);
                            changed = true;
                            close = true;
                        }
                        if ui.button(format!("{}  {}", crate::lucide::REFRESH_CW, crate::i18n::t("autre consigne"))).clicked() {
                            retry = true;
                        }
                    });
                }
                Kind::Proofread => {
                    let items = parse_proof(out);
                    if items.is_empty() {
                        ui.label(crate::i18n::t("Rien à corriger."));
                    }
                    egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                        for (k, (bad, good, why)) in items.iter().enumerate() {
                            if task.done.contains(&k) {
                                continue;
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.label(egui::RichText::new(bad).strikethrough().color(p.missing));
                                ui.label("→");
                                ui.label(egui::RichText::new(good).color(egui::Color32::from_rgb(90, 190, 120)));
                                if !why.is_empty() {
                                    ui.label(egui::RichText::new(why).weak().size(11.0));
                                }
                                let found = text.find(bad.as_str());
                                if ui.add_enabled(found.is_some(), egui::Button::new(crate::lucide::CHECK)).on_hover_text(crate::i18n::t("appliquer")).on_disabled_hover_text(crate::i18n::t("extrait introuvable dans le texte")).clicked() {
                                    if let Some(at) = found {
                                        text.replace_range(at..at + bad.len(), good);
                                        changed = true;
                                    }
                                    done.push(k);
                                }
                                if ui.small_button(crate::lucide::X).on_hover_text(crate::i18n::t("ignorer")).clicked() {
                                    done.push(k);
                                }
                            });
                        }
                    });
                }
                Kind::Props => {
                    ui.label(egui::RichText::new(out).monospace());
                    if ui.button(format!("{}  {}", crate::lucide::CHECK, crate::i18n::t("ajouter à l'en-tête"))).on_hover_text(crate::i18n::t("Les propriétés déjà là gardent leur valeur")).clicked() {
                        *text = merge_props(text, out);
                        changed = true;
                        close = true;
                    }
                }
                Kind::Explain { .. } => {
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        ui.label(out);
                    });
                }
            },
        }
        if retry {
            task.result = None;
            task.focus = true;
        }
        task.done.extend(done);
    });
    if close {
        *slot = None;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relecture_et_proprietes() {
        let v = parse_proof("- les chat => les chats | accord\nRIEN\n« ca » => « ça » | accent");
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].0, "ca");
        let t = merge_props("# Titre\ntexte", "tags:\n  - perso\ntype: personnage");
        assert!(t.starts_with("---\ntags:\n  - perso\ntype: personnage\n---\n\n# Titre"));
        let t2 = merge_props("---\ntype: plan\n---\nx", "type: personnage\nrésumé: un essai");
        assert_eq!(t2, "---\ntype: plan\nrésumé: un essai\n---\nx");
        assert_eq!(strip_fences("```rust\nfn a() {}\n```"), "fn a() {}");
        assert_eq!(strip_fences("<think>hmm</think>\nok"), "ok");
    }
}
