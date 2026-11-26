//! Le mode « Apprendre » : des leçons Markdown embarquées (docs/apprendre, voir FORMAT.md) lues par un
//! panneau à côté de la zone de travail. Ce fichier tient la logique, sans interface : lecture de
//! l'en-tête et des blocs (`essayer`, `etape`, `quiz`, `astuce`, `attention`, `raccourci`, `defi`),
//! catalogue des parcours, vérifications des étapes sur l'état de l'application, progression gardée
//! dans `<coffre>/.app/apprendre.json`. L'interface est dans `learn_ui.rs`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Dossier du coffre où s'ouvrent les points de départ des leçons (copies : l'original n'est jamais touché).
pub const DIR: &str = "Apprendre";
/// Fichier de progression, dans le coffre.
pub const PROGRESS: &str = ".app/apprendre.json";
/// Le parcours proposé au tout premier lancement d'un coffre.
pub const FIRST: &str = "01-demarrer";

// ---------------------------------------------------------------------------------------------- en-tête

/// En-tête d'une page (parcours ou leçon).
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Meta {
    pub titre: String,
    /// « débutant », « intermédiaire », « avancé » (tel qu'écrit)
    pub niveau: String,
    pub duree: String,
    /// Leçons à avoir suivies avant : chemins sans .md (« 01-demarrer/02-l-interface »)
    pub prerequis: Vec<String>,
    pub objectifs: Vec<String>,
}

/// Sépare l'en-tête YAML (entre deux lignes `---`) du corps. Sans en-tête : tout est du corps.
pub fn split_front(text: &str) -> (Meta, &str) {
    let mut meta = Meta::default();
    let t = text.trim_start_matches('\u{feff}');
    let Some(rest) = t.strip_prefix("---") else { return (meta, t) };
    let Some(rest) = rest.strip_prefix("\r\n").or_else(|| rest.strip_prefix('\n')) else { return (meta, t) };
    // La ligne de fin : « --- » seule
    let mut at = 0;
    let mut end = None;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            end = Some((at, at + line.len()));
            break;
        }
        at += line.len();
    }
    let Some((a, b)) = end else { return (meta, t) };
    for line in rest[..a].lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        let v = v.trim().trim_matches('"').trim();
        match crate::chat::fold(k.trim()).to_lowercase().as_str() {
            "titre" | "title" => meta.titre = v.to_string(),
            "niveau" => meta.niveau = v.to_string(),
            "duree" => meta.duree = v.to_string(),
            "prerequis" => meta.prerequis = v.split(',').map(|s| s.trim().trim_end_matches(".md").to_string()).filter(|s| !s.is_empty() && s != "aucun").collect(),
            "objectifs" => meta.objectifs = v.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            _ => {}
        }
    }
    (meta, &rest[b..])
}

/// Rang d'un niveau : 0 débutant, 1 intermédiaire, 2 avancé.
pub fn level_rank(niveau: &str) -> u8 {
    let n = crate::chat::fold(niveau).to_lowercase();
    if n.starts_with("inter") {
        1
    } else if n.starts_with("avan") || n.starts_with("expert") {
        2
    } else {
        0
    }
}

/// Minutes annoncées par une durée (« 15 min », « 1 h 30 », « 2 h »). 0 si illisible.
pub fn minutes(duree: &str) -> u32 {
    let d = duree.to_lowercase();
    let mut total = 0;
    let mut num = String::new();
    let mut pending: Option<u32> = None;
    for c in d.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        if !num.is_empty() {
            pending = num.parse().ok();
            num.clear();
        }
        if c == 'h' {
            if let Some(n) = pending.take() {
                total += n * 60;
            }
        } else if c == 'm' {
            if let Some(n) = pending.take() {
                total += n;
            }
        } else if c.is_alphabetic() {
            // Un nombre suivi d'un autre mot (« 3 leçons ») n'est pas une durée
            pending = None;
        }
    }
    // « 1 h 30 » : le nombre qui suit les heures compte en minutes ; un nombre seul aussi
    if let Some(n) = pending {
        total += n;
    }
    total
}

/// Écrit des minutes comme une durée courte : « 45 min », « 1 h 30 ».
pub fn duration_text(min: u32) -> String {
    match (min / 60, min % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m:02}"),
    }
}

// ---------------------------------------------------------------------------------------------- blocs

/// Point de départ d'un bloc `essayer`.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// Fichier vide de ce type (extension sans le point), avec un nom facultatif
    Nouveau { ext: String, nom: Option<String> },
    /// Exemple intégré (voir `examples`)
    Exemple(String),
    /// Fichier fourni dans docs/apprendre/ (chemin relatif à ce dossier : « fichiers/scene.aryscene »)
    Fichier(String),
}

/// Vérification d'une étape (ligne `verifier:`).
#[derive(Clone, Debug, PartialEq)]
pub enum Check {
    Onglet(String),
    SousOnglet(String),
    FichierOuvert(String),
    FichierEnregistre,
    GrapheContient(String),
    GrapheNoeuds(usize),
    GrapheLiens(usize),
    GrapheGroupe,
    AtelierCalques(usize),
    SceneObjets(usize),
    SceneSelection,
    LectureImage(i32),
    TexteContient(String),
    Calques(usize),
    /// Vérification que le moteur ne connaît pas : l'étape se coche à la main
    Inconnue(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Callout {
    Astuce,
    Attention,
    Raccourci,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    /// Markdown ordinaire
    Texte(String),
    /// Bouton « Essaie par toi-même » ; Err : bloc mal écrit (le message dit pourquoi)
    Essayer(Result<Source, String>),
    Etape { texte: String, verifier: Option<Check> },
    Quiz { question: String, choix: Vec<(String, bool)>, explication: String },
    Encadre { genre: Callout, texte: String },
    Defi { texte: String, indice: Option<String> },
}

const KEYWORDS: [&str; 7] = ["essayer", "etape", "quiz", "astuce", "attention", "raccourci", "defi"];

/// Les types de fichiers qu'un bloc `essayer` sait créer vides.
pub const NEW_KINDS: [&str; 13] = ["md", "aryscene", "arymat", "arygeo", "arycomp", "aryimg", "aryvec", "aryseq", "arymotion", "aryaudio", "arymusic", "aryboard", "arystory"];

/// Un fichier vide de cette extension peut-il être créé ? Les types d'Aryadne, et les fichiers de texte ou de code.
pub fn can_create(ext: &str) -> bool {
    NEW_KINDS.contains(&ext) || crate::workspace::CODE.contains(&ext) || matches!(ext, "txt" | "fountain" | "csv")
}

/// Retire un commentaire entre parenthèses en fin de ligne, séparé par au moins deux espaces
/// (FORMAT.md en met dans ses exemples).
fn strip_comment(v: &str) -> &str {
    match v.find("  (") {
        Some(i) if v.trim_end().ends_with(')') => v[..i].trim(),
        _ => v.trim(),
    }
}

/// `clé: valeur` en début de ligne, la clé comparée sans accents ni majuscules.
fn key_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let (k, v) = line.split_once(':')?;
    (crate::chat::fold(k.trim()).to_lowercase() == key).then_some(v.trim())
}

fn parse_essayer(body: &str) -> Result<Source, String> {
    let (mut nouveau, mut nom, mut exemple, mut fichier) = (None, None, None, None);
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        if let Some(v) = key_value(line, "nouveau") {
            nouveau = Some(strip_comment(v).split_whitespace().next().unwrap_or("").trim_start_matches('.').to_lowercase());
        } else if let Some(v) = key_value(line, "nom") {
            nom = Some(strip_comment(v).to_string()).filter(|n| !n.is_empty());
        } else if let Some(v) = key_value(line, "exemple") {
            exemple = Some(strip_comment(v).to_string());
        } else if let Some(v) = key_value(line, "fichier") {
            fichier = Some(strip_comment(v).to_string());
        } else {
            return Err(format!("ligne non comprise : {}", line.trim()));
        }
    }
    match (nouveau, exemple, fichier) {
        (Some(ext), None, None) => {
            if can_create(&ext) {
                Ok(Source::Nouveau { ext, nom })
            } else {
                Err(format!("type de fichier inconnu : {ext}"))
            }
        }
        (None, Some(e), None) if !e.is_empty() => Ok(Source::Exemple(e)),
        (None, None, Some(f)) if !f.is_empty() => {
            // Le chemin reste dans docs/apprendre
            if f.split('/').any(|s| s == "..") || f.starts_with('/') {
                Err(format!("chemin refusé : {f}"))
            } else {
                Ok(Source::Fichier(f))
            }
        }
        (None, None, None) => Err("aucune source (nouveau, exemple ou fichier)".into()),
        _ => Err("une seule source à la fois (nouveau, exemple ou fichier)".into()),
    }
}

/// Lit une ligne `verifier:` (sans son mot-clé).
pub fn parse_check(raw: &str) -> Check {
    let raw = raw.trim();
    let norm = raw.replace('≥', ">=").replace("=>", ">=");
    let (name, op, value) = if let Some((a, b)) = norm.split_once(">=") {
        (a, ">=", b)
    } else if let Some((a, b)) = norm.split_once('=') {
        (a, "=", b)
    } else {
        (norm.as_str(), "", "")
    };
    let name = crate::chat::fold(name.trim()).to_lowercase().replace('œ', "oe").replace(' ', "");
    let value = value.trim().trim_matches('`').trim_matches('"').trim();
    let num = || value.parse::<f32>().ok().filter(|n| *n >= 0.0).map(|n| n.round() as usize);
    let unknown = || Check::Inconnue(raw.to_string());
    match (name.as_str(), op) {
        ("onglet", "=") if !value.is_empty() => Check::Onglet(value.to_lowercase()),
        ("sous_onglet" | "sous-onglet" | "sousonglet", "=") if !value.is_empty() => Check::SousOnglet(value.to_lowercase()),
        ("fichier.ouvert", "=") if !value.is_empty() => Check::FichierOuvert(value.trim_start_matches('.').to_lowercase()),
        ("fichier.enregistre", "") => Check::FichierEnregistre,
        ("graphe.contient", "=") if !value.is_empty() => Check::GrapheContient(value.to_string()),
        ("graphe.noeuds" | "graphe.nodes", ">=") => num().map(Check::GrapheNoeuds).unwrap_or_else(unknown),
        ("graphe.liens", ">=") => num().map(Check::GrapheLiens).unwrap_or_else(unknown),
        ("graphe.groupe", "") => Check::GrapheGroupe,
        ("atelier.calques", ">=") => num().map(Check::AtelierCalques).unwrap_or_else(unknown),
        ("scene.objets", ">=") => num().map(Check::SceneObjets).unwrap_or_else(unknown),
        ("scene.selection", "") => Check::SceneSelection,
        ("lecture.image", ">=") => value.parse::<f32>().ok().map(|n| Check::LectureImage(n.round() as i32)).unwrap_or_else(unknown),
        ("texte.contient", "=") if !value.is_empty() => Check::TexteContient(value.to_string()),
        ("calques", ">=") => num().map(Check::Calques).unwrap_or_else(unknown),
        _ => unknown(),
    }
}

fn parse_etape(body: &str) -> Block {
    let mut texte: Vec<&str> = vec![];
    let mut verifier = None;
    for line in body.lines() {
        match key_value(line, "verifier") {
            Some(v) if verifier.is_none() => verifier = Some(parse_check(v)).filter(|_| !v.trim().is_empty()),
            _ => texte.push(line),
        }
    }
    Block::Etape { texte: texte.join("\n").trim().to_string(), verifier }
}

/// Un quiz : la question, ses choix (`- [ ]`, `- [x]`), l'explication. None s'il n'a pas de choix
/// ou pas de bonne réponse : le bloc reste alors du texte.
fn parse_quiz(body: &str) -> Option<Block> {
    let mut question: Vec<&str> = vec![];
    let mut choix: Vec<(String, bool)> = vec![];
    let mut explication: Option<String> = None;
    for line in body.lines() {
        let t = line.trim();
        if let Some(e) = explication.as_mut() {
            // L'explication peut tenir sur plusieurs lignes
            if !t.is_empty() {
                e.push(' ');
                e.push_str(t);
            }
            continue;
        }
        if let Some(v) = key_value(line, "explication") {
            explication = Some(v.to_string());
            continue;
        }
        let item = t.strip_prefix("- [").or_else(|| t.strip_prefix("* ["));
        match item.and_then(|r| r.split_once(']')) {
            Some((mark, rest)) if matches!(mark.trim(), "" | "x" | "X") => choix.push((rest.trim().to_string(), !mark.trim().is_empty())),
            _ if choix.is_empty() => question.push(line),
            // Une ligne qui suit un choix le prolonge
            _ if !t.is_empty() => {
                if let Some(last) = choix.last_mut() {
                    last.0.push(' ');
                    last.0.push_str(t);
                }
            }
            _ => {}
        }
    }
    if choix.len() < 2 || !choix.iter().any(|c| c.1) {
        return None;
    }
    Some(Block::Quiz { question: question.join("\n").trim().to_string(), choix, explication: explication.unwrap_or_default().trim().to_string() })
}

fn parse_defi(body: &str) -> Block {
    let mut texte: Vec<&str> = vec![];
    let mut indice: Option<String> = None;
    for line in body.lines() {
        if let Some(i) = indice.as_mut() {
            i.push('\n');
            i.push_str(line);
        } else if let Some(v) = key_value(line, "indice") {
            indice = Some(v.to_string());
        } else {
            texte.push(line);
        }
    }
    Block::Defi { texte: texte.join("\n").trim().to_string(), indice: indice.map(|i| i.trim().to_string()).filter(|i| !i.is_empty()) }
}

/// Découpe le corps d'une leçon en blocs. Un bloc de code dont le mot-clé n'est pas un des nôtres,
/// un bloc jamais refermé ou un quiz sans réponse restent du texte, tels quels.
pub fn parse_blocks(body: &str) -> Vec<Block> {
    let mut out: Vec<Block> = vec![];
    let mut text = String::new();
    let lines: Vec<&str> = body.lines().collect();
    let fence_of = |l: &str| -> Option<(usize, String)> {
        let t = l.trim_start();
        let n = t.chars().take_while(|c| *c == '`').count();
        (n >= 3).then(|| (n, t[n..].trim().to_string()))
    };
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some((n, info)) = fence_of(line) {
            // La fin du bloc : une ligne d'au moins autant d'accents graves, seule
            let end = (i + 1..lines.len()).find(|&j| fence_of(lines[j]).is_some_and(|(m, rest)| m >= n && rest.is_empty()));
            let kw = crate::chat::fold(&info).to_lowercase();
            match end {
                Some(j) if KEYWORDS.contains(&kw.as_str()) => {
                    let inner = lines[i + 1..j].join("\n");
                    let block = match kw.as_str() {
                        "essayer" => Some(Block::Essayer(parse_essayer(&inner))),
                        "etape" => Some(parse_etape(&inner)).filter(|b| !matches!(b, Block::Etape { texte, .. } if texte.is_empty())),
                        "quiz" => parse_quiz(&inner),
                        "defi" => Some(parse_defi(&inner)).filter(|b| !matches!(b, Block::Defi { texte, .. } if texte.is_empty())),
                        k => {
                            let genre = match k {
                                "astuce" => Callout::Astuce,
                                "attention" => Callout::Attention,
                                _ => Callout::Raccourci,
                            };
                            Some(Block::Encadre { genre, texte: inner.trim().to_string() }).filter(|_| !inner.trim().is_empty())
                        }
                    };
                    match block {
                        Some(b) => {
                            if !text.trim().is_empty() {
                                out.push(Block::Texte(std::mem::take(&mut text).trim_matches('\n').to_string()));
                            }
                            text.clear();
                            out.push(b);
                        }
                        // Bloc vide ou mal écrit : il reste affiché comme du code, pour que le rédacteur le voie
                        None => {
                            for l in &lines[i..=j] {
                                text.push_str(l);
                                text.push('\n');
                            }
                        }
                    }
                    i = j + 1;
                    continue;
                }
                Some(j) => {
                    // Bloc de code ordinaire : recopié d'un seul tenant (son contenu n'est pas relu)
                    for l in &lines[i..=j] {
                        text.push_str(l);
                        text.push('\n');
                    }
                    i = j + 1;
                    continue;
                }
                None => {}
            }
        }
        text.push_str(line);
        text.push('\n');
        i += 1;
    }
    if !text.trim().is_empty() {
        out.push(Block::Texte(text.trim_matches('\n').to_string()));
    }
    out
}

/// Une leçon lue : son en-tête et ses blocs.
#[derive(Clone, Debug, Default)]
pub struct Lesson {
    pub meta: Meta,
    pub blocks: Vec<Block>,
}

impl Lesson {
    pub fn parse(text: &str) -> Lesson {
        let (meta, body) = split_front(text);
        let mut blocks = parse_blocks(body);
        // Le titre de l'en-tête est déjà affiché : un « # Titre » identique en tête du corps ferait doublon
        if let Some(Block::Texte(t)) = blocks.first_mut() {
            if let Some(first) = t.lines().next() {
                if first.starts_with("# ") && crate::chat::fold(first[2..].trim()).to_lowercase() == crate::chat::fold(meta.titre.trim()).to_lowercase() {
                    *t = t[first.len()..].trim_start_matches('\n').to_string();
                }
            }
            if t.trim().is_empty() {
                blocks.remove(0);
            }
        }
        Lesson { meta, blocks }
    }

    /// Nombre d'étapes et de quiz.
    pub fn counts(&self) -> (usize, usize) {
        (self.blocks.iter().filter(|b| matches!(b, Block::Etape { .. })).count(), self.blocks.iter().filter(|b| matches!(b, Block::Quiz { .. })).count())
    }
}

// ---------------------------------------------------------------------------------------------- catalogue

#[derive(Clone, Debug)]
pub struct LessonRef {
    /// « 01-demarrer/02-l-interface » (chemin dans docs/apprendre, sans .md)
    pub id: String,
    pub meta: Meta,
    /// Étapes et quiz de la leçon
    pub steps: usize,
    pub quizzes: usize,
}

#[derive(Clone, Debug)]
pub struct Course {
    /// « 01-demarrer »
    pub id: String,
    pub meta: Meta,
    pub lessons: Vec<LessonRef>,
}

impl Course {
    /// Durée annoncée par le parcours, sinon la somme de ses leçons.
    pub fn minutes(&self) -> u32 {
        match minutes(&self.meta.duree) {
            0 => self.lessons.iter().map(|l| minutes(&l.meta.duree)).sum(),
            m => m,
        }
    }
}

/// Nom lisible tiré d'un identifiant de dossier ou de fichier : « 02-l-interface » → « L interface ».
pub fn pretty(id: &str) -> String {
    let name = id.rsplit('/').next().unwrap_or(id);
    let name = name.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start_matches('-');
    crate::i18n::sentence(&name.replace('-', " "))
}

/// Range des pages (chemin relatif à docs/, texte) en parcours. Seul `apprendre/<parcours>/<page>.md` compte.
pub fn catalog_from(files: &[(&str, &str)]) -> Vec<Course> {
    let mut courses: BTreeMap<String, Course> = BTreeMap::new();
    for (rel, text) in files {
        let Some(rest) = rel.strip_prefix("apprendre/") else { continue };
        let Some((dir, file)) = rest.split_once('/') else { continue };
        if dir == "fichiers" || file.contains('/') || !file.ends_with(".md") {
            continue;
        }
        let c = courses.entry(dir.to_string()).or_insert_with(|| Course { id: dir.to_string(), meta: Meta { titre: pretty(dir), ..Default::default() }, lessons: vec![] });
        let stem = file.trim_end_matches(".md");
        if stem == "index" {
            let (meta, _) = split_front(text);
            let titre = if meta.titre.is_empty() { c.meta.titre.clone() } else { meta.titre.clone() };
            c.meta = Meta { titre, ..meta };
        } else if stem.to_lowercase() != "readme" {
            let l = Lesson::parse(text);
            let (steps, quizzes) = l.counts();
            let mut meta = l.meta;
            if meta.titre.is_empty() {
                meta.titre = pretty(stem);
            }
            c.lessons.push(LessonRef { id: format!("{dir}/{stem}"), meta, steps, quizzes });
        }
    }
    let mut v: Vec<Course> = courses.into_values().filter(|c| !c.lessons.is_empty()).collect();
    for c in &mut v {
        c.lessons.sort_by(|a, b| a.id.cmp(&b.id));
    }
    v
}

/// Les parcours embarqués dans le binaire.
pub fn catalog() -> &'static [Course] {
    static CAT: std::sync::LazyLock<Vec<Course>> = std::sync::LazyLock::new(|| catalog_from(crate::docs::FILES));
    &CAT
}

/// Texte d'une page embarquée (« 01-demarrer/02-l-interface », ou « 01-demarrer/index »).
pub fn page_text(id: &str) -> Option<&'static str> {
    let rel = format!("apprendre/{id}.md");
    crate::docs::FILES.iter().find(|f| f.0 == rel).map(|f| f.1)
}

/// Fichier fourni avec les leçons (« fichiers/scene.aryscene »).
pub fn asset(rel: &str) -> Option<&'static [u8]> {
    let full = format!("apprendre/{}", rel.trim_start_matches("./"));
    crate::docs::ASSETS.iter().find(|a| a.0 == full).map(|a| a.1)
}

/// La leçon d'avant et celle d'après, dans l'ordre des parcours (on passe d'un parcours au suivant).
pub fn neighbours(cat: &[Course], id: &str) -> (Option<String>, Option<String>) {
    let all: Vec<&str> = cat.iter().flat_map(|c| c.lessons.iter().map(|l| l.id.as_str())).collect();
    match all.iter().position(|l| *l == id) {
        Some(i) => (i.checked_sub(1).map(|j| all[j].to_string()), all.get(i + 1).map(|s| s.to_string())),
        None => (None, None),
    }
}

/// Le parcours d'une leçon.
pub fn course_of(id: &str) -> &str {
    id.split('/').next().unwrap_or(id)
}

// ---------------------------------------------------------------------------------------------- exemples

/// Les exemples intégrés qu'un bloc `essayer` peut ouvrir : (nom, extension du fichier créé).
pub fn examples() -> Vec<(&'static str, &'static str)> {
    let mut v: Vec<(&'static str, &'static str)> = crate::geo::EXAMPLES.iter().map(|n| (*n, "arygeo")).collect();
    v.extend(crate::nodes::MATERIAL_PRESETS.iter().map(|n| (*n, "arymat")));
    v.extend([("scène de départ", "aryscene"), ("morceau de départ", "arymusic"), ("session audio de départ", "aryaudio"), ("dessin de départ", "aryvec"), ("storyboard de départ", "arystory")]);
    v
}

/// Retrouve un exemple par son nom, sans tenir compte des accents ni des majuscules.
pub fn find_example(name: &str) -> Option<(&'static str, &'static str)> {
    let want = crate::chat::fold(name.trim()).to_lowercase();
    examples().into_iter().find(|(n, _)| crate::chat::fold(n).to_lowercase() == want)
}

/// Écrit un exemple intégré dans un fichier.
pub fn write_example(name: &str, path: &std::path::Path) -> Result<(), String> {
    let Some((n, _)) = find_example(name) else { return Err(format!("exemple inconnu : {name}")) };
    if let Some(g) = crate::geo::example(n) {
        return g.save(path);
    }
    if let Some(g) = crate::nodes::material_preset(n) {
        return g.save(path);
    }
    match n {
        "scène de départ" => crate::scene::Scene::starter().write(path),
        "morceau de départ" => crate::music::Song::starter().write(path),
        "session audio de départ" => crate::audio::Session::starter().write(path),
        "dessin de départ" => crate::vector::Doc::starter().save(path),
        "storyboard de départ" => crate::story::Story::starter().save(path),
        _ => Err(format!("exemple inconnu : {name}")),
    }
}

/// Écrit un fichier vide du type demandé (les éditeurs d'Aryadne partent de leur document de départ).
pub fn write_new(ext: &str, path: &std::path::Path) -> Result<(), String> {
    let text = |t: &str| crate::vault::atomic_write(path, t).map_err(|e| e.to_string());
    match ext {
        "aryscene" => crate::scene::Scene::starter().write(path),
        "arymat" => crate::nodes::Graph::new_material().save(path),
        "arygeo" => crate::nodes::Graph::new_geometry().save(path),
        "arycomp" => crate::nodes::Graph::new_compositor().save(path),
        "aryimg" => crate::paint::Doc::new(1920, 1080, true).save(path),
        "aryvec" => crate::vector::Doc::starter().save(path),
        "aryseq" => crate::montage::Project::default().write(path),
        "arymotion" => crate::motion::Compo::default().write(path),
        "aryaudio" => crate::audio::Session::starter().write(path),
        "arymusic" => crate::music::Song::starter().write(path),
        "arystory" => crate::story::Story::starter().save(path),
        "aryboard" => text("{\n  \"items\": []\n}\n"),
        e if can_create(e) => text(""),
        e => Err(format!("type de fichier inconnu : {e}")),
    }
}

/// Nom de fichier sûr tiré d'un texte libre (pas de séparateur de dossier ni de caractère refusé par Windows).
pub fn safe_name(s: &str) -> String {
    let n: String = s.trim().chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { ' ' } else { c }).collect();
    let n = n.split_whitespace().collect::<Vec<_>>().join(" ");
    let n = n.trim_matches(|c| c == '.' || c == ' ').to_string();
    if n.is_empty() { "exercice".into() } else { n }
}

/// Le fichier qu'un bloc `essayer` crée dans le coffre : (nom sans extension, extension).
pub fn target_of(src: &Source, lesson_id: &str) -> Result<(String, String), String> {
    match src {
        Source::Nouveau { ext, nom } => Ok((safe_name(nom.as_deref().unwrap_or(&pretty(lesson_id).to_lowercase())), ext.clone())),
        Source::Exemple(name) => find_example(name).map(|(n, e)| (safe_name(n), e.to_string())).ok_or_else(|| format!("exemple inconnu : {name}")),
        Source::Fichier(f) => {
            let file = f.rsplit('/').next().unwrap_or(f);
            if asset(f).is_none() {
                return Err(format!("fichier introuvable : {f}"));
            }
            // Les extensions d'Aryadne peuvent avoir un point dans leur ancien nom : on coupe au dernier point
            match file.rsplit_once('.') {
                Some((stem, ext)) => Ok((safe_name(stem), ext.to_lowercase())),
                None => Err(format!("fichier sans extension : {f}")),
            }
        }
    }
}

/// Crée le point de départ dans `<coffre>/Apprendre/` et rend son chemin dans le coffre. Un fichier
/// du même nom déjà là n'est jamais écrasé : la copie prend un numéro.
pub fn start(root: &str, src: &Source, lesson_id: &str) -> Result<String, String> {
    let (stem, ext) = target_of(src, lesson_id)?;
    let dir = std::path::Path::new(root).join(DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut name = format!("{stem}.{ext}");
    let mut k = 2;
    while dir.join(&name).exists() {
        name = format!("{stem} {k}.{ext}");
        k += 1;
    }
    let path = dir.join(&name);
    match src {
        Source::Nouveau { ext, .. } => write_new(ext, &path)?,
        Source::Exemple(n) => write_example(n, &path)?,
        Source::Fichier(f) => std::fs::write(&path, asset(f).ok_or_else(|| format!("fichier introuvable : {f}"))?).map_err(|e| e.to_string())?,
    }
    Ok(format!("{DIR}/{name}"))
}

// ---------------------------------------------------------------------------------------------- vérifications

/// Ce qu'on sait du graphe de nodes actif, tous niveaux confondus (racine, groupes, calques).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphStats {
    /// Opération (nom interne) et nom affiché de chaque node
    pub ops: Vec<(String, String)>,
    pub links: usize,
    pub group: bool,
}

impl GraphStats {
    pub fn add(&mut self, nodes: &[crate::nodes::Node], links: &[crate::nodes::Link]) {
        for n in nodes {
            let label = crate::nodes::def(&n.op).map(|d| d.label.to_string()).unwrap_or_default();
            if n.op == "group" {
                self.group = true;
            }
            self.ops.push((n.op.clone(), label));
        }
        self.links += links.len();
    }

    /// Tout un graphe : sa racine, l'intérieur de ses groupes, les graphes de ses calques (sauf `skip`).
    pub fn add_graph(&mut self, g: &crate::nodes::Graph, skip: Option<usize>) {
        self.add(&g.nodes, &g.links);
        for gr in &g.groups {
            self.group = true;
            self.add(&gr.nodes, &gr.links);
        }
        if let Some(stack) = &g.stack {
            for (i, l) in stack.layers.iter().enumerate() {
                if Some(i) == skip {
                    continue;
                }
                for inner in [l.graph.as_ref(), l.mask.graph.as_ref()].into_iter().flatten() {
                    self.add_graph(&inner.0, None);
                }
            }
        }
    }

    /// Le graphe contient-il ce node ? `want` : le nom interne (« sim_solver »), sa fin (« solver »),
    /// ou le nom affiché en français ou en anglais (« Solveur », « Solver »).
    pub fn contains(&self, want: &str) -> bool {
        let fold = |s: &str| crate::chat::fold(s.trim()).to_lowercase().replace([' ', '-'], "_");
        let w = fold(want);
        if w.is_empty() {
            return false;
        }
        self.ops.iter().any(|(op, label)| {
            let o = op.to_lowercase();
            o == w || o.split_once('_').is_some_and(|(_, rest)| rest == w) || (!label.is_empty() && (fold(label) == w || fold(&crate::i18n::en(label)) == w))
        })
    }
}

/// L'état de l'application que lisent les vérifications. Les tests en fabriquent un à la main.
#[derive(Clone, Debug, Default)]
pub struct State<'a> {
    /// Espace affiché : « notes », « 2d », « 3d », « motion », « video », « son », « code »
    pub mode: String,
    /// Outil de l'espace : identifiant (« scene », « mat », « sim »…)
    pub tool: String,
    /// Extension du document actif (vide : aucun)
    pub ext: String,
    /// Un document est ouvert, à sa place dans le coffre, sans modification en attente
    pub saved: bool,
    pub graph: Option<GraphStats>,
    /// Calques du matériau actif (l'Atelier)
    pub mat_layers: Option<usize>,
    /// Objets de la scène 3D active, et y a-t-il un objet choisi
    pub objects: Option<usize>,
    pub selection: bool,
    /// Tête de lecture (timeline 3D ou simulation)
    pub frame: Option<i32>,
    pub text: Option<&'a str>,
    /// Calques de l'image ou du dessin vectoriel actif
    pub layers: Option<usize>,
}

/// Les noms acceptés pour un outil dans `sous_onglet = …` (les trois de la 3D sont ceux de FORMAT.md).
fn tool_names(tool: &str) -> &'static [&'static str] {
    match tool {
        "scene" => &["scene"],
        "mat" => &["materiaux", "materiau", "mat", "atelier"],
        "sim" => &["simulation", "sim", "geometrie"],
        "board" => &["moodboard", "board"],
        "image" => &["retouche", "image"],
        "vector" => &["vectoriel", "vector"],
        "comp" => &["compositing", "comp"],
        "story" => &["storyboard", "story"],
        "seq" => &["montage", "seq"],
        "review" => &["revue", "review"],
        "audio" => &["audio"],
        "music" => &["musique", "music"],
        "notes" => &["notes"],
        "tasks" => &["taches", "tasks"],
        "brain" => &["cerveau", "brain"],
        "motion" => &["compos", "motion"],
        "code" => &["code"],
        _ => &[],
    }
}

/// Les noms acceptés pour un espace dans `onglet = …`.
fn mode_matches(mode: &str, want: &str) -> bool {
    let w = crate::chat::fold(want).to_lowercase();
    mode == w
        || match mode {
            "video" => matches!(w.as_str(), "montage" | "videos"),
            "son" => matches!(w.as_str(), "audio" | "sound"),
            _ => false,
        }
}

impl Check {
    /// La vérification est-elle vraie dans cet état ? Une vérification inconnue n'est jamais vraie.
    pub fn holds(&self, s: &State) -> bool {
        match self {
            Check::Onglet(m) => mode_matches(&s.mode, m),
            Check::SousOnglet(t) => {
                let w = crate::chat::fold(t).to_lowercase();
                tool_names(&s.tool).contains(&w.as_str())
            }
            Check::FichierOuvert(e) => !s.ext.is_empty() && s.ext == *e,
            Check::FichierEnregistre => s.saved,
            Check::GrapheContient(op) => s.graph.as_ref().is_some_and(|g| g.contains(op)),
            Check::GrapheNoeuds(n) => s.graph.as_ref().is_some_and(|g| g.ops.len() >= *n),
            Check::GrapheLiens(n) => s.graph.as_ref().is_some_and(|g| g.links >= *n),
            Check::GrapheGroupe => s.graph.as_ref().is_some_and(|g| g.group),
            Check::AtelierCalques(n) => s.mat_layers.is_some_and(|l| l >= *n),
            Check::SceneObjets(n) => s.objects.is_some_and(|o| o >= *n),
            Check::SceneSelection => s.objects.is_some() && s.selection,
            Check::LectureImage(n) => s.frame.is_some_and(|f| f >= *n),
            Check::TexteContient(w) => s.text.is_some_and(|t| crate::chat::fold(t).to_lowercase().contains(&crate::chat::fold(w).to_lowercase())),
            Check::Calques(n) => s.layers.is_some_and(|l| l >= *n),
            Check::Inconnue(_) => false,
        }
    }

    /// Le moteur sait-il vérifier cette étape tout seul ?
    pub fn automatic(&self) -> bool {
        !matches!(self, Check::Inconnue(_))
    }
}

// ---------------------------------------------------------------------------------------------- progression

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct LessonProgress {
    /// Étapes cochées (rang de l'étape dans la leçon, depuis 0)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub etapes: Vec<usize>,
    /// Quiz réussis (rang du quiz dans la leçon)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quiz: Vec<usize>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub terminee: bool,
    /// Points de départ déjà créés (rang du bloc `essayer` → chemin dans le coffre)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub essais: BTreeMap<usize, String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Progress {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub lecons: BTreeMap<String, LessonProgress>,
    /// Dernière leçon ouverte (« continuer là où tu t'es arrêté »)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derniere: Option<String>,
    /// L'accueil du débutant a déjà été proposé pour ce coffre : il ne revient pas
    #[serde(default)]
    pub accueil_propose: bool,
    /// Le panneau était ouvert à la fermeture
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub panneau: bool,
    /// Vrai : il n'y avait pas de fichier de progression (premier lancement de ce coffre)
    #[serde(skip)]
    pub fresh: bool,
}

impl Progress {
    pub fn load(root: &str) -> Progress {
        match std::fs::read(std::path::Path::new(root).join(PROGRESS)) {
            Ok(b) => serde_json::from_slice(&b).unwrap_or_default(),
            Err(_) => Progress { fresh: true, ..Default::default() },
        }
    }

    pub fn save(&self, root: &str) -> Result<(), String> {
        let path = std::path::Path::new(root).join(PROGRESS);
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let mut me = self.clone();
        me.version = 1;
        let data = serde_json::to_vec_pretty(&me).map_err(|e| e.to_string())?;
        crate::vault::atomic_write(path, data).map_err(|e| e.to_string())
    }

    pub fn lesson(&self, id: &str) -> Option<&LessonProgress> {
        self.lecons.get(id)
    }

    pub fn step_done(&self, id: &str, k: usize) -> bool {
        self.lecons.get(id).is_some_and(|l| l.etapes.contains(&k))
    }

    pub fn quiz_done(&self, id: &str, k: usize) -> bool {
        self.lecons.get(id).is_some_and(|l| l.quiz.contains(&k))
    }

    /// Coche ou décoche une étape. Vrai si quelque chose a changé.
    pub fn set_step(&mut self, id: &str, k: usize, on: bool) -> bool {
        let l = self.lecons.entry(id.to_string()).or_default();
        let had = l.etapes.contains(&k);
        if on && !had {
            l.etapes.push(k);
            l.etapes.sort_unstable();
        } else if !on && had {
            l.etapes.retain(|x| *x != k);
        }
        on != had
    }

    /// Note un quiz réussi. Vrai si c'est nouveau.
    pub fn pass_quiz(&mut self, id: &str, k: usize) -> bool {
        let l = self.lecons.entry(id.to_string()).or_default();
        if l.quiz.contains(&k) {
            return false;
        }
        l.quiz.push(k);
        l.quiz.sort_unstable();
        true
    }

    /// Une leçon est terminée quand toutes ses étapes sont cochées et tous ses quiz réussis.
    /// Met à jour `terminee` (une leçon terminée le reste). Vrai si elle vient de l'être.
    pub fn update_done(&mut self, id: &str, steps: usize, quizzes: usize) -> bool {
        if steps + quizzes == 0 {
            return false;
        }
        let Some(l) = self.lecons.get_mut(id) else { return false };
        let all = (0..steps).all(|k| l.etapes.contains(&k)) && (0..quizzes).all(|k| l.quiz.contains(&k));
        if all && !l.terminee {
            l.terminee = true;
            return true;
        }
        false
    }

    /// Marque une leçon terminée à la main (une leçon sans étape ni quiz). Vrai si c'est nouveau.
    pub fn finish(&mut self, id: &str) -> bool {
        let l = self.lecons.entry(id.to_string()).or_default();
        !std::mem::replace(&mut l.terminee, true)
    }

    pub fn done(&self, id: &str) -> bool {
        self.lecons.get(id).is_some_and(|l| l.terminee)
    }

    /// Avancement d'une leçon entre 0 et 1 (1 : terminée).
    pub fn lesson_ratio(&self, l: &LessonRef) -> f32 {
        if self.done(&l.id) {
            return 1.0;
        }
        let total = l.steps + l.quizzes;
        let Some(p) = self.lecons.get(&l.id) else { return 0.0 };
        if total == 0 {
            return 0.0;
        }
        let got = p.etapes.iter().filter(|k| **k < l.steps).count() + p.quiz.iter().filter(|k| **k < l.quizzes).count();
        (got as f32 / total as f32).min(0.99)
    }

    /// Leçons terminées d'un parcours.
    pub fn course_done(&self, c: &Course) -> usize {
        c.lessons.iter().filter(|l| self.done(&l.id)).count()
    }

    pub fn course_finished(&self, c: &Course) -> bool {
        !c.lessons.is_empty() && self.course_done(c) == c.lessons.len()
    }

    /// Où reprendre : la dernière leçon ouverte si elle n'est pas finie, sinon la première leçon non
    /// terminée à sa suite (puis depuis le début). None : rien n'a été commencé, ou tout est fini.
    pub fn resume(&self, cat: &[Course]) -> Option<String> {
        let last = self.derniere.as_ref()?;
        let all: Vec<&str> = cat.iter().flat_map(|c| c.lessons.iter().map(|l| l.id.as_str())).collect();
        let at = all.iter().position(|l| l == last)?;
        if !self.done(last) {
            return Some(last.clone());
        }
        all[at + 1..].iter().chain(all[..at].iter()).find(|l| !self.done(l)).map(|l| l.to_string())
    }

    /// Oublie la progression d'une leçon (pour la refaire). Les fichiers créés restent dans le coffre.
    pub fn reset_lesson(&mut self, id: &str) {
        self.lecons.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LECON: &str = "---\ntitre: Ta première simulation de tissu\nniveau: débutant\nduree: 15 min\nprerequis: 01-demarrer/02-l-interface, 08-noeuds/01-premier-graphe.md\nobjectifs: Créer une zone de simulation ; Régler un solveur de tissu ; Lire le cache\n---\n\n# Ta première simulation de tissu\n\nUn peu de texte avec **Add** (ajouter).\n\n```essayer\nexemple: tissu sur une boule\n```\n\n```etape\nAjoute un node **Solver** dans la zone de simulation.\nverifier: graphe.contient = solveur\n```\n\n```etape\nRegarde le résultat.\n```\n\n```quiz\nQue fait la zone de simulation ?\n- [ ] Elle dessine le tissu plus vite.\n- [x] Elle garde l'état d'une image à la suivante.\n- [ ] Elle exporte la géométrie.\nexplication: Ce qui entre dans la zone repart de l'état de l'image précédente.\n```\n\n```astuce\nMaj+A ouvre la recherche de nodes sous la souris.\n```\n\n```attention\nLe cache se vide si tu changes le solveur.\n```\n\n```raccourci\nEspace : lecture\n```\n\n```defi\nFais tomber le drapeau sur la boîte au lieu de la boule.\nindice: Le node Solver a un réglage Box center.\n```\n\nFin.\n";

    #[test]
    fn en_tete() {
        let (m, body) = split_front(LECON);
        assert_eq!(m.titre, "Ta première simulation de tissu");
        assert_eq!(m.niveau, "débutant");
        assert_eq!(m.duree, "15 min");
        assert_eq!(m.prerequis, vec!["01-demarrer/02-l-interface", "08-noeuds/01-premier-graphe"]);
        assert_eq!(m.objectifs.len(), 3);
        assert_eq!(m.objectifs[1], "Régler un solveur de tissu");
        assert!(body.trim_start().starts_with("# Ta première"));
        // Sans en-tête, ou en-tête jamais refermé : tout reste du corps
        assert_eq!(split_front("# Titre\ntexte").1, "# Titre\ntexte");
        assert_eq!(split_front("---\ntitre: x\npas fini").0, Meta::default());
        assert!(split_front("---\ntitre: x\npas fini").1.contains("pas fini"));
        // Prérequis vide, fins de ligne Windows, clés avec accents
        let (m, b) = split_front("---\r\ntitre: A\r\nprérequis:\r\ndurée: 1 h 30\r\n---\r\ncorps");
        assert!(m.prerequis.is_empty() && m.titre == "A" && m.duree == "1 h 30" && b == "corps");
        assert_eq!((level_rank("débutant"), level_rank("Intermédiaire"), level_rank("avancé"), level_rank("")), (0, 1, 2, 0));
        assert_eq!((minutes("15 min"), minutes("1 h 30"), minutes("2 h"), minutes("1h05"), minutes("45"), minutes("")), (15, 90, 120, 65, 45, 0));
        assert_eq!((duration_text(45), duration_text(120), duration_text(65)), ("45 min".to_string(), "2 h".to_string(), "1 h 05".to_string()));
    }

    #[test]
    fn blocs_de_format_md() {
        let l = Lesson::parse(LECON);
        // Le titre répété en tête du corps est retiré
        assert!(matches!(&l.blocks[0], Block::Texte(t) if t.starts_with("Un peu de texte")), "{:?}", l.blocks[0]);
        assert_eq!(l.blocks[1], Block::Essayer(Ok(Source::Exemple("tissu sur une boule".into()))));
        assert_eq!(l.blocks[2], Block::Etape { texte: "Ajoute un node **Solver** dans la zone de simulation.".into(), verifier: Some(Check::GrapheContient("solveur".into())) });
        assert_eq!(l.blocks[3], Block::Etape { texte: "Regarde le résultat.".into(), verifier: None });
        match &l.blocks[4] {
            Block::Quiz { question, choix, explication } => {
                assert_eq!(question, "Que fait la zone de simulation ?");
                assert_eq!(choix.iter().map(|c| c.1).collect::<Vec<_>>(), vec![false, true, false]);
                assert_eq!(choix[1].0, "Elle garde l'état d'une image à la suivante.");
                assert!(explication.starts_with("Ce qui entre"));
            }
            b => panic!("{b:?}"),
        }
        assert_eq!(l.blocks[5], Block::Encadre { genre: Callout::Astuce, texte: "Maj+A ouvre la recherche de nodes sous la souris.".into() });
        assert!(matches!(&l.blocks[6], Block::Encadre { genre: Callout::Attention, .. }));
        assert!(matches!(&l.blocks[7], Block::Encadre { genre: Callout::Raccourci, .. }));
        assert_eq!(l.blocks[8], Block::Defi { texte: "Fais tomber le drapeau sur la boîte au lieu de la boule.".into(), indice: Some("Le node Solver a un réglage Box center.".into()) });
        assert_eq!(l.blocks[9], Block::Texte("Fin.".into()));
        assert_eq!(l.counts(), (2, 1));
    }

    #[test]
    fn sources_d_essayer() {
        let one = |s: &str| match parse_blocks(&format!("```essayer\n{s}\n```")).remove(0) {
            Block::Essayer(r) => r,
            b => panic!("{b:?}"),
        };
        // Les exemples de FORMAT.md, avec leurs commentaires entre parenthèses
        assert_eq!(one("nouveau: arygeo                       (type de fichier vide : md, aryscene…)\nnom: drapeau                          (facultatif : nom du fichier créé)"), Ok(Source::Nouveau { ext: "arygeo".into(), nom: Some("drapeau".into()) }));
        assert_eq!(one("exemple: tissu sur une boule          (exemple intégré ; liste plus bas)"), Ok(Source::Exemple("tissu sur une boule".into())));
        assert_eq!(one("fichier: fichiers/scene-depart.aryscene   (fichier fourni dans docs/apprendre/fichiers/)"), Ok(Source::Fichier("fichiers/scene-depart.aryscene".into())));
        for ext in NEW_KINDS {
            assert_eq!(one(&format!("nouveau: {ext}")), Ok(Source::Nouveau { ext: ext.into(), nom: None }));
        }
        assert_eq!(one("nouveau: .PY\nnom: mon script (essai)"), Ok(Source::Nouveau { ext: "py".into(), nom: Some("mon script (essai)".into()) }));
        // Mal écrits : pas de source, deux sources, type inconnu, ligne inconnue, chemin qui sort du dossier
        assert!(one("nom: seul").is_err());
        assert!(one("nouveau: md\nexemple: tissu sur une boule").is_err());
        assert!(one("nouveau: docx").is_err());
        assert!(one("ouvre: quelque chose").is_err());
        assert!(one("fichier: ../../Cargo.toml").is_err());
        assert!(one("").is_err());
    }

    #[test]
    fn blocs_malformes() {
        // Bloc jamais refermé : du texte
        let b = parse_blocks("avant\n```etape\nPas de fin");
        assert_eq!(b.len(), 1);
        assert!(matches!(&b[0], Block::Texte(t) if t.contains("```etape") && t.contains("Pas de fin")));
        // Mot-clé inconnu, ou bloc de code ordinaire : du texte, d'un seul tenant
        let b = parse_blocks("```rust\nfn main() {}\n```\n\n```exercice\nrien\n```");
        assert_eq!(b.len(), 1);
        assert!(matches!(&b[0], Block::Texte(t) if t.contains("fn main") && t.contains("```exercice")));
        // Un bloc de code qui cite nos mots-clés n'est pas lu comme un bloc (FORMAT.md en est plein)
        let b = parse_blocks("````\n```etape\nExemple\n```\n````\nsuite");
        assert_eq!(b.len(), 1, "{b:?}");
        // Quiz sans bonne réponse, ou avec un seul choix : du texte
        assert!(matches!(&parse_blocks("```quiz\nQ ?\n- [ ] a\n- [ ] b\n```")[0], Block::Texte(_)));
        assert!(matches!(&parse_blocks("```quiz\nQ ?\n- [x] a\n```")[0], Block::Texte(_)));
        // Étape, encadré ou défi vides : du texte
        assert!(matches!(&parse_blocks("```etape\n\n```")[0], Block::Texte(_)));
        assert!(matches!(&parse_blocks("```astuce\n```")[0], Block::Texte(_)));
        assert!(matches!(&parse_blocks("```defi\nindice: seul\n```")[0], Block::Texte(_)));
        // Étape dont il ne reste que la vérification : du texte aussi
        assert!(matches!(&parse_blocks("```etape\nverifier: fichier.enregistre\n```")[0], Block::Texte(_)));
        // Quiz à plusieurs bonnes réponses, sans explication, mot-clé avec accent et majuscule
        match &parse_blocks("``` Étape\nFais.\n```\n```quiz\nLesquels ?\n- [x] a\n- [X] b\n- [ ] c\n```")[..] {
            [Block::Etape { texte, verifier: None }, Block::Quiz { choix, explication, .. }] => {
                assert_eq!(texte, "Fais.");
                assert_eq!(choix.iter().filter(|c| c.1).count(), 2);
                assert!(explication.is_empty());
            }
            b => panic!("{b:?}"),
        }
        // Défi sans indice ; indice sur plusieurs lignes
        assert_eq!(parse_blocks("```defi\nFais mieux.\n```")[0], Block::Defi { texte: "Fais mieux.".into(), indice: None });
        assert_eq!(parse_blocks("```defi\nA\nindice: un\ndeux\n```")[0], Block::Defi { texte: "A".into(), indice: Some("un\ndeux".into()) });
        // Rien du tout
        assert!(parse_blocks("").is_empty() && parse_blocks("\n\n").is_empty());
    }

    #[test]
    fn lecture_des_verifications() {
        let table = [
            ("onglet = 3d", Check::Onglet("3d".into())),
            ("sous_onglet = simulation", Check::SousOnglet("simulation".into())),
            ("fichier.ouvert = arygeo", Check::FichierOuvert("arygeo".into())),
            ("fichier.ouvert = .MD", Check::FichierOuvert("md".into())),
            ("fichier.enregistre", Check::FichierEnregistre),
            ("graphe.contient = sim_solver", Check::GrapheContient("sim_solver".into())),
            ("graphe.noeuds >= 4", Check::GrapheNoeuds(4)),
            ("graphe.nœuds ≥ 4", Check::GrapheNoeuds(4)),
            ("graphe.liens >= 3", Check::GrapheLiens(3)),
            ("graphe.liens>=3", Check::GrapheLiens(3)),
            ("graphe.groupe", Check::GrapheGroupe),
            ("atelier.calques >= 2", Check::AtelierCalques(2)),
            ("scene.objets >= 4", Check::SceneObjets(4)),
            ("scène.objets >= 4", Check::SceneObjets(4)),
            ("scene.selection", Check::SceneSelection),
            ("lecture.image >= 24", Check::LectureImage(24)),
            ("texte.contient = Bonjour", Check::TexteContient("Bonjour".into())),
            ("calques >= 2", Check::Calques(2)),
        ];
        for (raw, want) in table {
            assert_eq!(parse_check(raw), want, "{raw}");
            assert!(want.automatic());
        }
        // Inconnues ou mal écrites : case à cocher à la main
        for raw in ["camera.active", "graphe.noeuds >= beaucoup", "onglet =", "graphe.noeuds = 4", "scene.objets", "fichier.enregistre = oui", "n'importe quoi"] {
            assert!(matches!(parse_check(raw), Check::Inconnue(r) if r == raw), "{raw}");
        }
        assert!(!Check::Inconnue("x".into()).automatic() && !Check::Inconnue("x".into()).holds(&State::default()));
        // `verifier:` vide : pas de vérification
        assert_eq!(parse_blocks("```etape\nFais.\nverifier:\n```")[0], Block::Etape { texte: "Fais.".into(), verifier: None });
    }

    fn graph_stats() -> GraphStats {
        let mut g = crate::geo::example("tissu sur une boule").unwrap();
        g.groups.push(crate::nodeflow::Group { name: "g".into(), inputs: vec![], outputs: vec![], nodes: vec![], links: vec![], frames: vec![] });
        let mut s = GraphStats::default();
        s.add_graph(&g, None);
        s
    }

    #[test]
    fn chaque_verification_sur_un_etat_simule() {
        let vide = State::default();
        // onglet
        let s = State { mode: "3d".into(), tool: "sim".into(), ..Default::default() };
        assert!(Check::Onglet("3d".into()).holds(&s) && Check::Onglet("3D".into()).holds(&s) && !Check::Onglet("2d".into()).holds(&s) && !Check::Onglet("3d".into()).holds(&vide));
        assert!(Check::Onglet("vidéo".into()).holds(&State { mode: "video".into(), ..Default::default() }));
        // sous_onglet
        assert!(Check::SousOnglet("simulation".into()).holds(&s) && !Check::SousOnglet("scene".into()).holds(&s) && !Check::SousOnglet("simulation".into()).holds(&vide));
        assert!(Check::SousOnglet("matériaux".into()).holds(&State { tool: "mat".into(), ..Default::default() }));
        assert!(Check::SousOnglet("scene".into()).holds(&State { tool: "scene".into(), ..Default::default() }));
        // fichier.ouvert
        let s = State { ext: "arygeo".into(), saved: true, ..Default::default() };
        assert!(Check::FichierOuvert("arygeo".into()).holds(&s) && !Check::FichierOuvert("md".into()).holds(&s) && !Check::FichierOuvert("md".into()).holds(&vide));
        // fichier.enregistre
        assert!(Check::FichierEnregistre.holds(&s) && !Check::FichierEnregistre.holds(&State { ext: "arygeo".into(), ..Default::default() }) && !Check::FichierEnregistre.holds(&vide));
        // graphe.*
        let g = graph_stats();
        let n = g.ops.len();
        let links = g.links;
        assert!(n >= 5 && links >= 4);
        let s = State { graph: Some(g), ..Default::default() };
        for name in ["sim_solver", "solver", "solveur", "Solveur", "geo_grid", "grille", "Zone de simulation", "sim_in"] {
            assert!(Check::GrapheContient(name.into()).holds(&s), "{name}");
        }
        assert!(!Check::GrapheContient("sim_emit".into()).holds(&s) && !Check::GrapheContient("".into()).holds(&s) && !Check::GrapheContient("sim_solver".into()).holds(&vide));
        assert!(Check::GrapheNoeuds(n).holds(&s) && !Check::GrapheNoeuds(n + 1).holds(&s) && !Check::GrapheNoeuds(0).holds(&vide));
        assert!(Check::GrapheLiens(links).holds(&s) && !Check::GrapheLiens(links + 1).holds(&s) && !Check::GrapheLiens(0).holds(&vide));
        assert!(Check::GrapheGroupe.holds(&s) && !Check::GrapheGroupe.holds(&vide));
        let mut plain = GraphStats::default();
        plain.add_graph(&crate::geo::example("vagues animées").unwrap(), None);
        assert!(!Check::GrapheGroupe.holds(&State { graph: Some(plain), ..Default::default() }));
        // atelier.calques
        let s = State { mat_layers: Some(2), ..Default::default() };
        assert!(Check::AtelierCalques(2).holds(&s) && !Check::AtelierCalques(3).holds(&s) && !Check::AtelierCalques(0).holds(&vide));
        // scene.*
        let s = State { objects: Some(3), selection: true, ..Default::default() };
        assert!(Check::SceneObjets(3).holds(&s) && !Check::SceneObjets(4).holds(&s) && !Check::SceneObjets(0).holds(&vide));
        assert!(Check::SceneSelection.holds(&s) && !Check::SceneSelection.holds(&State { objects: Some(3), ..Default::default() }) && !Check::SceneSelection.holds(&State { selection: true, ..Default::default() }));
        // lecture.image
        let s = State { frame: Some(24), ..Default::default() };
        assert!(Check::LectureImage(24).holds(&s) && Check::LectureImage(10).holds(&s) && !Check::LectureImage(25).holds(&s) && !Check::LectureImage(0).holds(&vide));
        // texte.contient
        let s = State { text: Some("# Idées\n\nBonjour le Québec"), ..Default::default() };
        assert!(Check::TexteContient("bonjour".into()).holds(&s) && Check::TexteContient("quebec".into()).holds(&s) && !Check::TexteContient("adieu".into()).holds(&s) && !Check::TexteContient("bonjour".into()).holds(&vide));
        // calques
        let s = State { layers: Some(2), ..Default::default() };
        assert!(Check::Calques(2).holds(&s) && !Check::Calques(3).holds(&s) && !Check::Calques(0).holds(&vide));
    }

    #[test]
    fn graphe_tous_niveaux() {
        // Un matériau à calques : les nodes des graphes de ses calques comptent, sauf le calque qu'on dit de sauter
        let mut inner = crate::nodes::Graph::new_material();
        let before = inner.nodes.len();
        inner.add("noise", [0.0, 0.0]);
        let mut outer = crate::atelier::to_stack(crate::nodes::Graph::new_material());
        let stack = outer.stack.get_or_insert_with(Default::default);
        stack.layers.push(crate::atelier::Layer::own("rouille", inner));
        let k = stack.layers.len() - 1;
        let mut all = GraphStats::default();
        all.add_graph(&outer, None);
        let mut skipped = GraphStats::default();
        skipped.add_graph(&outer, Some(k));
        assert_eq!(all.ops.len() - skipped.ops.len(), before + 1);
        assert!(all.contains("noise") && all.contains("bruit"));
    }

    fn cat() -> Vec<Course> {
        catalog_from(&[
            ("apprendre/README.md", "# Sommaire"),
            ("apprendre/FORMAT.md", "# Format"),
            ("apprendre/02-notes/01-a.md", "---\ntitre: A\nduree: 10 min\n---\n```etape\nx\n```\n```quiz\nQ\n- [x] a\n- [ ] b\n```"),
            ("apprendre/01-demarrer/index.md", "---\ntitre: Démarrer\nniveau: débutant\n---\nBienvenue."),
            ("apprendre/01-demarrer/02-b.md", "---\ntitre: B\nduree: 5 min\n---\ntexte"),
            ("apprendre/01-demarrer/01-a.md", "---\nduree: 20 min\n---\n```etape\nx\n```\n```etape\ny\n```"),
            ("apprendre/fichiers/lisez-moi.md", "pas une leçon"),
            ("apprendre/03-vide/index.md", "---\ntitre: Vide\n---"),
            ("utilisateur/notes.md", "# Notes"),
        ])
    }

    #[test]
    fn catalogue() {
        let c = cat();
        assert_eq!(c.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), vec!["01-demarrer", "02-notes"]);
        assert_eq!(c[0].meta.titre, "Démarrer");
        assert_eq!(c[1].meta.titre, "Notes");
        assert_eq!(c[0].lessons.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(), vec!["01-demarrer/01-a", "01-demarrer/02-b"]);
        assert_eq!(c[0].lessons[0].meta.titre, "A");
        assert_eq!((c[0].lessons[0].steps, c[0].lessons[0].quizzes), (2, 0));
        assert_eq!(c[0].minutes(), 25);
        assert_eq!(neighbours(&c, "01-demarrer/01-a"), (None, Some("01-demarrer/02-b".into())));
        assert_eq!(neighbours(&c, "01-demarrer/02-b"), (Some("01-demarrer/01-a".into()), Some("02-notes/01-a".into())));
        assert_eq!(neighbours(&c, "02-notes/01-a"), (Some("01-demarrer/02-b".into()), None));
        assert_eq!(neighbours(&c, "inconnue"), (None, None));
        assert_eq!(pretty("01-demarrer/02-l-interface"), "L interface");
    }

    #[test]
    fn lecons_embarquees_lisibles() {
        // Toutes les leçons du dépôt se lisent, leurs blocs `essayer` sont bien écrits et leurs sources existent
        let mut bad = vec![];
        for c in catalog() {
            for l in &c.lessons {
                let lesson = Lesson::parse(page_text(&l.id).unwrap());
                assert!(!lesson.blocks.is_empty(), "{} est vide", l.id);
                for b in &lesson.blocks {
                    if let Block::Essayer(r) = b {
                        match r {
                            Err(e) => bad.push(format!("{} : {e}", l.id)),
                            Ok(src) => {
                                if let Err(e) = target_of(src, &l.id) {
                                    bad.push(format!("{} : {e}", l.id));
                                }
                            }
                        }
                    }
                }
                for p in &l.meta.prerequis {
                    if page_text(p).is_none() {
                        bad.push(format!("{} : prérequis introuvable {p}", l.id));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    #[test]
    fn progression() {
        let c = cat();
        let mut p = Progress::default();
        assert_eq!(p.resume(&c), None);
        assert!(p.set_step("01-demarrer/01-a", 1, true) && !p.set_step("01-demarrer/01-a", 1, true));
        assert!(p.step_done("01-demarrer/01-a", 1) && !p.step_done("01-demarrer/01-a", 0));
        assert!(!p.update_done("01-demarrer/01-a", 2, 0));
        assert!((p.lesson_ratio(&c[0].lessons[0]) - 0.5).abs() < 1e-6);
        assert!(p.set_step("01-demarrer/01-a", 0, true));
        assert!(p.update_done("01-demarrer/01-a", 2, 0) && !p.update_done("01-demarrer/01-a", 2, 0));
        assert!(p.done("01-demarrer/01-a"));
        assert_eq!(p.lesson_ratio(&c[0].lessons[0]), 1.0);
        // Décocher une étape ne retire pas « terminée »
        assert!(p.set_step("01-demarrer/01-a", 0, false));
        p.update_done("01-demarrer/01-a", 2, 0);
        assert!(p.done("01-demarrer/01-a"));
        // Reprise : la dernière leçon si elle n'est pas finie, sinon la suivante non terminée
        p.derniere = Some("01-demarrer/01-a".into());
        assert_eq!(p.resume(&c).as_deref(), Some("01-demarrer/02-b"));
        p.derniere = Some("02-notes/01-a".into());
        assert_eq!(p.resume(&c).as_deref(), Some("02-notes/01-a"));
        // Leçon sans étape ni quiz : terminée à la main
        assert!(!p.update_done("01-demarrer/02-b", 0, 0));
        assert!(p.finish("01-demarrer/02-b") && !p.finish("01-demarrer/02-b"));
        assert_eq!(p.course_done(&c[0]), 2);
        assert!(p.course_finished(&c[0]) && !p.course_finished(&c[1]));
        // Étape et quiz : les deux comptent
        p.set_step("02-notes/01-a", 0, true);
        assert!(!p.update_done("02-notes/01-a", 1, 1));
        assert!(p.pass_quiz("02-notes/01-a", 0) && !p.pass_quiz("02-notes/01-a", 0) && p.quiz_done("02-notes/01-a", 0));
        assert!(p.update_done("02-notes/01-a", 1, 1));
        // Tout est fini : plus rien à reprendre
        assert_eq!(p.resume(&c), None);
        p.reset_lesson("02-notes/01-a");
        assert_eq!(p.resume(&c).as_deref(), Some("02-notes/01-a"));
        // Une leçon retirée du catalogue ne casse pas la reprise
        p.derniere = Some("99-ancien/01-x".into());
        assert_eq!(p.resume(&c), None);
    }

    #[test]
    fn progression_sur_le_disque() {
        let dir = std::env::temp_dir().join(format!("aryadne-apprendre-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.to_string_lossy().to_string();
        // Premier lancement du coffre : pas de fichier
        let mut p = Progress::load(&root);
        assert!(p.fresh && !p.accueil_propose);
        p.accueil_propose = true;
        p.derniere = Some("01-demarrer/01-a".into());
        p.set_step("01-demarrer/01-a", 0, true);
        p.pass_quiz("01-demarrer/01-a", 2);
        p.lecons.get_mut("01-demarrer/01-a").unwrap().essais.insert(1, "Apprendre/drapeau.arygeo".into());
        p.save(&root).unwrap();
        let back = Progress::load(&root);
        assert!(!back.fresh && back.accueil_propose && back.version == 1);
        assert_eq!(back.derniere, p.derniere);
        assert_eq!(back.lecons, p.lecons);
        // Fichier abîmé : on repart de zéro sans planter, et sans reproposer l'accueil à chaque fois
        std::fs::write(dir.join(PROGRESS), "{ pas du json").unwrap();
        let broken = Progress::load(&root);
        assert!(!broken.fresh && broken.lecons.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn points_de_depart() {
        let dir = std::env::temp_dir().join(format!("aryadne-essayer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.to_string_lossy().to_string();
        // Chaque type de fichier vide se crée, sous le nom de la leçon ou celui demandé
        for ext in NEW_KINDS {
            let rel = start(&root, &Source::Nouveau { ext: ext.into(), nom: Some("essai".into()) }, "01-demarrer/01-a").unwrap();
            assert_eq!(rel, format!("Apprendre/essai.{ext}"));
            assert!(dir.join(&rel).exists(), "{ext}");
        }
        assert_eq!(start(&root, &Source::Nouveau { ext: "md".into(), nom: None }, "02-notes/03-ma-premiere-note").unwrap(), "Apprendre/ma premiere note.md");
        // Jamais d'écrasement : la deuxième copie prend un numéro
        std::fs::write(dir.join("Apprendre/essai.md"), "mon travail").unwrap();
        assert_eq!(start(&root, &Source::Nouveau { ext: "md".into(), nom: Some("essai".into()) }, "x").unwrap(), "Apprendre/essai 2.md");
        assert_eq!(std::fs::read_to_string(dir.join("Apprendre/essai.md")).unwrap(), "mon travail");
        // Chaque exemple intégré s'écrit et se relit
        for (name, ext) in examples() {
            let rel = start(&root, &Source::Exemple(name.into()), "x").unwrap();
            assert!(rel.ends_with(&format!(".{ext}")), "{name}");
            if matches!(ext, "arygeo" | "arymat") {
                assert!(crate::nodes::Graph::load(&dir.join(&rel)).is_ok(), "{name}");
            }
        }
        assert!(find_example("Tissu sur une BOULE").is_some() && find_example("metal rouille").is_some());
        assert!(start(&root, &Source::Exemple("inconnu".into()), "x").is_err());
        assert!(start(&root, &Source::Fichier("fichiers/absent.aryscene".into()), "x").is_err());
        // Un nom libre ne sort pas du dossier
        assert_eq!(safe_name("../a/b:c"), "a b c");
        assert_eq!(safe_name("  "), "exercice");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
