# Lecture d'un maillage Blender vers le moteur : foreach_get directement dans le fichier partagé,
# triangulation par loop_triangles, matériau et arêtes réelles par triangle.
import time

import bpy
import numpy as np

NOM_MOD = "Aryadne (affichage lourd)"
NOM_GROUPE = ".Aryadne affichage lourd v2"


def groupe_cache():
    """Groupe de nœuds qui remplace la géométrie par les 8 coins de sa boîte, en points de rayon nul :
    Blender ne dessine plus rien en mode Solid, mais garde les bonnes bornes (cadrer la vue avec
    « . » du pavé numérique), l'origine et les gizmos."""
    g = bpy.data.node_groups.get(NOM_GROUPE)
    if g is not None and g.bl_idname == "GeometryNodeTree":
        return g
    g = bpy.data.node_groups.new(NOM_GROUPE, "GeometryNodeTree")
    g.interface.new_socket("Geometry", in_out="INPUT", socket_type="NodeSocketGeometry")
    g.interface.new_socket("Geometry", in_out="OUTPUT", socket_type="NodeSocketGeometry")
    n = g.nodes
    gi = n.new("NodeGroupInput")
    go = n.new("NodeGroupOutput")
    bb = n.new("GeometryNodeBoundBox")
    mp = n.new("GeometryNodeMeshToPoints")
    mp.inputs["Radius"].default_value = 0.0
    gi.location, bb.location, mp.location, go.location = (-500, 0), (-300, 0), (-100, 0), (100, 0)
    g.links.new(gi.outputs[0], bb.inputs[0])
    g.links.new(bb.outputs[0], mp.inputs[0])
    g.links.new(mp.outputs[0], go.inputs[0])
    return g


def modificateur(obj):
    return obj.modifiers.get(NOM_MOD)


def ajouter_cache(obj):
    m = modificateur(obj)
    if m is None:
        m = obj.modifiers.new(NOM_MOD, "NODES")
        m.node_group = groupe_cache()
        m.show_render = False
        m.show_in_editmode = False
        m.show_on_cage = False
        m.show_expanded = False
    return m


def retirer_cache(obj):
    m = modificateur(obj)
    if m is not None:
        obj.modifiers.remove(m)


def autres_modificateurs(obj):
    return [m for m in obj.modifiers if m.name != NOM_MOD and m.show_viewport]


def maillage_evalue(obj, depsgraph):
    """Maillage tel que Blender le montrerait (avec les modificateurs), sans notre cache.
    Rend (mesh, fin) : appeler fin() une fois la lecture faite."""
    if not autres_modificateurs(obj):
        # Pas d'autre modificateur : le maillage d'origine suffit, sans rien réévaluer
        return obj.data, lambda: None
    m = modificateur(obj)
    remis = False
    if m is not None and m.show_viewport:
        m.show_viewport = False
        remis = True
        depsgraph.update()
    ev = obj.evaluated_get(depsgraph)
    me = ev.to_mesh()

    def fin():
        ev.to_mesh_clear()
        if remis:
            m.show_viewport = True

    return me, fin


def compter(me):
    me.calc_loop_triangles()
    return len(me.vertices), len(me.loop_triangles)


def remplisseur(me, aretes=True):
    """Fonction de remplissage pour Moteur.envoyer_maillage, et la liste des extras."""
    nt = len(me.loop_triangles)
    nmat = len(me.materials)
    extras = []
    if nmat > 1:
        extras.append(("materiaux", np.uint32))
    # Arêtes réelles : seulement s'il y a des faces de plus de trois côtés (sinon toutes le sont)
    tout_tri = len(me.polygons) == nt
    if aretes and not tout_tri:
        extras.append(("aretes", np.uint8))

    def remplir(pos, tris, vues):
        me.vertices.foreach_get("co", pos)
        me.loop_triangles.foreach_get("vertices", tris.view(np.int32))
        k = 0
        if nmat > 1:
            mi = np.empty(nt, dtype=np.int32)
            me.loop_triangles.foreach_get("material_index", mi)
            vues[k][:] = np.clip(mi, 0, nmat - 1)
            k += 1
        if aretes and not tout_tri:
            vues[k][:] = aretes_reelles(me)

    return remplir, extras


def aretes_reelles(me):
    """Pour chaque triangle, les côtés qui sont de vraies arêtes du maillage (pas les diagonales
    ajoutées par la triangulation) : bit 0 v0-v1, bit 1 v1-v2, bit 2 v2-v0."""
    nt = len(me.loop_triangles)
    lo = np.empty(nt * 3, dtype=np.int32)
    me.loop_triangles.foreach_get("loops", lo)
    lo = lo.reshape(nt, 3)
    pi = np.empty(nt, dtype=np.int32)
    me.loop_triangles.foreach_get("polygon_index", pi)
    tot = np.empty(len(me.polygons), dtype=np.int32)
    me.polygons.foreach_get("loop_total", tot)
    t = tot[pi]
    out = np.zeros(nt, dtype=np.uint8)
    for bit, (a, b) in enumerate(((0, 1), (1, 2), (2, 0))):
        d = (lo[:, b] - lo[:, a]) % t
        out |= (((d == 1) | (d == t - 1)).astype(np.uint8) << bit)
    return out


def envoyer(moteur, ident, obj, depsgraph, **champs):
    """Envoie le maillage évalué de `obj` au moteur. Rend (réponse, ms de lecture Blender)."""
    t0 = time.perf_counter()
    me, fin = maillage_evalue(obj, depsgraph)
    try:
        nv, nt = compter(me)
        remplir, extras = remplisseur(me)
        lu = [0.0]

        def remplir_mesure(pos, tris, vues):
            remplir(pos, tris, vues)
            lu[0] = (time.perf_counter() - t0) * 1000.0

        r = moteur.envoyer_maillage(ident, remplir_mesure, nv, nt, extras, **champs)
    finally:
        fin()
    r["blender_ms"] = round(lu[0], 2)
    r["total_ms"] = round((time.perf_counter() - t0) * 1000.0, 2)
    return r


def signature(obj):
    """Empreinte du maillage d'origine quand aucun autre modificateur ne le change (pour ne pas
    renvoyer une géométrie identique après nos propres changements ou un « annuler » sans effet).
    None quand il y a d'autres modificateurs : on renvoie alors à chaque changement signalé."""
    if autres_modificateurs(obj):
        return None
    import zlib

    me = obj.data
    co = np.empty(len(me.vertices) * 3, dtype=np.float32)
    me.vertices.foreach_get("co", co)
    return (len(me.vertices), len(me.polygons), len(me.loops), zlib.crc32(co.tobytes()), tuple(m.name if m else "" for m in me.materials))
