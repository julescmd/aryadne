//! Fumée et feu sur la carte graphique.
//!
//! Les mêmes calculs que `smoke`, noyau par noyau (voir son en-tête pour l'ordre d'un pas).
//!
//! - **Champs en demi-précision.** La fumée et la vitesse sont rangées en f16, quatre par voxel
//!   (8 octets), lues et écrites par `unpack2x16float` et `pack2x16float` : deux fois moins de
//!   mémoire à déplacer, et la relecture pour le cache est une simple copie.
//! - **Pression sans relecture.** Les produits scalaires du gradient conjugué sont réduits sur la
//!   carte (par paquets de 64) et restent dans un petit tampon : le nombre d'itérations est fixe,
//!   rien n'attend le processeur. La divergence qui reste est relue avec un pas de retard, pour
//!   l'afficher.
//! - **Trois jeux de noyaux** : les champs (advection, sources, forces, projection), la pression
//!   (produits, mises à jour), la multigrille (lissage rouge-noir, restriction, prolongement).
//!   Chaque lancement choisit sa case de réglages (`Bind::Slot`).
//! - **Un seul pas en route.** `busy` dit si la carte calcule encore.

use crate::gpusim::{self, Bind, Column, Gpu, Kernels, Readback};
use crate::smoke::{Burn, Dim, Emit, Forces, Live, Op, Params, React, Shape, Shot, Stats, COARSE_SWEEPS, SWEEPS};
use crate::volume::Grids;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

/// En-tête commun aux trois modules : réglages, numéros de voxel, demi-précision
const COMMON: &str = r#"
struct P { n: vec4<u32>, k: vec4<u32>, a: vec4<f32>, b: vec4<f32>, c: vec4<f32>, d: vec4<f32>, e: vec4<f32>, f: vec4<f32> }
@group(0) @binding(0) var<uniform> p: P;
fn dims() -> vec3<i32> { return vec3<i32>(p.n.xyz); }
fn at(c: u32) -> vec3<i32> { return vec3<i32>(i32(c % p.n.x), i32((c / p.n.x) % p.n.y), i32(c / (p.n.x * p.n.y))); }
fn idx(q: vec3<i32>) -> u32 {
    let m = clamp(q, vec3<i32>(0), dims() - vec3<i32>(1));
    return u32(m.x) + p.n.x * (u32(m.y) + p.n.y * u32(m.z));
}
fn inside(q: vec3<i32>) -> bool { return all(q >= vec3<i32>(0)) && all(q < dims()); }
fn unp(v: vec2<u32>) -> vec4<f32> { return vec4<f32>(unpack2x16float(v.x), unpack2x16float(v.y)); }
fn pk(v: vec4<f32>) -> vec2<u32> {
    let c = clamp(v, vec4<f32>(-65504.0), vec4<f32>(65504.0));
    return vec2<u32>(pack2x16float(c.xy), pack2x16float(c.zw));
}
fn mx(a: vec4<f32>, b: vec4<f32>, t: f32) -> vec4<f32> { return a + (b - a) * t; }
const EX = vec3<i32>(1, 0, 0);
const EY = vec3<i32>(0, 1, 0);
const EZ = vec3<i32>(0, 0, 1);
fn axis(k: u32) -> vec3<i32> { return vec3<i32>(i32(k == 0u), i32(k == 1u), i32(k == 2u)); }
"#;

const FIELDS: &str = r#"
@group(0) @binding(1) var<storage, read_write> vel: array<vec2<u32>>;
@group(0) @binding(2) var<storage, read_write> sca: array<vec2<u32>>;
@group(0) @binding(3) var<storage, read_write> ta: array<vec2<u32>>;
@group(0) @binding(4) var<storage, read_write> tb: array<vec2<u32>>;
@group(0) @binding(5) var<storage, read_write> typ: array<u32>;
@group(0) @binding(6) var<storage, read_write> mask: array<f32>;
@group(0) @binding(7) var<storage, read_write> pr: array<f32>;
@group(0) @binding(8) var<storage, read_write> rs: array<f32>;

fn tat(q: vec3<i32>) -> u32 { if (inside(q)) { return typ[idx(q)]; } return 1u; }

// Types des voxels : la couche du bord, les obstacles, le reste fluide
@compute @workgroup_size(WG)
fn types(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    let q = at(c);
    let n = dims();
    if (q.x == 0 || q.y == 0 || q.z == 0 || q.x == n.x - 1 || q.y == n.y - 1 || q.z == n.z - 1) {
        var t = 2u;
        if (p.k.x == 0u && q.y == 0) { t = 1u; }
        if (p.k.x >= 2u && q.y != n.y - 1) { t = 1u; }
        typ[c] = t;
        return;
    }
    let w = p.d.xyz + (vec3<f32>(q) + 0.5) * p.d.w;
    var t = 0u;
    let db = w - p.a.xyz;
    if (p.a.w > 0.0 && dot(db, db) < p.a.w * p.a.w) { t = 1u; }
    if (p.b.w > 0.5 && all(abs(w - p.b.xyz) < p.c.xyz)) { t = 1u; }
    typ[c] = t;
}

fn fetch(w: u32, q: vec3<i32>) -> vec4<f32> {
    let i = idx(q);
    if (w == 0u) { return unp(vel[i]); }
    if (w == 1u) { return unp(sca[i]); }
    return unp(ta[i]);
}

struct Tri { v: vec4<f32>, lo: vec4<f32>, hi: vec4<f32> }

// Lecture trilinéaire, avec le plus petit et le plus grand des huit voisins
fn tri(w: u32, q0: vec3<f32>) -> Tri {
    let q = clamp(q0, vec3<f32>(0.0), vec3<f32>(dims() - vec3<i32>(1)));
    let f = floor(q);
    let t = q - f;
    let i = vec3<i32>(f);
    let c0 = fetch(w, i);
    let c1 = fetch(w, i + EX);
    let c2 = fetch(w, i + EY);
    let c3 = fetch(w, i + EX + EY);
    let c4 = fetch(w, i + EZ);
    let c5 = fetch(w, i + EX + EZ);
    let c6 = fetch(w, i + EY + EZ);
    let c7 = fetch(w, i + EX + EY + EZ);
    var r: Tri;
    r.v = mx(mx(mx(c0, c1, t.x), mx(c2, c3, t.x), t.y), mx(mx(c4, c5, t.x), mx(c6, c7, t.x), t.y), t.z);
    r.lo = min(min(min(c0, c1), min(c2, c3)), min(min(c4, c5), min(c6, c7)));
    r.hi = max(max(max(c0, c1), max(c2, c3)), max(max(c4, c5), max(c6, c7)));
    return r;
}

fn vel_center(q: vec3<i32>) -> vec3<f32> {
    let v = unp(vel[idx(q)]);
    return 0.5 * (v.xyz + vec3<f32>(unp(vel[idx(q + EX)]).x, unp(vel[idx(q + EY)]).y, unp(vel[idx(q + EZ)]).z));
}

// Vitesse au milieu de la face basse d'axe k
fn vel_face(q: vec3<i32>, k: u32) -> vec3<f32> {
    let ek = axis(k);
    var u = vec3<f32>(0.0);
    for (var a = 0u; a < 3u; a++) {
        if (a == k) {
            u[a] = unp(vel[idx(q)])[a];
        } else {
            let ea = axis(a);
            u[a] = 0.25 * (unp(vel[idx(q)])[a] + unp(vel[idx(q - ek)])[a] + unp(vel[idx(q + ea)])[a] + unp(vel[idx(q + ea - ek)])[a]);
        }
    }
    return u;
}

// Advection de la fumée : l'aller (vers ta)
@compute @workgroup_size(WG)
fn advect_sca_fwd(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    let q = at(c);
    let u = vel_center(q);
    ta[c] = pk(tri(1u, vec3<f32>(q) - u * p.a.x).v);
}

// Advection de la fumée : le retour et la correction bornée (vers tb)
@compute @workgroup_size(WG)
fn advect_sca_corr(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    let q = at(c);
    let u = vel_center(q);
    let back = tri(1u, vec3<f32>(q) - u * p.a.x);
    let again = tri(2u, vec3<f32>(q) + u * p.a.x).v;
    tb[c] = pk(clamp(unp(ta[c]) + 0.5 * (unp(sca[c]) - again), back.lo, back.hi));
}

@compute @workgroup_size(WG)
fn advect_vel_fwd(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    let q = at(c);
    var o = vec4<f32>(0.0);
    for (var k = 0u; k < 3u; k++) {
        let u = vel_face(q, k);
        o[k] = tri(0u, vec3<f32>(q) - u * p.a.x).v[k];
    }
    ta[c] = pk(o);
}

@compute @workgroup_size(WG)
fn advect_vel_corr(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    let q = at(c);
    let here = unp(vel[c]);
    let fwd = unp(ta[c]);
    var o = vec4<f32>(0.0);
    for (var k = 0u; k < 3u; k++) {
        let u = vel_face(q, k);
        let back = tri(0u, vec3<f32>(q) - u * p.a.x);
        let again = tri(2u, vec3<f32>(q) + u * p.a.x).v[k];
        o[k] = clamp(fwd[k] + 0.5 * (here[k] - again), back.lo[k], back.hi[k]);
    }
    tb[c] = pk(o);
}

fn rnd(c: u32, seed: u32) -> f32 {
    var x = (c * 0x9E3779B9u) ^ (seed * 0x85EBCA6Bu);
    x ^= x >> 16u;
    x *= 0x7FEB352Du;
    x ^= x >> 15u;
    x *= 0x846CA68Bu;
    x ^= x >> 16u;
    return f32(x >> 8u) / 16777216.0;
}

// Part d'un voxel prise par la source
fn cov(q: vec3<i32>) -> f32 {
    if (!inside(q)) { return 0.0; }
    let w = p.e.xyz + (vec3<f32>(q) + 0.5) * p.e.w;
    var s = 0.0;
    if (p.k.x == 0u) {
        s = clamp((p.a.w - length(w - p.a.xyz)) / p.b.w, 0.0, 1.0);
    } else if (p.k.x == 1u) {
        let m = abs(w - p.a.xyz) - p.b.xyz;
        s = clamp(-max(max(m.x, m.y), m.z) / p.b.w, 0.0, 1.0);
    } else {
        s = mask[idx(q)];
    }
    let u = vec3<u32>(q);
    let blk = (u.x >> 2u) + 1024u * ((u.y >> 2u) + 1024u * (u.z >> 2u));
    return s * (1.0 - p.c.w * rnd(blk, p.k.z));
}

// Une source, sur place
@compute @workgroup_size(WG)
fn emit(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    let q = at(c);
    let w = cov(q);
    if (w > 0.0) {
        var a = unp(sca[c]);
        if (p.k.y == 1u) {
            a = vec4<f32>(a.xyz + p.c.xyz * w * p.f.x, a.w);
        } else {
            a = vec4<f32>(max(a.xyz, p.c.xyz * w), a.w);
        }
        sca[c] = pk(a);
    }
    if (p.k.w == 1u) {
        let wf = 0.5 * (vec3<f32>(w) + vec3<f32>(cov(q - EX), cov(q - EY), cov(q - EZ)));
        if (any(wf > vec3<f32>(0.0))) {
            var u = unp(vel[c]);
            u = vec4<f32>(u.xyz + (p.d.xyz - u.xyz) * min(wf, vec3<f32>(1.0)), u.w);
            vel[c] = pk(u);
        }
    }
}

// Dissipation, refroidissement, combustion
@compute @workgroup_size(WG)
fn react(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    var v = unp(vel[c]);
    if (typ[c] != 0u) {
        sca[c] = vec2<u32>(0u);
        v.w = 0.0;
        vel[c] = pk(v);
        return;
    }
    var a = unp(sca[c]);
    a.x *= p.a.x;
    a.y *= p.a.y;
    a.w *= p.a.w;
    var grow = 0.0;
    if (p.k.x == 1u) {
        if (a.z > 0.0 && a.y > p.b.x) {
            let most = p.b.y * p.a.z;
            let burned = min(a.z, most);
            a.z -= burned;
            a.y += p.b.z * burned;
            a.x += p.b.w * burned;
            a.w = max(a.w, burned / max(most, 1e-9));
            grow = p.c.x * burned / max(p.a.z, 1e-9);
        }
        let t = max(a.y, 0.0);
        a.y = t / pow(1.0 + 3.0 * p.c.y * p.a.z * t * t * t, 1.0 / 3.0);
    }
    sca[c] = pk(a);
    v.w = grow;
    vel[c] = pk(v);
}

// Rotationnel au centre des voxels, et sa norme (vers ta)
@compute @workgroup_size(WG)
fn curl(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    let q = at(c);
    let xp = vel_center(q + EX);
    let xm = vel_center(q - EX);
    let yp = vel_center(q + EY);
    let ym = vel_center(q - EY);
    let zp = vel_center(q + EZ);
    let zm = vel_center(q - EZ);
    let k = 0.5 * p.a.x;
    let w = vec3<f32>((yp.z - ym.z - zp.y + zm.y) * k, (zp.x - zm.x - xp.z + xm.z) * k, (xp.y - xm.y - yp.x + ym.x) * k);
    ta[c] = pk(vec4<f32>(w, length(w)));
}

// Force qui rend les tourbillons (de ta vers tb)
@compute @workgroup_size(WG)
fn confine(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    let q = at(c);
    let n = vec3<f32>(unp(ta[idx(q + EX)]).w - unp(ta[idx(q - EX)]).w, unp(ta[idx(q + EY)]).w - unp(ta[idx(q - EY)]).w, unp(ta[idx(q + EZ)]).w - unp(ta[idx(q - EZ)]).w);
    let len = length(n);
    if (len < 1e-6) {
        tb[c] = vec2<u32>(0u);
        return;
    }
    tb[c] = pk(vec4<f32>(p.a.x * cross(n / len, unp(ta[c]).xyz), 0.0));
}

// Forces sur les faces, et vitesse nulle contre les solides
@compute @workgroup_size(WG)
fn forces(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    let q = at(c);
    let here = typ[c];
    var u = unp(vel[c]);
    for (var k = 0u; k < 3u; k++) {
        let qb = q - axis(k);
        let below = tat(qb);
        if (here == 1u || below == 1u || (here != 0u && below != 0u)) {
            u[k] = 0.0;
            continue;
        }
        let b = idx(qb);
        var acc = 0.0;
        if (k == 1u) {
            let s0 = unp(sca[c]);
            let s1 = unp(sca[b]);
            acc += p.a.y * 0.5 * (s0.y + s1.y) - p.a.z * 0.5 * (s0.x + s1.x);
            acc += p.b.y * 2.0;
        }
        if (p.k.x == 1u) {
            acc += 0.5 * (unp(tb[c])[k] + unp(tb[b])[k]);
        }
        u[k] += acc * p.a.x;
        if (k != 1u) {
            u[k] += (p.b[k] - u[k]) * p.b.w;
        }
        u[k] *= p.a.w;
    }
    vel[c] = pk(u);
}

fn pat(q: vec3<i32>) -> f32 { if (tat(q) == 0u) { return pr[idx(q)]; } return 0.0; }

// Résidu de départ : divergence voulue, moins celle qu'on a, moins A·p
@compute @workgroup_size(WG)
fn residual(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    if (typ[c] != 0u) {
        rs[c] = 0.0;
        return;
    }
    let q = at(c);
    let v = unp(vel[c]);
    let flux = unp(vel[idx(q + EX)]).x - v.x + unp(vel[idx(q + EY)]).y - v.y + unp(vel[idx(q + EZ)]).z - v.z;
    var diag = 0.0;
    var sum = 0.0;
    for (var k = 0u; k < 3u; k++) {
        for (var e = -1; e <= 1; e += 2) {
            let qn = q + axis(k) * e;
            let t = tat(qn);
            if (t != 1u) { diag += 1.0; }
            if (t == 0u) { sum += pr[idx(qn)]; }
        }
    }
    rs[c] = v.w * p.a.x - flux - (diag * pr[c] - sum);
}

// La vitesse perd le gradient de la pression
@compute @workgroup_size(WG)
fn project(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    let q = at(c);
    let here = typ[c];
    var pc = 0.0;
    if (here == 0u) { pc = pr[c]; }
    var u = unp(vel[c]);
    for (var k = 0u; k < 3u; k++) {
        let qb = q - axis(k);
        let below = tat(qb);
        if (here == 1u || below == 1u || (here != 0u && below != 0u)) { continue; }
        u[k] -= pc - pat(qb);
    }
    vel[c] = pk(u);
}
"#;

const PRESSURE: &str = r#"
@group(0) @binding(1) var<storage, read_write> pr: array<f32>;
@group(0) @binding(2) var<storage, read_write> rs: array<f32>;
@group(0) @binding(3) var<storage, read_write> z: array<f32>;
@group(0) @binding(4) var<storage, read_write> s: array<f32>;
@group(0) @binding(5) var<storage, read_write> typ: array<u32>;
@group(0) @binding(6) var<storage, read_write> pa: array<f32>;
@group(0) @binding(7) var<storage, read_write> pb: array<f32>;
@group(0) @binding(8) var<storage, read_write> sc: array<f32>;

fn tat(q: vec3<i32>) -> u32 { if (inside(q)) { return typ[idx(q)]; } return 1u; }

// A·s en un voxel fluide
fn apply_s(c: u32) -> f32 {
    let q = at(c);
    var diag = 0.0;
    var sum = 0.0;
    for (var k = 0u; k < 3u; k++) {
        for (var d = -1; d <= 1; d += 2) {
            let qn = q + axis(k) * d;
            let t = tat(qn);
            if (t != 1u) { diag += 1.0; }
            if (t == 0u) { sum += s[idx(qn)]; }
        }
    }
    return diag * s[c] - sum;
}

// Premier étage d'une réduction, par paquets de 64 voxels : r·z, s·A·s, ou le plus grand |r|
@compute @workgroup_size(WG)
fn dots(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= p.n.w) { return; }
    let total = p.n.x * p.n.y * p.n.z;
    var acc = 0.0;
    for (var j = 0u; j < 64u; j++) {
        let c = i * 64u + j;
        if (c >= total) { break; }
        if (typ[c] != 0u) { continue; }
        if (p.k.x == 0u) {
            acc += rs[c] * z[c];
        } else if (p.k.x == 1u) {
            acc += s[c] * apply_s(c);
        } else {
            acc = max(acc, abs(rs[c]));
        }
    }
    pa[i] = acc;
}

@compute @workgroup_size(WG)
fn reduce_ab(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= p.n.w) { return; }
    var acc = 0.0;
    for (var j = 0u; j < 64u; j++) {
        let c = i * 64u + j;
        if (c >= p.k.x) { break; }
        if (p.k.y == 0u) { acc += pa[c]; } else { acc = max(acc, pa[c]); }
    }
    pb[i] = acc;
}

@compute @workgroup_size(WG)
fn reduce_ba(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = lin(gid, nwg);
    if (i >= p.n.w) { return; }
    var acc = 0.0;
    for (var j = 0u; j < 64u; j++) {
        let c = i * 64u + j;
        if (c >= p.k.x) { break; }
        if (p.k.y == 0u) { acc += pb[c]; } else { acc = max(acc, pb[c]); }
    }
    pa[i] = acc;
}

// Dernier étage : un seul fil range le résultat dans le tampon des scalaires
@compute @workgroup_size(WG)
fn last_a(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    if (lin(gid, nwg) != 0u) { return; }
    var acc = 0.0;
    for (var c = 0u; c < p.k.x; c++) {
        if (p.k.y == 0u) { acc += pa[c]; } else { acc = max(acc, pa[c]); }
    }
    sc[p.k.z] = acc;
}

@compute @workgroup_size(WG)
fn last_b(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    if (lin(gid, nwg) != 0u) { return; }
    var acc = 0.0;
    for (var c = 0u; c < p.k.x; c++) {
        if (p.k.y == 0u) { acc += pb[c]; } else { acc = max(acc, pb[c]); }
    }
    sc[p.k.z] = acc;
}

// p ← p + α·s, r ← r − α·A·s, avec α = r·z / s·A·s
@compute @workgroup_size(WG)
fn update_pr(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    if (typ[c] != 0u) { return; }
    let sq = sc[2];
    var alpha = 0.0;
    if (sq > 1e-30) { alpha = sc[p.k.x] / sq; }
    pr[c] += alpha * s[c];
    rs[c] -= alpha * apply_s(c);
}

// s ← z + β·s, avec β = nouveau r·z / ancien
@compute @workgroup_size(WG)
fn update_s(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    if (p.k.y == 1u) {
        s[c] = z[c];
        return;
    }
    let old = sc[1u - p.k.x];
    var beta = 0.0;
    if (old > 1e-30) { beta = sc[p.k.x] / old; }
    s[c] = z[c] + beta * s[c];
}
"#;

const MULTIGRID: &str = r#"
@group(0) @binding(1) var<storage, read_write> xf: array<f32>;
@group(0) @binding(2) var<storage, read_write> bf: array<f32>;
@group(0) @binding(3) var<storage, read_write> tf: array<u32>;
@group(0) @binding(4) var<storage, read_write> xc: array<f32>;
@group(0) @binding(5) var<storage, read_write> bc: array<f32>;
@group(0) @binding(6) var<storage, read_write> tc: array<u32>;

fn tat(q: vec3<i32>) -> u32 { if (inside(q)) { return tf[idx(q)]; } return 1u; }
fn cdims() -> vec3<u32> { return p.n.xyz / 2u; }
fn cat(c: u32) -> vec3<i32> { let n = cdims(); return vec3<i32>(i32(c % n.x), i32((c / n.x) % n.y), i32(c / (n.x * n.y))); }

// Un demi-balayage de Gauss-Seidel rouge-noir
@compute @workgroup_size(WG)
fn relax(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    let q = at(c);
    let mine = (u32(q.x + q.y + q.z) & 1u) == p.k.x;
    let first = p.k.y == 1u;
    if (tf[c] != 0u || (first && !mine)) {
        if (first) { xf[c] = 0.0; }
        return;
    }
    if (!mine) { return; }
    var diag = 0.0;
    var sum = 0.0;
    for (var k = 0u; k < 3u; k++) {
        for (var d = -1; d <= 1; d += 2) {
            let qn = q + axis(k) * d;
            let t = tat(qn);
            if (t != 1u) { diag += 1.0; }
            if (t == 0u && !first) { sum += xf[idx(qn)]; }
        }
    }
    var x = 0.0;
    if (diag > 0.0) { x = (bf[c] + sum) / diag; }
    xf[c] = x;
}

// Second membre du niveau grossier : le résidu du niveau fin, réuni par huit
@compute @workgroup_size(WG)
fn gather(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    if (tc[c] != 0u) {
        bc[c] = 0.0;
        return;
    }
    let qc = cat(c);
    var total = 0.0;
    for (var j = 0; j < 8; j++) {
        let q = 2 * qc + vec3<i32>(j & 1, (j >> 1u) & 1, j >> 2u);
        let f = idx(q);
        if (tf[f] != 0u) { continue; }
        var diag = 0.0;
        var sum = 0.0;
        for (var k = 0u; k < 3u; k++) {
            for (var d = -1; d <= 1; d += 2) {
                let qn = q + axis(k) * d;
                let t = tat(qn);
                if (t != 1u) { diag += 1.0; }
                if (t == 0u) { sum += xf[idx(qn)]; }
            }
        }
        total += bf[f] - (diag * xf[f] - sum);
    }
    bc[c] = 0.5 * total;
}

// La correction du niveau grossier, ajoutée au niveau fin
@compute @workgroup_size(WG)
fn prolong(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    if (tf[c] != 0u) { return; }
    let q = vec3<u32>(at(c)) / 2u;
    let n = cdims();
    xf[c] += xc[q.x + n.x * (q.y + n.y * q.z)];
}

// Types du niveau grossier
@compute @workgroup_size(WG)
fn coarsen(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let c = lin(gid, nwg);
    if (c >= p.n.w) { return; }
    let qc = cat(c);
    var airy = false;
    var wet = false;
    for (var j = 0; j < 8; j++) {
        let t = tf[idx(2 * qc + vec3<i32>(j & 1, (j >> 1u) & 1, j >> 2u))];
        airy = airy || t == 2u;
        wet = wet || t == 0u;
    }
    var kind = 1u;
    if (airy) { kind = 2u; } else if (wet) { kind = 0u; }
    tc[c] = kind;
}
"#;

const SLOT: u64 = 128;
/// Cases de réglages par envoi : au-delà, le travail est envoyé en plusieurs fois
const SLOTS: usize = 512;

struct Programs {
    fields: Kernels,
    pressure: Kernels,
    grid: Kernels,
    slot: u64,
}

// Rangs des noyaux dans chaque jeu
const F_TYPES: usize = 0;
const F_SCA_FWD: usize = 1;
const F_SCA_CORR: usize = 2;
const F_VEL_FWD: usize = 3;
const F_VEL_CORR: usize = 4;
const F_EMIT: usize = 5;
const F_REACT: usize = 6;
const F_CURL: usize = 7;
const F_CONFINE: usize = 8;
const F_FORCES: usize = 9;
const F_RESIDUAL: usize = 10;
const F_PROJECT: usize = 11;
const P_DOTS: usize = 0;
const P_RED_AB: usize = 1;
const P_RED_BA: usize = 2;
const P_FIN_A: usize = 3;
const P_FIN_B: usize = 4;
const P_UPDATE_PR: usize = 5;
const P_UPDATE_S: usize = 6;
const G_SMOOTH: usize = 0;
const G_RESTRICT: usize = 1;
const G_PROLONG: usize = 2;
const G_COARSEN: usize = 3;

fn programs(g: &Gpu) -> &'static Programs {
    static P: OnceLock<Programs> = OnceLock::new();
    P.get_or_init(|| {
        let slot = g.slot_size(SLOT);
        let binds = |n: usize| std::iter::once(Bind::Slot(SLOT)).chain(std::iter::repeat_n(Bind::Write, n)).collect::<Vec<_>>();
        Programs {
            fields: g.kernels("fumée-champs", &format!("{COMMON}{FIELDS}"), &binds(8), &["types", "advect_sca_fwd", "advect_sca_corr", "advect_vel_fwd", "advect_vel_corr", "emit", "react", "curl", "confine", "forces", "residual", "project"]),
            pressure: g.kernels("fumée-pression", &format!("{COMMON}{PRESSURE}"), &binds(8), &["dots", "reduce_ab", "reduce_ba", "last_a", "last_b", "update_pr", "update_s"]),
            grid: g.kernels("fumée-multigrille", &format!("{COMMON}{MULTIGRID}"), &binds(6), &["relax", "gather", "prolong", "coarsen"]),
            slot,
        }
    })
}

/// Une case de réglages : dimensions, nombre d'éléments du lancement, quatre entiers, des vecteurs
fn slot(d: Dim, count: u64, k: [u32; 4], f: &[[f32; 4]]) -> [u32; 32] {
    let mut s = [0u32; 32];
    s[..4].copy_from_slice(&[d.nx as u32, d.ny as u32, d.nz as u32, count as u32]);
    s[4..8].copy_from_slice(&k);
    for (i, v) in f.iter().take(6).enumerate() {
        for j in 0..4 {
            s[8 + i * 4 + j] = v[j].to_bits();
        }
    }
    s
}

enum Call {
    Run(u8, usize, usize, u32, u64),
    Copy(u8, u8),
}

/// La fumée sur la carte : ses tampons, ses branchements
pub struct GpuSmoke {
    g: &'static Gpu,
    d: Dim,
    origin: [f32; 3],
    h: f32,
    levels: Vec<Dim>,
    vel: Column,
    sca: Column,
    ta: Column,
    tb: Column,
    typ: Column,
    mask: Column,
    pr: Column,
    rs: Column,
    s: Column,
    /// Par niveau : inconnue, second membre, types (au niveau 0 : z, puis rs et typ)
    lx: Vec<Column>,
    lb: Vec<Column>,
    lt: Vec<Column>,
    pa: Column,
    pb: Column,
    sc: Column,
    slots: wgpu::Buffer,
    ga: wgpu::BindGroup,
    gb: wgpu::BindGroup,
    gc: Vec<wgpu::BindGroup>,
    epoch: Arc<AtomicU64>,
    typed_for: Option<Params>,
    mask_id: u64,
    /// Travail en cours de préparation
    plan_slots: Vec<[u32; 32]>,
    plan: Vec<Call>,
    /// Scalaires du dernier pas envoyé (divergence avant et après), et l'heure de l'envoi
    stat: Option<(Readback, std::time::Instant, usize)>,
    last: Option<Arc<Shot>>,
}

impl GpuSmoke {
    /// Prépare les tampons. Rien si la grille ne tient pas dans ce que la carte laisse brancher.
    pub fn new(g: &'static Gpu, res: [usize; 3], origin: [f32; 3], h: f32) -> Option<GpuSmoke> {
        let d = Dim::of(res);
        let n = d.cells();
        if n as u64 * 8 > g.max_binding.min(g.max_buffer) || n > u32::MAX as usize / 2 || g.max_storage < 8 {
            return None;
        }
        let pr = programs(g);
        let levels = d.levels();
        let col = |label: &str, len: usize, stride: usize| g.column(label, len.max(4), stride);
        let (vel, sca, ta, tb) = (col("fumée-vitesse", n, 8), col("fumée-champs", n, 8), col("fumée-a", n, 8), col("fumée-b", n, 8));
        let (typ, mask) = (col("fumée-types", n, 4), col("fumée-masque", 4, 4));
        let (p, rs, s) = (col("fumée-pression", n, 4), col("fumée-residu", n, 4), col("fumée-direction", n, 4));
        let (mut lx, mut lb, mut lt) = (vec![], vec![], vec![]);
        for (l, dl) in levels.iter().enumerate() {
            let m = if l == 0 { 4 } else { dl.cells() };
            lx.push(col("fumée-niveau-x", dl.cells(), 4));
            lb.push(col("fumée-niveau-b", m, 4));
            lt.push(col("fumée-niveau-t", m, 4));
        }
        // Un niveau de plus, vide : ce que le plus grossier branche à la place de son parent
        for v in [&mut lx, &mut lb, &mut lt] {
            v.push(col("fumée-vide", 4, 4));
        }
        let m1 = n.div_ceil(64);
        let (pa, pb, sc) = (col("fumée-partiel-a", m1, 4), col("fumée-partiel-b", m1.div_ceil(64), 4), col("fumée-scalaires", 16, 4));
        let slots = g.device.create_buffer(&wgpu::BufferDescriptor { label: Some("fumée-reglages"), size: pr.slot * SLOTS as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let b = |c: &Column| c.pages[0].clone();
        let ga = g.group(&pr.fields, &[&slots, &b(&vel), &b(&sca), &b(&ta), &b(&tb), &b(&typ), &b(&mask), &b(&p), &b(&rs)]);
        let gb = g.group(&pr.pressure, &[&slots, &b(&p), &b(&rs), &b(&lx[0]), &b(&s), &b(&typ), &b(&pa), &b(&pb), &b(&sc)]);
        let gc = (0..levels.len())
            .map(|l| {
                let (bf, tf) = if l == 0 { (b(&rs), b(&typ)) } else { (b(&lb[l]), b(&lt[l])) };
                g.group(&pr.grid, &[&slots, &b(&lx[l]), &bf, &tf, &b(&lx[l + 1]), &b(&lb[l + 1]), &b(&lt[l + 1])])
            })
            .collect();
        if g.broken() {
            return None;
        }
        Some(GpuSmoke { g, d, origin, h, levels, vel, sca, ta, tb, typ, mask, pr: p, rs, s, lx, lb, lt, pa, pb, sc, slots, ga, gb, gc, epoch: Arc::new(AtomicU64::new(1)), typed_for: None, mask_id: 0, plan_slots: vec![], plan: vec![], stat: None, last: None })
    }

    /// Octets tenus sur la carte
    pub fn bytes(&self) -> u64 {
        [&self.vel, &self.sca, &self.ta, &self.tb, &self.typ, &self.mask, &self.pr, &self.rs, &self.s, &self.pa, &self.pb].iter().map(|c| c.bytes()).sum::<u64>() + self.lx.iter().chain(&self.lb).chain(&self.lt).map(|c| c.bytes()).sum::<u64>()
    }

    pub fn load(&mut self, g: &Grids) {
        if g.sca.len() != self.d.cells() {
            return;
        }
        self.g.write(&self.sca, &g.sca);
        self.g.write(&self.vel, &g.vel);
        let mut enc = self.g.device.create_command_encoder(&Default::default());
        enc.clear_buffer(&self.pr.pages[0], 0, None);
        self.g.queue.submit([enc.finish()]);
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.last = None;
    }

    pub fn busy(&mut self) -> bool {
        self.stat.as_ref().is_some_and(|s| !s.0.ready())
    }

    pub fn wait(&mut self) {
        if let Some(s) = &self.stat {
            s.0.block();
        }
    }

    /// Chiffres du dernier pas fini, s'ils sont arrivés depuis la dernière demande
    pub fn stats(&mut self) -> Option<(Stats, f32)> {
        if !self.stat.as_ref().is_some_and(|s| s.0.ready()) {
            return None;
        }
        let (rb, t0, iterations) = self.stat.take()?;
        let ms = t0.elapsed().as_secs_f32() * 1000.0;
        let v = rb.take::<f32>().ok()?;
        Some((Stats { before: v[5] / self.h, after: v[3] / self.h, iterations }, ms))
    }

    fn run(&mut self, set: u8, kernel: usize, group: usize, s: [u32; 32], count: u64) {
        let at = match self.plan_slots.iter().position(|x| *x == s) {
            Some(k) => k,
            None => {
                if self.plan_slots.len() >= SLOTS {
                    self.flush();
                }
                self.plan_slots.push(s);
                self.plan_slots.len() - 1
            }
        };
        self.plan.push(Call::Run(set, kernel, group, at as u32, count));
    }

    /// Envoie à la carte le travail préparé
    fn flush(&mut self) {
        if self.plan.is_empty() {
            return;
        }
        let pr = programs(self.g);
        let mut bytes = vec![0u8; self.plan_slots.len() * pr.slot as usize];
        for (i, s) in self.plan_slots.iter().enumerate() {
            bytes[i * pr.slot as usize..][..SLOT as usize].copy_from_slice(gpusim::bytes_of(s));
        }
        self.g.queue.write_buffer(&self.slots, 0, &bytes);
        let mut enc = self.g.device.create_command_encoder(&Default::default());
        let mut pass: Option<wgpu::ComputePass> = None;
        let buf = |k: u8| match k {
            0 => &self.sca.pages[0],
            1 => &self.vel.pages[0],
            2 => &self.ta.pages[0],
            _ => &self.tb.pages[0],
        };
        for call in &self.plan {
            match call {
                Call::Run(set, kernel, group, at, count) => {
                    let (k, grp) = match set {
                        0 => (&pr.fields, &self.ga),
                        1 => (&pr.pressure, &self.gb),
                        _ => (&pr.grid, &self.gc[*group]),
                    };
                    let ps = pass.get_or_insert_with(|| enc.begin_compute_pass(&Default::default()).forget_lifetime());
                    self.g.launch(ps, &k.pipelines[*kernel], grp, &[*at * pr.slot as u32], *count);
                }
                Call::Copy(from, to) => {
                    pass = None;
                    enc.copy_buffer_to_buffer(buf(*from), 0, buf(*to), 0, (self.d.cells() * 8) as u64);
                }
            }
        }
        drop(pass);
        self.g.queue.submit([enc.finish()]);
        self.plan.clear();
        self.plan_slots.clear();
    }

    /// Réduit sur la carte une grandeur (0 : r·z, 1 : s·A·s, 2 : le plus grand |r|) vers la case `dest` des scalaires
    fn reduce(&mut self, kind: u32, dest: u32) {
        let d = self.d;
        let op = (kind == 2) as u32;
        let mut count = d.cells().div_ceil(64);
        self.run(1, P_DOTS, 0, slot(d, count as u64, [kind, 0, 0, 0], &[]), count as u64);
        let mut in_a = true;
        while count > 1024 {
            let next = count.div_ceil(64);
            self.run(1, if in_a { P_RED_AB } else { P_RED_BA }, 0, slot(d, next as u64, [count as u32, op, 0, 0], &[]), next as u64);
            count = next;
            in_a = !in_a;
        }
        self.run(1, if in_a { P_FIN_A } else { P_FIN_B }, 0, slot(d, 1, [count as u32, op, dest, 0], &[]), 1);
    }

    fn smooth(&mut self, l: usize, color: u32, first: bool) {
        let d = self.levels[l];
        self.run(2, G_SMOOTH, l, slot(d, d.cells() as u64, [color, first as u32, 0, 0], &[]), d.cells() as u64);
    }

    /// z = M⁻¹ r : un cycle en V, dans le même ordre que `smoke::vcycle`
    fn vcycle(&mut self) {
        let last = self.levels.len() - 1;
        for l in 0..last {
            for k in 0..SWEEPS {
                self.smooth(l, 0, k == 0);
                self.smooth(l, 1, false);
            }
            let (d, dc) = (self.levels[l], self.levels[l + 1]);
            self.run(2, G_RESTRICT, l, slot(d, dc.cells() as u64, [0; 4], &[]), dc.cells() as u64);
        }
        for k in 0..COARSE_SWEEPS {
            self.smooth(last, 0, k == 0);
            self.smooth(last, 1, false);
        }
        for _ in 0..COARSE_SWEEPS {
            self.smooth(last, 1, false);
            self.smooth(last, 0, false);
        }
        for l in (0..last).rev() {
            let d = self.levels[l];
            self.run(2, G_PROLONG, l, slot(d, d.cells() as u64, [0; 4], &[]), d.cells() as u64);
            for _ in 0..SWEEPS {
                self.smooth(l, 1, false);
                self.smooth(l, 0, false);
            }
        }
    }

    fn set_types(&mut self, par: &Params) {
        if self.typed_for.as_ref().is_some_and(|o| o.borders == par.borders && o.ball == par.ball && o.cube == par.cube) {
            return;
        }
        let (d, n) = (self.d, self.d.cells() as u64);
        let ball = par.ball.map(|(c, r)| [c[0], c[1], c[2], r]).unwrap_or([0.0; 4]);
        let (cc, ch) = par.cube.map(|(c, h)| ([c[0], c[1], c[2], 1.0], [h[0], h[1], h[2], 0.0])).unwrap_or(([0.0; 4], [0.0; 4]));
        self.run(0, F_TYPES, 0, slot(d, n, [par.borders, 0, 0, 0], &[ball, cc, ch, [self.origin[0], self.origin[1], self.origin[2], self.h]]), n);
        for l in 0..self.levels.len() - 1 {
            let (dl, dc) = (self.levels[l], self.levels[l + 1]);
            self.run(2, G_COARSEN, l, slot(dl, dc.cells() as u64, [0; 4], &[]), dc.cells() as u64);
        }
        self.typed_for = Some(*par);
    }

    fn emit(&mut self, e: &Emit, scale: f32) {
        let (d, n) = (self.d, self.d.cells() as u64);
        let soft = e.soft.max(self.h * 0.5);
        let (shape, a, b) = match &e.shape {
            Shape::Ball(c, r) => (0, [c[0], c[1], c[2], *r], [0.0, 0.0, 0.0, soft]),
            Shape::Cube(c, h) => (1, [c[0], c[1], c[2], 0.0], [h[0], h[1], h[2], soft]),
            Shape::Mask(m) => {
                if m.w.len() != d.cells() {
                    return;
                }
                if self.mask_id != m.id {
                    // Le masque change : ce qui précède part d'abord, avec l'ancien
                    self.flush();
                    if self.mask.len != d.cells() {
                        self.mask = self.g.column("fumée-masque", d.cells(), 4);
                        let pr = programs(self.g);
                        let c = |c: &Column| c.pages[0].clone();
                        self.ga = self.g.group(&pr.fields, &[&self.slots, &c(&self.vel), &c(&self.sca), &c(&self.ta), &c(&self.tb), &c(&self.typ), &c(&self.mask), &c(&self.pr), &c(&self.rs)]);
                    }
                    self.g.write(&self.mask, &m.w);
                    self.mask_id = m.id;
                }
                (2, [0.0; 4], [0.0, 0.0, 0.0, soft])
            }
        };
        let moving = e.velocity.iter().any(|v| *v != 0.0) as u32;
        let s = slot(d, n, [shape, e.add as u32, e.seed, moving], &[a, b, [e.density, e.temperature, e.fuel, e.noise], [e.velocity[0], e.velocity[1], e.velocity[2], 0.0], [self.origin[0], self.origin[1], self.origin[2], self.h], [e.dt * scale, 0.0, 0.0, 0.0]]);
        self.run(0, F_EMIT, 0, s, n);
    }

    /// Avance d'une image : tout le travail est envoyé, rien n'est attendu
    pub fn step(&mut self, dt: f32, par: &Params, ops: &[Op]) {
        let (d, n, h) = (self.d, self.d.cells() as u64, self.h);
        let t0 = std::time::Instant::now();
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.last = None;
        self.set_types(par);
        let sub = par.substeps.clamp(1, 64);
        let iters = par.iterations.max(1);
        let burn = ops.iter().rev().find_map(|o| if let Op::Burn(b) = o { Some(*b) } else { None });
        let plain = |k: [u32; 4], f: &[[f32; 4]]| slot(d, n, k, f);
        for k in 0..sub {
            let dt = dt / sub as f32;
            let adv = plain([0; 4], &[[dt / h, 0.0, 0.0, 0.0]]);
            // La fumée puis la vitesse : les deux lisent la vitesse d'avant
            let from = if par.maccormack { 3 } else { 2 };
            self.run(0, F_SCA_FWD, 0, adv, n);
            if par.maccormack {
                self.run(0, F_SCA_CORR, 0, adv, n);
            }
            self.plan.push(Call::Copy(from, 0));
            self.run(0, F_VEL_FWD, 0, adv, n);
            if par.maccormack {
                self.run(0, F_VEL_CORR, 0, adv, n);
            }
            self.plan.push(Call::Copy(from, 1));
            for op in ops {
                if let Op::Emit(e) = op {
                    self.emit(&Emit { seed: e.seed.wrapping_add(k as u32), ..e.clone() }, 1.0 / sub as f32);
                }
            }
            let r = React::new(par, burn.map(|b| Burn { dt, ..b }), dt);
            let b = r.burn.unwrap_or(Burn { ignition: 0.0, rate: 0.0, heat: 0.0, smoke: 0.0, expansion: 0.0, cooling: 0.0, dt });
            self.run(0, F_REACT, 0, plain([r.burn.is_some() as u32, 0, 0, 0], &[[r.keep_density, r.keep_heat, dt, r.fade()], [b.ignition, b.rate, b.heat, b.smoke], [b.expansion, b.cooling, 0.0, 0.0]]), n);
            let swirl = par.vorticity > 0.0;
            if swirl {
                self.run(0, F_CURL, 0, plain([0; 4], &[[1.0 / h, 0.0, 0.0, 0.0]]), n);
                self.run(0, F_CONFINE, 0, plain([0; 4], &[[par.vorticity * h, 0.0, 0.0, 0.0]]), n);
            }
            let f = Forces::new(par, dt);
            self.run(0, F_FORCES, 0, plain([swirl as u32, 0, 0, 0], &[[f.dt, f.buoyancy, f.weight, f.keep], [f.wind[0], f.wind[1], f.wind[2], f.follow]]), n);
            // Pression : le même ordre que `smoke::Cpu::pressure`
            self.run(0, F_RESIDUAL, 0, plain([0; 4], &[[h, 0.0, 0.0, 0.0]]), n);
            self.reduce(2, 5);
            self.vcycle();
            self.reduce(0, 0);
            self.run(1, P_UPDATE_S, 0, plain([0, 1, 0, 0], &[]), n);
            for it in 0..iters {
                let cur = (it % 2) as u32;
                self.reduce(1, 2);
                self.run(1, P_UPDATE_PR, 0, plain([cur, 0, 0, 0], &[]), n);
                if it + 1 == iters {
                    break;
                }
                self.vcycle();
                self.reduce(0, 1 - cur);
                self.run(1, P_UPDATE_S, 0, plain([1 - cur, 0, 0, 0], &[]), n);
            }
            self.reduce(2, 3);
            self.run(0, F_PROJECT, 0, plain([0; 4], &[]), n);
        }
        self.flush();
        self.stat = Some((self.g.read(&self.sc), t0, iters));
    }

    /// L'état à la fin du dernier pas : relecture demandée, pas attendue
    pub fn shot(&mut self) -> Arc<Shot> {
        if let Some(s) = &self.last {
            return s.clone();
        }
        let live = Live::new(self.sca.pages[0].clone(), [self.d.nx, self.d.ny, self.d.nz], self.epoch.clone());
        let s = Arc::new(Shot::waiting(live, self.d.cells(), self.g.read(&self.sca), self.g.read(&self.vel)));
        self.last = Some(s.clone());
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smoke::{Cpu, Sim};
    use crate::volume::from_half;

    fn source() -> Vec<Op> {
        vec![
            Op::Emit(Emit { shape: Shape::Ball([0.5, 0.22, 0.5], 0.12), density: 1.0, temperature: 1.2, fuel: 0.8, velocity: [0.0, 0.6, 0.0], add: false, soft: 0.03, noise: 0.5, seed: 3, dt: 1.0 / 24.0 }),
            Op::Emit(Emit { shape: Shape::Cube([0.3, 0.5, 0.5], [0.06, 0.06, 0.06]), density: 2.0, temperature: 0.0, fuel: 0.0, velocity: [0.0; 3], add: true, soft: 0.0, noise: 0.0, seed: 0, dt: 1.0 / 24.0 }),
            Op::Burn(Burn { ignition: 0.4, rate: 3.0, heat: 2.0, smoke: 0.6, expansion: 0.8, cooling: 0.3, dt: 1.0 / 24.0 }),
        ]
    }

    /// Plus grand écart entre deux états, rapporté à la plus grande valeur
    fn gap(a: &Grids, b: &Grids) -> (f32, f32) {
        let d = |x: &[[u16; 4]], y: &[[u16; 4]]| {
            let (mut worst, mut top) = (0.0f32, 1e-6f32);
            for (p, q) in x.iter().zip(y) {
                for k in 0..4 {
                    let (u, v) = (from_half(p[k]), from_half(q[k]));
                    worst = worst.max((u - v).abs());
                    top = top.max(u.abs());
                }
            }
            worst / top
        };
        (d(&a.sca, &b.sca), d(&a.vel, &b.vel))
    }

    #[test]
    fn carte_contre_processeur() {
        let Some(_) = gpusim::gpu() else { return };
        let n = 32;
        let par = Params { substeps: 2, iterations: 3, ball: Some(([0.5, 0.6, 0.5], 0.1)), cube: Some(([0.7, 0.4, 0.5], [0.05, 0.1, 0.1])), wind: [0.3, 0.0, 0.0], damping: 0.1, ..Default::default() };
        let mut cpu = Cpu::new([n, n, n], [0.0; 3], 1.0 / n as f32);
        let mut gpu = Sim::new([n, n, n], [0.0; 3], 1.0 / n as f32, true);
        assert!(gpu.on_gpu());
        let ops = source();
        for step in 0..4 {
            cpu.step(1.0 / 24.0, &par, &ops);
            gpu.step(1.0 / 24.0, &par, &ops);
            let got = gpu.shot().grids();
            let (s, v) = gap(&cpu.g, &got);
            let (st, _) = gpu.stats_now();
            println!("pas {step} : écart fumée {s:.2e}, vitesse {v:.2e} ; divergence carte {:.2e} → {:.2e}, processeur {:.2e} → {:.2e}", st.before, st.after, cpu.stats.before, cpu.stats.after);
            assert!(s < 0.02 && v < 0.02, "pas {step} : écarts {s} {v}");
            assert!(st.before > 0.0 && (st.before - cpu.stats.before).abs() < cpu.stats.before * 0.05, "{st:?} {:?}", cpu.stats);
            assert!(st.after < st.before * 0.05, "{st:?}");
        }
        // Il s'est passé quelque chose : de la fumée, de la flamme, du mouvement
        let got = gpu.shot().grids();
        assert!(got.sca.iter().any(|v| from_half(v[0]) > 0.5) && got.sca.iter().any(|v| from_half(v[3]) > 0.1) && got.vel.iter().any(|v| from_half(v[1]) > 0.2));
    }

    #[test]
    fn sans_maccormack_et_par_un_masque() {
        let Some(_) = gpusim::gpu() else { return };
        let (res, h) = ([32usize, 16, 16], 1.0 / 16.0);
        let d = Dim::of(res);
        let w: Vec<f32> = (0..d.cells()).map(|c| if d.at(c) == [10, 4, 8] || d.at(c) == [20, 5, 8] { 1.0 } else { 0.0 }).collect();
        let ops = vec![Op::Emit(Emit { shape: Shape::Mask(Arc::new(crate::smoke::Mask { w, id: 77 })), density: 1.0, temperature: 2.0, fuel: 0.0, velocity: [0.0; 3], add: false, soft: 0.0, noise: 0.0, seed: 0, dt: 1.0 / 24.0 })];
        let par = Params { maccormack: false, vorticity: 0.0, borders: 1, ..Default::default() };
        let mut cpu = Cpu::new(res, [0.0; 3], h);
        let mut gpu = Sim::new(res, [0.0; 3], h, true);
        for _ in 0..3 {
            cpu.step(1.0 / 24.0, &par, &ops);
            gpu.step(1.0 / 24.0, &par, &ops);
        }
        let (s, v) = gap(&cpu.g, &gpu.shot().grids());
        assert!(s < 0.02 && v < 0.02, "écarts {s} {v}");
        assert!(cpu.g.sca.iter().any(|v| from_half(v[0]) > 0.5));
    }

    #[test]
    fn un_etat_charge_revient_tel_quel() {
        let Some(_) = gpusim::gpu() else { return };
        let n = 16;
        let mut g = Grids::zero(n * n * n);
        for (i, v) in g.sca.iter_mut().enumerate() {
            *v = crate::volume::pack([i as f32 * 0.001, 1.0, 0.25, 0.0]);
        }
        let mut sim = Sim::new([n, n, n], [0.0; 3], 0.1, true);
        sim.load(&g);
        assert!(*sim.shot().grids() == g);
    }
}
