//! Exemples de fumée et de feu pour l'éditeur de nodes (voir `geo::example`).

use super::{hide_thumbs, set_param, set_value};
use crate::nodeflow::TimeRange;
use crate::nodes::Graph;

/// Le squelette d'une fumée : une boîte vide, et dans la zone de simulation une source puis le
/// solveur. Rend le graphe et ses nodes (boîte, source, solveur, fin de zone, sortie).
fn base(size: [f32; 3], res: f32) -> (Graph, u32, u32, u32, u32, u32) {
    let mut g = Graph::empty(crate::nodes::GraphKind::Geometry);
    g.time = Some(TimeRange { fps: 24.0, start: 1, end: 120 });
    let vol = g.add("vol_box", [0.0, 0.0]);
    set_value(&mut g, vol, 0, [0.0, size[1] * 0.5, 0.0, 0.0]);
    set_value(&mut g, vol, 1, [size[0], size[1], size[2], 0.0]);
    set_value(&mut g, vol, 2, [res; 4]);
    let (begin, end) = crate::nodeflow::add_zone(&mut g, "sim", [0.0, 330.0]);
    let src = g.add("vol_source", [250.0, 330.0]);
    let solver = g.add("sim_smoke", [750.0, 330.0]);
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == end) {
        n.pos = [1000.0, 330.0];
    }
    let out = g.add("geo_out", [1250.0, 330.0]);
    g.connect(vol, 0, begin, 0);
    g.connect(begin, 1, src, 0);
    g.connect(begin, 0, src, 11);
    g.connect(src, 0, solver, 0);
    g.connect(begin, 0, solver, 1);
    g.connect(solver, 0, end, 0);
    g.connect(end, 0, out, 0);
    (g, vol, src, solver, end, out)
}

/// Une bouffée chaude qui sort d'une petite sphère et monte en volutes.
pub fn rising() -> Graph {
    let (mut g, _, src, _, _, _) = base([1.6, 2.4, 1.6], 72.0);
    set_value(&mut g, src, 2, [0.0, 0.22, 0.0, 0.0]);
    set_value(&mut g, src, 3, [0.14; 4]);
    g.frames.push(crate::nodes::Frame { rect: [230.0, 290.0, 720.0, 420.0], label: "à chaque image : la source pose de la fumée chaude, le solveur la fait monter".into(), color: [120, 120, 140], note: String::new() });
    g
}

/// Un feu : la source pose du carburant chaud, la combustion le brûle en flammes et en fumée.
pub fn campfire() -> Graph {
    let (mut g, _, src, solver, _, _) = base([1.6, 2.4, 1.6], 72.0);
    set_param(&mut g, src, "forme", "boîte");
    set_value(&mut g, src, 2, [0.0, 0.2, 0.0, 0.0]);
    set_value(&mut g, src, 4, [0.42, 0.2, 0.42, 0.0]);
    set_value(&mut g, src, 5, [0.0; 4]);
    set_value(&mut g, src, 6, [1.0; 4]);
    set_value(&mut g, src, 7, [1.0; 4]);
    set_value(&mut g, src, 8, [0.0, 0.6, 0.0, 0.0]);
    set_value(&mut g, src, 10, [0.8; 4]);
    let burn = g.add("vol_burn", [500.0, 330.0]);
    set_value(&mut g, burn, 2, [3.0; 4]);
    set_value(&mut g, burn, 3, [2.6; 4]);
    set_value(&mut g, burn, 4, [0.25; 4]);
    set_value(&mut g, burn, 6, [0.35; 4]);
    let begin = g.nodes.iter().find(|n| n.op == "sim_in").map(|n| n.id).unwrap_or(0);
    g.links.retain(|l| !(l.from == src && l.to == solver));
    g.connect(src, 0, burn, 0);
    g.connect(begin, 0, burn, 7);
    g.connect(burn, 0, solver, 0);
    set_value(&mut g, solver, 4, [2.2; 4]);
    set_value(&mut g, solver, 6, [3.5; 4]);
    set_value(&mut g, solver, 7, [0.6; 4]);
    set_value(&mut g, solver, 8, [0.2; 4]);
    g.frames.push(crate::nodes::Frame { rect: [230.0, 290.0, 720.0, 420.0], label: "le carburant chaud brûle : chaleur, flammes, un peu de fumée".into(), color: [210, 120, 50], note: String::new() });
    g
}

/// La fumée rencontre une boule et la contourne. La boule est montrée en points.
pub fn obstacle() -> Graph {
    let (mut g, _, src, solver, end, out) = base([1.6, 2.4, 1.6], 72.0);
    set_value(&mut g, src, 2, [0.0, 0.2, 0.0, 0.0]);
    set_value(&mut g, src, 3, [0.16; 4]);
    set_value(&mut g, solver, 10, [0.06, 1.0, 0.0, 0.0]);
    set_value(&mut g, solver, 11, [0.26; 4]);
    let ball = g.add("geo_sphere", [500.0, 760.0]);
    set_value(&mut g, ball, 0, [500.0; 4]);
    set_value(&mut g, ball, 1, [0.25; 4]);
    let ball_at = g.add("geo_transform", [750.0, 760.0]);
    set_value(&mut g, ball_at, 1, [0.06, 1.0, 0.0, 0.0]);
    let tint = g.add("geo_attr", [1000.0, 760.0]);
    set_value(&mut g, tint, 1, [0.25, 0.42, 0.75, 1.0]);
    let both = g.add("geo_merge", [1250.0, 330.0]);
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == out) {
        n.pos = [1500.0, 330.0];
    }
    g.links.retain(|l| !(l.from == end && l.to == out));
    g.connect(ball, 0, ball_at, 0);
    g.connect(ball_at, 0, tint, 0);
    g.connect(end, 0, both, 0);
    g.connect(tint, 0, both, 1);
    g.connect(both, 0, out, 0);
    hide_thumbs(&mut g, &[ball, ball_at, tint]);
    g
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::volume::from_half;

    /// Les trois exemples de fumée, en petit : le volume traverse la zone, bouge, garde son état
    /// d'image en image, et se fige sur le disque en demi-précision.
    #[test]
    fn fumee_par_les_nodes() {
        for (name, card) in [("fumée qui monte", false), ("feu de camp", false), ("fumée contre un obstacle", false), ("feu de camp", true)] {
            let mut g = example(name).unwrap();
            let id = |g: &Graph, op: &str| g.nodes.iter().find(|n| n.op == op).map(|n| n.id).unwrap();
            let (vol, solver, end) = (id(&g, "vol_box"), id(&g, "sim_smoke"), id(&g, "sim_out"));
            set_value(&mut g, vol, 2, [24.0; 4]);
            set_param(&mut g, solver, "calcul", if card { "carte graphique" } else { "processeur" });
            g.time = Some(TimeRange { fps: 24.0, start: 1, end: 8 });
            let mut e = Engine::default();
            let first = e.eval(&g, 1, "").map_err(|x| format!("{name} : {}", x.1)).unwrap();
            let v0 = first.volume.clone().expect("un volume sort du graphe");
            assert!(v0.blank() && v0.res.iter().all(|n| n % 8 == 0), "{name} : {:?}", v0.res);
            let r = e.eval(&g, 8, "").map_err(|x| format!("{name} : {}", x.1)).unwrap();
            assert_eq!(e.stats.steps, 7, "{name}");
            let v = r.volume.clone().unwrap();
            let grids = v.grids();
            let total = |k: usize| grids.sca.iter().map(|s| from_half(s[k]) as f64).sum::<f64>();
            assert!(total(0) > 1.0, "{name} : de la fumée ({})", total(0));
            assert!(grids.vel.iter().any(|u| from_half(u[1]) > 0.05), "{name} : elle monte");
            assert!(grids.sca.iter().chain(grids.vel.iter()).all(|s| s.iter().all(|x| from_half(*x).is_finite())), "{name}");
            if name == "feu de camp" {
                assert!(total(3) > 1.0, "des flammes ({})", total(3));
                assert!(grids.sca.iter().any(|s| from_half(s[1]) > 1.2), "la combustion chauffe");
            }
            if name == "fumée contre un obstacle" {
                // Rien dans la boule, et ses points sont dans la sortie à côté du volume
                assert!(r.n > 0);
                let (c, rad) = ([0.06f32, 1.0, 0.0], 0.2f32);
                let d = crate::smoke::Dim::of(v.res);
                for i in 0..d.cells() {
                    let q = d.at(i);
                    let p = [0, 1, 2].map(|k| v.origin[k] + (q[k] as f32 + 0.5) * v.voxel);
                    if (0..3).map(|k| (p[k] - c[k]).powi(2)).sum::<f32>().sqrt() < rad {
                        assert_eq!(grids.sca[i], [0; 4]);
                    }
                }
            }
            // L'image d'avant sort du cache, sans rien simuler
            let _ = e.eval(&g, 7, "").map_err(|x| x.1).unwrap();
            assert_eq!(e.stats.steps, 0, "{name}");
            if card {
                continue;
            }
            // Figé sur le disque puis relu : les mêmes voxels, 14 octets chacun
            let dir = std::env::temp_dir().join(format!("aryadne-fumee-test-{}-{}", std::process::id(), name.len()));
            let mut baker = Engine::default();
            assert_eq!(baker.bake(&g, end, &dir, "").unwrap(), 8);
            let file = bake_file(&dir, 8, 0);
            assert!(std::fs::metadata(&file).unwrap().len() < (v.cells() * 14 + 400) as u64);
            let mut fresh = Engine::default();
            fresh.baked.insert(end, dir.clone());
            let back = fresh.eval(&g, 8, "").map_err(|x| x.1).unwrap().volume.unwrap();
            assert_eq!(fresh.stats.steps, 0);
            assert!(*back.grids() == *baker.eval(&g, 8, "").map_err(|x| x.1).unwrap().volume.unwrap().grids(), "{name} : le cache rend les mêmes voxels");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// Une source de points : la fumée naît autour des points d'une géométrie
    #[test]
    fn source_par_les_points_d_une_geometrie() {
        let mut g = Graph::empty(crate::nodes::GraphKind::Geometry);
        let vol = g.add("vol_box", [0.0, 0.0]);
        set_value(&mut g, vol, 2, [16.0; 4]);
        let line = g.add("geo_line", [0.0, 200.0]);
        set_value(&mut g, line, 1, [-0.5, 1.0, 0.0, 0.0]);
        set_value(&mut g, line, 2, [0.5, 1.0, 0.0, 0.0]);
        let src = g.add("vol_source", [250.0, 0.0]);
        set_param(&mut g, src, "forme", "points d'une géométrie");
        set_value(&mut g, src, 3, [0.1; 4]);
        set_value(&mut g, src, 10, [0.0; 4]);
        let look = g.add("vol_look", [500.0, 0.0]);
        set_value(&mut g, look, 1, [3.0; 4]);
        let out = g.add("geo_out", [750.0, 0.0]);
        g.connect(vol, 0, src, 0);
        g.connect(line, 0, src, 1);
        g.connect(src, 0, look, 0);
        g.connect(look, 0, out, 0);
        let v = Engine::default().eval(&g, 1, "").map_err(|x| x.1).unwrap().volume.unwrap();
        assert_eq!(v.look.density, 3.0);
        let s = crate::volume::Sampler::new(&v);
        assert!(s.at([0.0, 1.0, 0.0])[0] > 0.9 && s.at([0.4, 1.0, 0.0])[0] > 0.9, "de la fumée le long de la ligne");
        assert_eq!(s.at([0.0, 1.6, 0.0])[0], 0.0);
        assert_eq!(v.field("densité").unwrap()[0].len(), v.cells());
    }
}
