//! Lancer de rayons sur un maillage : arbre de boîtes englobantes (BVH) construit par
//! coupe médiane, parcouru sans allocation. Sert au bake (occlusion, épaisseur, détails
//! d'un modèle haute définition) ; pensé pour être appelé depuis plusieurs fils à la fois.

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

struct Node {
    lo: V3,
    hi: V3,
    /// Feuille : premier triangle et nombre ; sinon indice du fils gauche (le droit suit) et 0
    start: u32,
    count: u32,
}

pub struct Bvh {
    nodes: Vec<Node>,
    /// Triangles rangés dans l'ordre des feuilles : sommet, deux arêtes, indice d'origine
    tris: Vec<(V3, V3, V3, u32)>,
}

pub struct Hit {
    pub t: f32,
    pub tri: u32,
    pub u: f32,
    pub v: f32,
}

impl Bvh {
    pub fn new(pos: &[V3], tris: &[[u32; 3]]) -> Bvh {
        let mut items: Vec<(V3, V3, V3, u32, V3)> = tris
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
                (a, sub(b, a), sub(c, a), i as u32, [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, (a[2] + b[2] + c[2]) / 3.0])
            })
            .collect();
        let mut nodes = vec![];
        let n = items.len();
        build(&mut nodes, &mut items, 0, n);
        Bvh { nodes, tris: items.into_iter().map(|x| (x.0, x.1, x.2, x.3)).collect() }
    }

    /// L'arbre à plat pour la carte graphique : nœuds (min + début, max + nombre) et triangles
    /// (sommet + indice d'origine, deux arêtes), dans l'ordre des feuilles.
    pub fn flat(&self) -> (Vec<[f32; 4]>, Vec<[f32; 4]>) {
        let mut nodes = Vec::with_capacity(self.nodes.len() * 2);
        for n in &self.nodes {
            nodes.push([n.lo[0], n.lo[1], n.lo[2], f32::from_bits(n.start)]);
            nodes.push([n.hi[0], n.hi[1], n.hi[2], f32::from_bits(n.count)]);
        }
        let mut tris = Vec::with_capacity(self.tris.len() * 3);
        for t in &self.tris {
            tris.push([t.0[0], t.0[1], t.0[2], f32::from_bits(t.3)]);
            tris.push([t.1[0], t.1[1], t.1[2], 0.0]);
            tris.push([t.2[0], t.2[1], t.2[2], 0.0]);
        }
        (nodes, tris)
    }

    /// Premier triangle touché entre `tmin` et `tmax`.
    pub fn closest(&self, o: V3, d: V3, tmin: f32, tmax: f32) -> Option<Hit> {
        self.trace(o, d, tmin, tmax, false)
    }

    /// Y a-t-il un obstacle entre `tmin` et `tmax` ? (plus rapide : s'arrête au premier)
    pub fn occluded(&self, o: V3, d: V3, tmin: f32, tmax: f32) -> bool {
        self.trace(o, d, tmin, tmax, true).is_some()
    }

    fn trace(&self, o: V3, d: V3, tmin: f32, mut tmax: f32, any: bool) -> Option<Hit> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = [1.0 / d[0], 1.0 / d[1], 1.0 / d[2]];
        // Entrée du rayon dans une boîte (None : il la manque)
        let enter = |node: &Node, tmax: f32| -> Option<f32> {
            let (mut t0, mut t1) = (tmin, tmax);
            for k in 0..3 {
                let (a, b) = ((node.lo[k] - o[k]) * inv[k], (node.hi[k] - o[k]) * inv[k]);
                t0 = t0.max(a.min(b));
                t1 = t1.min(a.max(b));
            }
            (t0 <= t1).then_some(t0)
        };
        let t_root = enter(&self.nodes[0], tmax)?;
        // Pile de (nœud, entrée) : le fils le plus proche est visité d'abord, et un nœud dont l'entrée
        // est plus loin que le meilleur impact déjà trouvé est sauté sans être relu
        let mut stack = [(0u32, 0.0f32); 64];
        stack[0] = (0, t_root);
        let mut sp = 1;
        let mut best = None;
        while sp > 0 {
            sp -= 1;
            let (ni, t_in) = stack[sp];
            if t_in > tmax {
                continue;
            }
            let node = &self.nodes[ni as usize];
            if node.count > 0 {
                for tr in &self.tris[node.start as usize..(node.start + node.count) as usize] {
                    // Möller-Trumbore
                    let p = cross(d, tr.2);
                    let det = dot(tr.1, p);
                    if det.abs() < 1e-12 {
                        continue;
                    }
                    let f = 1.0 / det;
                    let s = sub(o, tr.0);
                    let u = f * dot(s, p);
                    if !(0.0..=1.0).contains(&u) {
                        continue;
                    }
                    let q = cross(s, tr.1);
                    let v = f * dot(d, q);
                    if v < 0.0 || u + v > 1.0 {
                        continue;
                    }
                    let t = f * dot(tr.2, q);
                    if t > tmin && t < tmax {
                        tmax = t;
                        best = Some(Hit { t, tri: tr.3, u, v });
                        if any {
                            return best;
                        }
                    }
                }
            } else if sp + 2 <= stack.len() {
                let (l, r) = (node.start, node.start + 1);
                match (enter(&self.nodes[l as usize], tmax), enter(&self.nodes[r as usize], tmax)) {
                    (Some(a), Some(b)) => {
                        // Le plus proche en haut de la pile
                        let (near, far) = if a <= b { ((l, a), (r, b)) } else { ((r, b), (l, a)) };
                        stack[sp] = far;
                        stack[sp + 1] = near;
                        sp += 2;
                    }
                    (Some(a), None) => {
                        stack[sp] = (l, a);
                        sp += 1;
                    }
                    (None, Some(b)) => {
                        stack[sp] = (r, b);
                        sp += 1;
                    }
                    (None, None) => {}
                }
            }
        }
        best
    }
}

fn build(nodes: &mut Vec<Node>, items: &mut [(V3, V3, V3, u32, V3)], start: usize, end: usize) -> u32 {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    let (mut clo, mut chi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for it in &items[start..end] {
        for p in [it.0, [it.0[0] + it.1[0], it.0[1] + it.1[1], it.0[2] + it.1[2]], [it.0[0] + it.2[0], it.0[1] + it.2[1], it.0[2] + it.2[2]]] {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        for k in 0..3 {
            clo[k] = clo[k].min(it.4[k]);
            chi[k] = chi[k].max(it.4[k]);
        }
    }
    let id = nodes.len() as u32;
    nodes.push(Node { lo, hi, start: start as u32, count: (end - start) as u32 });
    let ext = sub(chi, clo);
    let axis = if ext[0] >= ext[1] && ext[0] >= ext[2] { 0 } else if ext[1] >= ext[2] { 1 } else { 2 };
    if end - start <= 4 || ext[axis] <= 0.0 {
        return id;
    }
    let mid = (start + end) / 2;
    items[start..end].select_nth_unstable_by(mid - start, |a, b| a.4[axis].total_cmp(&b.4[axis]));
    // Les deux fils doivent se suivre : on réserve le gauche, puis on construit son sous-arbre
    // après avoir placé le droit juste derrière lui.
    let left = nodes.len() as u32;
    nodes.push(Node { lo, hi, start: 0, count: 0 });
    nodes.push(Node { lo, hi, start: 0, count: 0 });
    let l = build_into(nodes, items, start, mid, left);
    let r = build_into(nodes, items, mid, end, left + 1);
    let _ = (l, r);
    nodes[id as usize].start = left;
    nodes[id as usize].count = 0;
    id
}

/// Construit le sous-arbre de `items[start..end]` en écrivant sa racine à l'emplacement `slot`.
fn build_into(nodes: &mut Vec<Node>, items: &mut [(V3, V3, V3, u32, V3)], start: usize, end: usize, slot: u32) -> u32 {
    let tmp = build(nodes, items, start, end);
    // `build` a ajouté la racine à la fin (et ses descendants après) : on la recopie dans `slot`
    let root = std::mem::replace(&mut nodes[tmp as usize], Node { lo: [0.0; 3], hi: [0.0; 3], start: 0, count: 0 });
    nodes[slot as usize] = root;
    slot
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rayons_sur_un_cube() {
        let pos: Vec<V3> = (0..8).map(|i| [if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 }, if i & 4 == 0 { -1.0 } else { 1.0 }]).collect();
        let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let tris: Vec<[u32; 3]> = quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect();
        let b = Bvh::new(&pos, &tris);
        // De l'extérieur vers le centre : on touche la face à distance 4
        let h = b.closest([5.0, 0.2, 0.3], [-1.0, 0.0, 0.0], 0.0, 100.0).unwrap();
        assert!((h.t - 4.0).abs() < 1e-4);
        // Du centre : toujours un mur ; vers l'extérieur depuis dehors : rien
        assert!(b.occluded([0.0, 0.0, 0.0], [0.3, 0.9, 0.1], 0.0, 100.0));
        assert!(!b.occluded([5.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.0, 100.0));
        // Beaucoup de triangles : même réponse qu'une recherche exhaustive
        let mut p2 = vec![];
        let mut t2 = vec![];
        for i in 0..40 {
            for j in 0..40 {
                let (x, y) = (i as f32 * 0.1, j as f32 * 0.1);
                let base = p2.len() as u32;
                p2.extend([[x, y, (x * 3.0).sin() * 0.2], [x + 0.1, y, ((x + 0.1) * 3.0).sin() * 0.2], [x, y + 0.1, (x * 3.0).sin() * 0.2]]);
                t2.push([base, base + 1, base + 2]);
            }
        }
        let b2 = Bvh::new(&p2, &t2);
        let mut hits = 0;
        for k in 0..200 {
            let o = [(k % 20) as f32 * 0.2 + 0.03, (k / 20) as f32 * 0.4 + 0.02, 2.0];
            if b2.closest(o, [0.0, 0.0, -1.0], 0.0, 10.0).is_some() {
                hits += 1;
            }
        }
        // Ces rayons tombent tous dans la moitié couverte de leur case ; ceux de l'autre moitié passent à travers
        assert_eq!(hits, 200);
        let mut miss = 0;
        for k in 0..200 {
            let o = [(k % 20) as f32 * 0.2 + 0.08, (k / 20) as f32 * 0.4 + 0.08, 2.0];
            if b2.closest(o, [0.0, 0.0, -1.0], 0.0, 10.0).is_none() {
                miss += 1;
            }
        }
        assert_eq!(miss, 200);
    }
}
