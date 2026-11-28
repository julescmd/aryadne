# Sphère bruitée de plusieurs millions de triangles, pour essayer l'affichage lourd.
import bpy
import numpy as np


def sphere_bruitee(triangles, rayon=1.0, graine=1):
    """Sommets (n×3 float32), quadrilatères (q×4) et triangles (t×3) d'une sphère UV déformée par un bruit fait de
    vagues superposées. Le nombre de triangles est proche de `triangles`."""
    # Une sphère UV de nu × nv quadrilatères (les pôles en triangles) : environ 2·nu·nv triangles
    nv = max(4, int((triangles / 4.0) ** 0.5))
    nu = 2 * nv
    th = np.linspace(0.0, np.pi, nv + 1, dtype=np.float32)[1:-1]  # anneaux sans les pôles
    ph = np.linspace(0.0, 2.0 * np.pi, nu, endpoint=False, dtype=np.float32)
    T, P = np.meshgrid(th, ph, indexing="ij")
    d = np.stack([np.sin(T) * np.cos(P), np.sin(T) * np.sin(P), np.cos(T)], -1).reshape(-1, 3)
    d = np.concatenate([d, np.array([[0, 0, 1], [0, 0, -1]], dtype=np.float32)])
    rng = np.random.default_rng(graine)
    r = np.ones(len(d), dtype=np.float32)
    amp = 0.12
    for k in range(7):
        f = rng.normal(size=3).astype(np.float32)
        f *= (2.0 + 2.2 ** k) / np.linalg.norm(f)
        ph0 = rng.uniform(0, 6.28)
        r += amp * np.sin(d @ f + ph0) * np.cos(d @ f[[1, 2, 0]] * 0.7 + ph0)
        amp *= 0.55
    pos = (d * (r[:, None] * rayon)).astype(np.float32)
    ring = lambda i: i * nu
    n_anneaux = nv - 1
    i = np.arange(nu)
    j = (i + 1) % nu
    quads = []
    for a in range(n_anneaux - 1):
        quads.append(np.stack([ring(a) + i, ring(a + 1) + i, ring(a + 1) + j, ring(a) + j], 1))
    q = np.concatenate(quads) if quads else np.zeros((0, 4), dtype=np.int64)
    nord, sud = len(d) - 2, len(d) - 1
    capn = np.stack([np.full(nu, nord), ring(0) + i, ring(0) + j], 1)
    caps = np.stack([np.full(nu, sud), ring(n_anneaux - 1) + j, ring(n_anneaux - 1) + i], 1)
    # Faces : quadrilatères, et triangles aux pôles
    return pos, q.astype(np.int32), np.concatenate([capn, caps]).astype(np.int32)


def creer_objet(nom, triangles, collection=None):
    """Objet maillage Blender (faces triangulaires) fait de la sphère bruitée."""
    pos, quads, tris = sphere_bruitee(triangles)
    me = bpy.data.meshes.new(nom)
    me.vertices.add(len(pos))
    me.vertices.foreach_set("co", pos.ravel())
    coins = np.concatenate([quads.ravel(), tris.ravel()])
    me.loops.add(coins.size)
    me.loops.foreach_set("vertex_index", coins)
    me.polygons.add(len(quads) + len(tris))
    debut = np.concatenate([np.arange(0, quads.size, 4), quads.size + np.arange(0, tris.size, 3)]).astype(np.int32)
    me.polygons.foreach_set("loop_start", debut)
    me.update(calc_edges=True)
    ob = bpy.data.objects.new(nom, me)
    (collection or bpy.context.scene.collection).objects.link(ob)
    return ob
