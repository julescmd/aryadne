//! Rig automatique d'un bipède : on pose des repères sur le personnage (bassin, épaule, coude,
//! genou…), côté gauche seulement, puis le squelette est construit des deux côtés avec ses cibles
//! de cinématique inverse aux mains et aux pieds, et le modèle lui est lié.

use crate::scene::Scene;

type V3 = [f32; 3];

/// Le groupe qui porte les repères dans la scène
pub const GROUP: &str = "repères de rig";

/// Les repères : nom dans la scène, puis place par défaut dans la boîte du personnage
/// (x : écart au milieu en part de la hauteur, y : part de la hauteur, z : vers l'avant en part de la hauteur)
pub const MARKERS: [(&str, [f32; 3]); 11] = [
    ("rig bassin", [0.0, 0.53, 0.0]),
    ("rig poitrine", [0.0, 0.72, 0.0]),
    ("rig cou", [0.0, 0.84, 0.0]),
    ("rig tête", [0.0, 0.98, 0.0]),
    ("rig épaule", [0.105, 0.80, 0.0]),
    ("rig coude", [0.14, 0.62, -0.01]),
    ("rig poignet", [0.16, 0.46, 0.0]),
    ("rig hanche", [0.055, 0.50, 0.0]),
    ("rig genou", [0.055, 0.28, 0.012]),
    ("rig cheville", [0.055, 0.05, 0.0]),
    ("rig orteils", [0.055, 0.008, 0.09]),
];

/// Place par défaut de chaque repère pour un personnage qui tient dans la boîte (`lo`, `hi`), debout
/// sur Y et tourné vers +Z. Bras écartés (pose en T) si la boîte est large, le long du corps sinon.
pub fn default_markers(lo: V3, hi: V3) -> Vec<(&'static str, V3)> {
    let ok = lo.iter().chain(&hi).all(|x| x.is_finite()) && hi[1] - lo[1] > 1e-4;
    let (lo, hi) = if ok { (lo, hi) } else { ([-0.9, 0.0, -0.2], [0.9, 1.8, 0.2]) };
    let h = hi[1] - lo[1];
    let (cx, cz) = ((lo[0] + hi[0]) * 0.5, (lo[2] + hi[2]) * 0.5);
    let half = (hi[0] - lo[0]) * 0.5;
    let t_pose = half > 0.35 * h;
    MARKERS
        .iter()
        .map(|&(name, [x, y, z])| {
            let mut p = [cx + x * h, lo[1] + y * h, cz + z * h];
            if t_pose {
                // Bras à l'horizontale : le coude à 45 % et le poignet à 85 % du chemin de l'épaule au bout des doigts
                let shoulder = cx + 0.105 * h;
                let reach = (cx + half - shoulder).max(0.05 * h);
                match name {
                    "rig coude" => p = [shoulder + reach * 0.45, lo[1] + 0.80 * h, cz - 0.01 * h],
                    "rig poignet" => p = [shoulder + reach * 0.85, lo[1] + 0.80 * h, cz],
                    _ => {}
                }
            }
            (name, p)
        })
        .collect()
}

/// Pose les repères dans la scène (des groupes vides sous « repères de rig »), à leur place par
/// défaut pour la boîte donnée. Ceux qui existent déjà sont remis en place. Rend l'indice du groupe.
pub fn place_markers(scene: &mut Scene, lo: V3, hi: V3) -> usize {
    let group = match scene.objects.iter().position(|o| o.name == GROUP) {
        Some(g) => g,
        None => scene.add_role(GROUP, ""),
    };
    (scene.objects[group].pos, scene.objects[group].rot, scene.objects[group].scale, scene.objects[group].visible) = ([0.0; 3], [0.0; 3], [1.0; 3], true);
    for (name, p) in default_markers(lo, hi) {
        let k = match scene.objects.iter().position(|o| o.name == name) {
            Some(k) => k,
            None => scene.add_role(name, ""),
        };
        let o = &mut scene.objects[k];
        (o.parent, o.pos, o.visible) = (GROUP.to_string(), p, true);
    }
    group
}

/// Les repères posés, à leur place dans la scène (None s'il en manque un)
pub fn read_markers(scene: &Scene) -> Option<Vec<(&'static str, V3)>> {
    MARKERS.iter().map(|&(name, _)| scene.objects.iter().position(|o| o.name == name).map(|k| (name, scene.world(k, [0.0; 3])))).collect()
}

/// Ce que la génération a créé
#[derive(Debug, Clone, PartialEq)]
pub struct Rig {
    pub root: usize,
    pub bones: usize,
    /// Cibles de cinématique inverse : mains et pieds, gauche puis droite
    pub targets: Vec<usize>,
}

/// Construit le squelette d'après les repères (ceux du côté gauche sont reflétés à droite, autour du
/// bassin), avec les cibles de cinématique inverse des mains et des pieds.
pub fn build(scene: &mut Scene, markers: &[(&str, V3)]) -> Result<Rig, String> {
    let get = |name: &str| markers.iter().find(|m| m.0 == name).map(|m| m.1).filter(|p| p.iter().all(|x| x.is_finite()));
    let missing = || crate::i18n::t("Il manque des repères de rig : pose-les d'abord (Ajouter › rig bipède › poser les repères).");
    let m = |name: &str| get(name).ok_or_else(missing);
    let (pelvis, chest, neck, head) = (m("rig bassin")?, m("rig poitrine")?, m("rig cou")?, m("rig tête")?);
    let (shoulder, elbow, wrist) = (m("rig épaule")?, m("rig coude")?, m("rig poignet")?);
    let (hip, knee, ankle, toes) = (m("rig hanche")?, m("rig genou")?, m("rig cheville")?, m("rig orteils")?);
    let d = |a: V3, b: V3| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    if d(pelvis, head) < 1e-3 {
        return Err(crate::i18n::t("Les repères de rig sont tous au même endroit."));
    }
    let n0 = scene.objects.len();
    let mid = |a: V3, b: V3| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5];
    let bone = crate::rig::add_bone_between;
    // Le tronc : deux os du bassin à la poitrine, le cou, la tête
    let waist = mid(pelvis, chest);
    let root = bone(scene, "bassin", None, pelvis, waist);
    let spine = bone(scene, "colonne", Some(root), waist, chest);
    let neck_b = bone(scene, "cou", Some(spine), chest, neck);
    bone(scene, "tête", Some(neck_b), neck, head);
    let mut targets = vec![];
    let mut hands = vec![];
    let mut feet = vec![];
    for (side, sx) in [("G", 1.0f32), ("D", -1.0)] {
        // Le côté droit est le reflet du gauche par rapport au plan du bassin
        let s = |p: V3| [pelvis[0] + (p[0] - pelvis[0]) * sx, p[1], p[2]];
        let (sh, el, wr) = (s(shoulder), s(elbow), s(wrist));
        let clav = bone(scene, &format!("clavicule.{side}"), Some(spine), [chest[0], sh[1], chest[2]], sh);
        let arm = bone(scene, &format!("bras.{side}"), Some(clav), sh, el);
        let fore = bone(scene, &format!("avant-bras.{side}"), Some(arm), el, wr);
        // La main prolonge l'avant-bras, sur un quart de sa longueur
        let k = 0.25;
        bone(scene, &format!("main.{side}"), Some(fore), wr, [wr[0] + (wr[0] - el[0]) * k, wr[1] + (wr[1] - el[1]) * k, wr[2] + (wr[2] - el[2]) * k]);
        hands.push((fore, wr, side));
        let (hp, kn, an, to) = (s(hip), s(knee), s(ankle), s(toes));
        let thigh = bone(scene, &format!("cuisse.{side}"), Some(root), hp, kn);
        let shin = bone(scene, &format!("tibia.{side}"), Some(thigh), kn, an);
        bone(scene, &format!("pied.{side}"), Some(shin), an, to);
        feet.push((shin, an, side));
    }
    // Cibles de cinématique inverse : l'avant-bras et le tibia tirent leur chaîne de deux os vers elles
    for (what, list) in [("main", hands), ("pied", feet)] {
        for (b, at, side) in list {
            let t = scene.add_role(&format!("IK {what}.{side}"), "");
            scene.objects[t].pos = at;
            let name = scene.objects[t].name.clone();
            (scene.objects[b].ik, scene.objects[b].ik_chain) = (name, 2);
            targets.push(t);
        }
    }
    crate::rig::set_rest(scene, root);
    Ok(Rig { root, bones: crate::rig::bones_of(scene, root).len(), targets })
        .inspect(|_| debug_assert!(scene.objects.len() > n0))
}

/// Génère le rig d'après les repères posés dans la scène, lie le modèle `model` au squelette s'il
/// est donné, et cache les repères (on peut les remontrer pour recommencer).
pub fn generate(scene: &mut Scene, model: Option<usize>) -> Result<Rig, String> {
    let markers = read_markers(scene).ok_or_else(|| crate::i18n::t("Il manque des repères de rig : pose-les d'abord (Ajouter › rig bipède › poser les repères)."))?;
    let rig = build(scene, &markers)?;
    if let Some(k) = model.filter(|&k| k < scene.objects.len() && crate::scene::kind(&scene.objects[k]) == crate::scene::Kind::Model) {
        crate::rig::bind(scene, k, rig.root);
    }
    if let Some(g) = scene.objects.iter().position(|o| o.name == GROUP) {
        scene.objects[g].visible = false;
    }
    Ok(rig)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist(a: V3, b: V3) -> f32 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }

    #[test]
    fn reperes_par_defaut() {
        // Personnage de 1,8 m, bras le long du corps
        let m = default_markers([-0.3, 0.0, -0.15], [0.3, 1.8, 0.15]);
        let at = |n: &str| m.iter().find(|x| x.0 == n).unwrap().1;
        assert_eq!(m.len(), MARKERS.len());
        assert!(at("rig tête")[1] > at("rig cou")[1] && at("rig cou")[1] > at("rig poitrine")[1] && at("rig poitrine")[1] > at("rig bassin")[1]);
        assert!(at("rig genou")[1] > at("rig cheville")[1] && at("rig hanche")[1] > at("rig genou")[1]);
        assert!(at("rig poignet")[1] < at("rig coude")[1], "bras le long du corps");
        assert!(at("rig orteils")[2] > at("rig cheville")[2], "les pieds pointent vers +Z");
        // Boîte large : pose en T, le bras part à l'horizontale et reste dans la boîte
        let t = default_markers([-0.9, 0.0, -0.15], [0.9, 1.8, 0.15]);
        let at = |n: &str| t.iter().find(|x| x.0 == n).unwrap().1;
        assert!((at("rig poignet")[1] - at("rig épaule")[1]).abs() < 1e-4 && at("rig poignet")[0] > at("rig coude")[0] && at("rig poignet")[0] < 0.9);
        // Boîte vide ou infinie : une taille humaine par défaut
        let d = default_markers([f32::NAN; 3], [0.0; 3]);
        assert!(d.iter().all(|x| x.1.iter().all(|v| v.is_finite())));
        assert!((d.iter().find(|x| x.0 == "rig tête").unwrap().1[1] - 1.764).abs() < 1e-3);
    }

    #[test]
    fn squelette_bipede() {
        let mut sc = Scene::default();
        let model = sc.add_primitive("@cube");
        // Sans repères : un message, pas de panique
        assert!(generate(&mut sc, Some(model)).is_err());
        let g = place_markers(&mut sc, [-0.3, 0.0, -0.15], [0.3, 1.8, 0.15]);
        assert_eq!(sc.objects.iter().filter(|o| o.parent == GROUP).count(), MARKERS.len());
        // Reposer les repères ne les double pas
        assert_eq!(place_markers(&mut sc, [-0.3, 0.0, -0.15], [0.3, 1.8, 0.15]), g);
        assert_eq!(sc.objects.iter().filter(|o| o.parent == GROUP).count(), MARKERS.len());
        // L'utilisateur déplace un repère : le coude plus bas et plus en arrière
        let elbow = sc.objects.iter().position(|o| o.name == "rig coude").unwrap();
        sc.objects[elbow].pos = [0.27, 1.05, -0.05];
        let markers = read_markers(&sc).unwrap();
        let at = |n: &str| markers.iter().find(|x| x.0 == n).unwrap().1;
        let rig = generate(&mut sc, Some(model)).unwrap();
        // 4 os de tronc, 4 par bras, 3 par jambe
        assert_eq!(rig.bones, 18);
        assert_eq!(rig.targets.len(), 4);
        fn k(sc: &Scene, n: &str) -> usize {
            sc.objects.iter().position(|o| o.name == n).unwrap_or_else(|| panic!("{n}"))
        }
        fn head_of(sc: &Scene, n: &str) -> V3 {
            sc.world(k(sc, n), [0.0; 3])
        }
        fn tail_of(sc: &Scene, n: &str) -> V3 {
            sc.world(k(sc, n), [0.0, sc.objects[k(sc, n)].length, 0.0])
        }
        // Les articulations sont sur les repères
        assert!(dist(head_of(&sc, "bassin"), at("rig bassin")) < 1e-3);
        assert!(dist(tail_of(&sc, "tête"), at("rig tête")) < 1e-3);
        assert!(dist(head_of(&sc, "bras.G"), at("rig épaule")) < 1e-3 && dist(tail_of(&sc, "bras.G"), at("rig coude")) < 1e-3);
        assert!(dist(tail_of(&sc, "avant-bras.G"), at("rig poignet")) < 1e-3);
        assert!(dist(head_of(&sc, "tibia.G"), at("rig genou")) < 1e-3 && dist(tail_of(&sc, "pied.G"), at("rig orteils")) < 1e-3);
        // Le côté droit est le reflet du gauche
        let (l, r) = (tail_of(&sc, "avant-bras.G"), tail_of(&sc, "avant-bras.D"));
        assert!((l[0] + r[0]).abs() < 1e-3 && (l[1] - r[1]).abs() < 1e-3 && (l[2] - r[2]).abs() < 1e-3, "{l:?} {r:?}");
        // Hiérarchie : la main descend du bassin par la colonne, le pied par la cuisse
        assert!(sc.descends(k(&sc, "main.D"), k(&sc, "colonne")) && sc.descends(k(&sc, "pied.G"), k(&sc, "cuisse.G")) && !sc.descends(k(&sc, "pied.G"), k(&sc, "colonne")));
        assert_eq!(crate::rig::root_of(&sc, k(&sc, "main.G")), rig.root);
        // Le modèle est lié, les repères cachés, la pose de repos retenue
        assert_eq!(sc.objects[model].skin, "bassin");
        assert!(!sc.objects[g].visible);
        assert!(sc.objects[k(&sc, "tibia.G")].rest.is_some());
        // Cinématique inverse : on monte la cible du pied gauche, la cheville la rejoint, le genou plie
        let t = k(&sc, "IK pied.G");
        assert_eq!((sc.objects[k(&sc, "tibia.G")].ik.as_str(), sc.objects[k(&sc, "tibia.G")].ik_chain), ("IK pied.G", 2));
        let knee0 = head_of(&sc, "tibia.G");
        sc.objects[t].pos[1] += 0.25;
        sc.objects[t].pos[2] += 0.1;
        assert!(crate::rig::solve_ik(&mut sc));
        let ankle = sc.world(k(&sc, "tibia.G"), [0.0, sc.objects[k(&sc, "tibia.G")].length, 0.0]);
        assert!(dist(ankle, sc.objects[t].pos) < 0.02, "{ankle:?} {:?}", sc.objects[t].pos);
        assert!(dist(sc.world(k(&sc, "tibia.G"), [0.0; 3]), knee0) > 0.02, "le genou a bougé");
        // La jambe droite n'a pas bougé
        assert!(dist(sc.world(k(&sc, "tibia.D"), [0.0, sc.objects[k(&sc, "tibia.D")].length, 0.0]), [-at("rig cheville")[0], at("rig cheville")[1], at("rig cheville")[2]]) < 1e-3);
        // Retour au repos
        crate::rig::to_rest(&mut sc, rig.root);
        assert!(dist(sc.world(k(&sc, "tibia.G"), [0.0; 3]), knee0) < 1e-3);
        // Repères tous confondus : refusé
        let same: Vec<(&str, V3)> = MARKERS.iter().map(|m| (m.0, [1.0; 3])).collect();
        assert!(build(&mut sc, &same).is_err());
    }
}
