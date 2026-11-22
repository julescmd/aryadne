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
        // Se relit à l'identique
        let back: Layout = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(panes(&back).len(), 5);
    }
}
