//! Disposition en cases, comme Blender ou Premiere : chaque panneau (fichiers, document, structure,
//! assistant, terminal, pages ajoutées…) est un onglet qu'on glisse pour diviser une zone, l'empiler
//! avec d'autres ou la déplacer. Chaque espace de travail garde la sienne.
use egui_tiles::{Container, Tile, TileId, Tiles, Tree};

pub type Layout = Tree<String>;

/// Le document (ou la page) au centre : il ne se ferme pas.
pub const MAIN: &str = "main";

/// Disposition de départ : fichiers à gauche, document au centre (terminal dessous), structure et assistant à droite.
pub fn default_layout(explorer_left: bool) -> Layout {
    let mut tiles: Tiles<String> = Tiles::default();
    let explorer = tiles.insert_pane("explorer".to_string());
    let explorer = tiles.insert_tab_tile(vec![explorer]);
    let main = tiles.insert_pane(MAIN.to_string());
    let main = tiles.insert_tab_tile(vec![main]);
    let term = tiles.insert_pane("terminal".to_string());
    let term = tiles.insert_tab_tile(vec![term]);
    let center = tiles.insert_vertical_tile(vec![main, term]);
    let side = tiles.insert_pane("side".to_string());
    let assistant = tiles.insert_pane("assistant".to_string());
    let right = tiles.insert_tab_tile(vec![side, assistant]);
    let children = if explorer_left { vec![explorer, center, right] } else { vec![center, right] };
    let root = tiles.insert_horizontal_tile(children);
    if let Some(Tile::Container(Container::Linear(l))) = tiles.get_mut(root) {
        l.shares.set_share(explorer, 1.0);
        l.shares.set_share(center, 4.0);
        l.shares.set_share(right, 1.2);
    }
    if let Some(Tile::Container(Container::Linear(l))) = tiles.get_mut(center) {
        l.shares.set_share(main, 3.0);
        l.shares.set_share(term, 1.0);
    }
    Tree::new(eframe::egui::Id::unique("aryadne-layout"), root, tiles)
}

/// La case d'un panneau.
pub fn find(tree: &Layout, id: &str) -> Option<TileId> {
    tree.tiles.find_pane(&id.to_string())
}

/// Les panneaux de la disposition.
pub fn panes(tree: &Layout) -> Vec<String> {
    tree.tiles.tiles().filter_map(|t| if let Tile::Pane(p) = t { Some(p.clone()) } else { None }).collect()
}

/// Ajoute un panneau : en onglet à côté de `near` s'il est là, sinon à droite de tout.
pub fn add(tree: &mut Layout, id: &str, near: &str) {
    if find(tree, id).is_some() {
        return;
    }
    let new = tree.tiles.insert_pane(id.to_string());
    if let Some(n) = find(tree, near) {
        if let Some(parent) = tree.tiles.parent_of(n) {
            if let Some(Tile::Container(c)) = tree.tiles.get_mut(parent) {
                c.add_child(new);
                if let Container::Tabs(t) = c {
                    t.set_active(new);
                }
                return;
            }
        }
    }
    let tabs = tree.tiles.insert_tab_tile(vec![new]);
    match tree.root() {
        Some(root) => {
            if let Some(Tile::Container(c)) = tree.tiles.get_mut(root) {
                c.add_child(tabs);
            }
        }
        None => tree.root = Some(tabs),
    }
}

/// Passe l'onglet d'un panneau devant ceux qui partagent sa case.
pub fn activate(tree: &mut Layout, id: &str) {
    let Some(t) = find(tree, id) else { return };
    if let Some(parent) = tree.tiles.parent_of(t) {
        if let Some(Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(parent) {
            tabs.set_active(t);
        }
    }
}

/// Donne au moins `min` points de large à la colonne d'un panneau, en prenant sur ses voisines
/// (jamais plus de la moitié de la rangée). Faux tant que sa place n'est pas connue.
pub fn widen(tree: &mut Layout, id: &str, min: f32) -> bool {
    let Some(mut t) = find(tree, id) else { return false };
    loop {
        let Some(parent) = tree.tiles.parent_of(t) else { return true };
        let (Some(w), Some(total)) = (tree.tiles.rect(t).map(|r| r.width()), tree.tiles.rect(parent).map(|r| r.width())) else { return false };
        let visible: Vec<TileId> = match tree.tiles.get(parent) {
            Some(Tile::Container(c)) => c.children().copied().filter(|k| tree.tiles.is_visible(*k)).collect(),
            _ => vec![],
        };
        if let Some(Tile::Container(Container::Linear(l))) = tree.tiles.get_mut(parent) {
            if l.dir == egui_tiles::LinearDir::Horizontal && visible.len() > 1 {
                let min = min.min(total * 0.5);
                if w + 1.0 < min && total > min {
                    let sum: f32 = visible.iter().map(|k| l.shares[*k]).sum();
                    let mine = l.shares[t];
                    l.shares.set_share(t, (sum - mine) * min / (total - min));
                }
                return true;
            }
        }
        t = parent;
    }
}

/// Retire un panneau de la disposition.
pub fn remove(tree: &mut Layout, id: &str) {
    if let Some(t) = find(tree, id) {
        tree.remove_recursively(t);
    }
}

/// Montre ou cache chaque panneau ; une rangée d'onglets dont tout est caché disparaît aussi.
pub fn set_visibility(tree: &mut Layout, visible: &dyn Fn(&str) -> bool) {
    let ids: Vec<(TileId, String)> = tree.tiles.iter().filter_map(|(id, t)| if let Tile::Pane(p) = t { Some((*id, p.clone())) } else { None }).collect();
    for (id, p) in &ids {
        tree.tiles.set_visible(*id, visible(p));
    }
    // Les conteneurs, du plus profond au plus haut : visibles s'il leur reste un enfant visible
    for _ in 0..6 {
        let conts: Vec<(TileId, Vec<TileId>)> = tree.tiles.iter().filter_map(|(id, t)| if let Tile::Container(c) = t { Some((*id, c.children().copied().collect())) } else { None }).collect();
        for (id, kids) in conts {
            let any = kids.iter().any(|k| tree.tiles.is_visible(*k));
            if Some(id) != tree.root() {
                tree.tiles.set_visible(id, any);
            }
            // Un onglet actif caché : on passe au premier visible
            let first_visible = kids.iter().copied().find(|k| tree.tiles.is_visible(*k));
            let active_ok = match tree.tiles.get(id) {
                Some(Tile::Container(Container::Tabs(t))) => t.active.is_some_and(|a| kids.contains(&a) && tree.tiles.is_visible(a)),
                _ => true,
            };
            if !active_ok {
                if let (Some(f), Some(Tile::Container(Container::Tabs(t)))) = (first_visible, tree.tiles.get_mut(id)) {
                    t.set_active(f);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disposition() {
        let mut t = default_layout(true);
        assert!(find(&t, MAIN).is_some() && find(&t, "explorer").is_some());
        add(&mut t, "view:brain#1", "side");
        let p = panes(&t);
        assert!(p.contains(&"view:brain#1".to_string()) && p.len() == 6);
        // Le panneau ajouté est l'onglet actif de la colonne de droite
        let right = t.tiles.parent_of(find(&t, "side").unwrap()).unwrap();
        assert_eq!(t.tiles.parent_of(find(&t, "view:brain#1").unwrap()), Some(right));
        set_visibility(&mut t, &|p| p != "terminal" && p != "explorer");
        let term_tabs = t.tiles.parent_of(find(&t, "terminal").unwrap()).unwrap();
        assert!(!t.tiles.is_visible(term_tabs));
        remove(&mut t, "view:brain#1");
        assert_eq!(panes(&t).len(), 5);
        // Un panneau ajouté plus tard passe devant quand on le demande
        add(&mut t, "learn", "side");
        activate(&mut t, "side");
        let tabs = |t: &Layout| match t.tiles.get(t.tiles.parent_of(find(t, "learn").unwrap()).unwrap()) {
            Some(Tile::Container(Container::Tabs(tb))) => tb.active,
            _ => None,
        };
        assert_eq!(tabs(&t), find(&t, "side"));
        activate(&mut t, "learn");
        assert_eq!(tabs(&t), find(&t, "learn"));
        // Sans place connue (jamais dessiné), on ne touche pas aux largeurs
        assert!(!widen(&mut t, "learn", 380.0));
        remove(&mut t, "learn");
        // Se relit à l'identique
        let back: Layout = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(panes(&back).len(), 5);
    }
}

// ---------------------------------------------------------------------- panneaux des éditeurs
//
// Un éditeur (3D, montage, retouche…) propose ses panneaux (outliner, propriétés, timeline, projet…).
// Rangé dans la disposition, un tel panneau a sa propre case : l'éditeur y dessine son contenu
// au lieu de son panneau intérieur. Sinon, il reste dans l'éditeur, comme avant.

use eframe::egui;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

thread_local! {
    static OFFERED: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    static OFFERED_LAST: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    static RECTS: RefCell<HashMap<String, egui::Rect>> = RefCell::new(HashMap::new());
    static DOCKED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    static TITLES: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// Identifiant de case d'un panneau d'éditeur.
pub fn editor_pane(id: &str) -> String {
    format!("ed:{id}")
}

/// Où dessiner un panneau d'éditeur sorti de son éditeur.
#[derive(Clone)]
pub struct Slot {
    /// Identifiant de case (« ed:… »)
    pub key: String,
    /// Clé française du titre
    pub title: String,
    /// Sa case dans la disposition ; None : il est dans sa propre fenêtre du système
    pub rect: Option<egui::Rect>,
}

/// L'éditeur propose ce panneau cette image-ci. Renvoie où le dessiner s'il n'est pas dans l'éditeur :
/// rangé dans une case de la disposition, ou détaché dans sa propre fenêtre.
pub fn offer(id: &str, title: &str) -> Option<Slot> {
    let key = editor_pane(id);
    OFFERED.with(|o| o.borrow_mut().insert(key.clone(), title.to_string()));
    TITLES.with(|t| t.borrow_mut().insert(key.clone(), title.to_string()));
    if crate::dock::is_out(&key) {
        return Some(Slot { key, title: title.to_string(), rect: None });
    }
    let rect = if DOCKED.with(|d| d.borrow().contains(&key)) { RECTS.with(|r| r.borrow().get(&key).copied()) } else { None };
    rect.map(|r| Slot { key, title: title.to_string(), rect: Some(r) })
}

/// Dessine `body` là où se trouve le panneau d'éditeur : dans sa case, ou dans sa fenêtre.
pub fn draw_in<R>(ui: &mut egui::Ui, slot: Slot, fill: egui::Color32, margin: f32, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    crate::dock::slot(ui, slot, fill, margin, body)
}

/// Panneaux proposés à l'image précédente (id de case → titre), et début d'une nouvelle image.
pub fn offered_take() -> HashMap<String, String> {
    let now = OFFERED.with(|o| std::mem::take(&mut *o.borrow_mut()));
    OFFERED_LAST.with(|l| *l.borrow_mut() = now.clone());
    now
}

/// Panneaux proposés par l'éditeur affiché (pour le menu « fenêtre »).
pub fn offered() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = OFFERED_LAST.with(|l| l.borrow().iter().map(|(a, b)| (a.clone(), b.clone())).collect());
    v.sort();
    v
}

pub fn set_docked(ids: HashSet<String>) {
    DOCKED.with(|d| *d.borrow_mut() = ids);
}

pub fn set_rect(id: &str, r: egui::Rect) {
    RECTS.with(|m| m.borrow_mut().insert(id.to_string(), r));
}

pub fn title(id: &str) -> Option<String> {
    TITLES.with(|t| t.borrow().get(id).cloned())
}
