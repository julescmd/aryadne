# Aryadne : affichage lourd dans Blender.
#
# Les objets restent de vrais objets Blender (outliner, modificateurs, annuler, sauvegarde). Seule
# la façon de dessiner les objets marqués « Affichage lourd » change : leur maillage évalué est
# envoyé une fois au moteur Aryadne (programme séparé, `aryadne --moteur`), Blender ne dessine plus
# leurs faces, et la vue 3D reçoit du moteur une image et sa profondeur qu'elle compose avec la
# sienne. Désactiver l'add-on rend tout comme avant.
import atexit
import math
import os
import shutil
import sys
import time

import bpy
from bpy.app.handlers import persistent
from bpy.props import BoolProperty, EnumProperty, FloatProperty, IntProperty, StringProperty
from mathutils import Matrix, Vector

from . import maillage, reglages
from .moteur import ErreurMoteur, Moteur, matrice

# ---------- état ----------


class Region:
    """Ce qu'on sait d'une vue 3D : la dernière image reçue, celle qu'on voudrait."""

    def __init__(self):
        self.image = None  # (rgba, prof, ids)
        self.image_vp = None  # vue-projection de l'image
        self.image_cle = None
        self.image_n = 0
        self.tex = None
        self.tex_n = -1
        self.sig_vue = None
        self.bouge = 0.0  # dernier mouvement de la caméra
        self.voulu = None  # (clé, demande, vp, sync)
        self.taille = (1, 1)
        self.persp = True
        self.view_matrix = None


class Etat:
    def __init__(self):
        self.moteur = None
        self.erreur = ""
        self.objets = {}  # uid -> dict(nom, matrice, visible, couleurs, sig)
        self.version = 0
        self.regions = {}
        self.en_vol = None  # (n, clé de région, clé demandée, vp)
        self.sale = set()  # uids dont la géométrie est à renvoyer
        self.ignorer = {}  # uid -> instant jusqu'auquel on ignore ses mises à jour (nos propres changements)
        self.dernier_op = 0
        self.handle = None
        self.stats = {}
        self.sync = {}  # uid -> ce qui a été envoyé (couleurs, fil, visible)
        self.echecs_dessin = 0

    def actif(self):
        return self.moteur is not None and self.moteur.vivant()


G = Etat()


def prefs():
    return bpy.context.preferences.addons[__package__].preferences


def exe_par_defaut():
    nom = "aryadne.exe" if sys.platform == "win32" else "aryadne"
    return shutil.which(nom) or ""


def chemin_exe():
    p = os.environ.get("ARYADNE_MOTEUR") or bpy.path.abspath(prefs().chemin) or exe_par_defaut()
    return p


def lourds(scene=None):
    scene = scene or bpy.context.scene
    return [o for o in scene.objects if o.type == "MESH" and o.aryadne_lourd]


def par_uid():
    return {o.aryadne_uid: o for o in lourds()}


def redessiner():
    wm = bpy.context.window_manager
    for win in wm.windows:
        for area in win.screen.areas:
            if area.type == "VIEW_3D":
                area.tag_redraw()


# ---------- moteur ----------


def demarrer():
    if G.actif():
        return True
    G.erreur = ""
    try:
        G.moteur = Moteur(chemin_exe())
        G.moteur.demarrer()
    except (ErreurMoteur, OSError) as e:
        G.erreur = str(e)
        G.moteur = None
        return False
    G.objets.clear()
    G.sync.clear()
    G.regions.clear()
    G.en_vol = None
    for o in lourds():
        G.sale.add(assurer_uid(o))
    if not bpy.app.timers.is_registered(tic):
        bpy.app.timers.register(tic, first_interval=0.01, persistent=True)
    return True


def arreter():
    if G.moteur is not None:
        G.moteur.arreter()
    G.moteur = None
    G.en_vol = None
    G.regions.clear()
    G.objets.clear()
    # Sans moteur, Blender redessine lui-même les objets lourds
    for o in lourds_partout():
        montrer_blender(o)


def lourds_partout():
    return [o for o in bpy.data.objects if o.type == "MESH" and o.aryadne_lourd]


atexit.register(lambda: G.moteur and G.moteur.arreter())

# ---------- objets ----------


def assurer_uid(o):
    """Numéro stable de l'objet (aussi écrit dans le tampon d'identifiants). Un objet dupliqué
    hérite du numéro de l'original : il en reçoit un nouveau."""
    pris = {}
    for x in bpy.data.objects:
        if x.aryadne_uid:
            pris.setdefault(x.aryadne_uid, []).append(x)
    if o.aryadne_uid == 0 or (len(pris.get(o.aryadne_uid, [])) > 1 and pris[o.aryadne_uid][0] != o):
        o.aryadne_uid = max(pris.keys(), default=0) + 1
    return o.aryadne_uid


def masquer_blender(o):
    """Blender ne dessine plus les faces de l'objet (il garde l'origine, la boîte pour cadrer, les gizmos)."""
    G.ignorer[o.aryadne_uid] = time.time() + 0.5
    if prefs().methode == "BOITE":
        if "aryadne_affichage" not in o:
            o["aryadne_affichage"] = o.display_type
        o.display_type = "BOUNDS"
    else:
        m = maillage.ajouter_cache(o)
        m.show_viewport = o.mode == "OBJECT"


def montrer_blender(o):
    G.ignorer[o.aryadne_uid] = time.time() + 0.5
    maillage.retirer_cache(o)
    if "aryadne_affichage" in o:
        o.display_type = o["aryadne_affichage"]
        del o["aryadne_affichage"]


def envoyer(o, depsgraph=None):
    depsgraph = depsgraph or bpy.context.evaluated_depsgraph_get()
    uid = assurer_uid(o)
    t0 = time.perf_counter()
    r = maillage.envoyer(G.moteur, str(uid), o, depsgraph, num=uid, matrice=matrice(o.matrix_world))
    G.objets[uid] = {"nom": o.name, "matrice": matrice(o.matrix_world), "sig": maillage.signature(o)}
    G.sync.pop(uid, None)
    G.version += 1
    G.stats = {"triangles": r.get("triangles", 0), "envoi_ms": round((time.perf_counter() - t0) * 1000.0, 1), "blender_ms": r.get("blender_ms"), "moteur_ms": r.get("ms")}
    return r


def maj_lourd(self, context):
    o = self
    if o.aryadne_lourd:
        assurer_uid(o)
        if not G.actif() and prefs().demarrage_auto:
            demarrer()
        if G.actif():
            masquer_blender(o)
            G.sale.add(o.aryadne_uid)
    else:
        montrer_blender(o)
        if G.actif() and o.aryadne_uid in G.objets:
            G.moteur.envoyer({"cmd": "retirer", "id": str(o.aryadne_uid)})
            G.objets.pop(o.aryadne_uid, None)
            G.version += 1
    redessiner()


# ---------- boucle (timer) ----------


def tic():
    if not G.actif():
        if G.moteur is not None:
            # Le moteur s'est arrêté tout seul : Blender reprend le dessin
            G.erreur = "Le moteur s'est arrêté. " + " ".join(G.moteur.journal[-3:])
            arreter()
            redessiner()
        return None
    try:
        recevoir()
        synchroniser()
        selection_auto()
        pomper()
    except Exception as e:  # le timer ne doit jamais mourir
        G.erreur = "%s: %s" % (type(e).__name__, e)
    now = time.time()
    # Une vue qui vient de s'arrêter de bouger attend son image pleine
    for re in G.regions.values():
        if re.voulu and re.bouge and now - re.bouge > prefs().delai_plein:
            redessiner()
            break
    return 1.0 / 60.0


def recevoir():
    for r in G.moteur.reponses():
        if G.en_vol is None or r.get("n") != G.en_vol[0]:
            continue
        _, cle_region, cle, vp, taille = G.en_vol
        G.en_vol = None
        if not r.get("ok"):
            G.erreur = r.get("erreur", "")
            continue
        img = G.moteur.lire_image(r)
        re = G.regions.get(cle_region)
        if img is None or re is None:
            continue
        re.image = img
        re.image_vp = vp
        re.image_cle = cle
        re.image_n += 1
        G.stats["image_ms"] = r.get("rendu_ms")
        G.stats["trace_ms"] = r.get("trace_ms")
        G.stats["image"] = "%d × %d" % (r["largeur"], r["hauteur"])
        redessiner()


def synchroniser():
    """Objets ajoutés, retirés, déplacés, en mode édition, ou dont la géométrie a changé."""
    scene = bpy.context.scene
    dg = bpy.context.evaluated_depsgraph_get()
    vus = set()
    for o in lourds(scene):
        uid = assurer_uid(o)
        vus.add(uid)
        objet_mode = o.mode == "OBJECT"
        m = maillage.modificateur(o)
        if prefs().methode != "BOITE":
            if m is None:
                masquer_blender(o)
                m = maillage.modificateur(o)
            # Hors du mode Objet (édition, sculpture, peinture) : Blender dessine lui-même
            if m.show_viewport != objet_mode:
                G.ignorer[uid] = time.time() + 0.5
                m.show_viewport = objet_mode
                if objet_mode:
                    G.sale.add(uid)
        if uid not in G.objets:
            G.sale.add(uid)
        if uid in G.sale and objet_mode:
            G.sale.discard(uid)
            info = G.objets.get(uid)
            if info is None or info.get("sig") is None or info["sig"] != maillage.signature(o):
                envoyer(o, dg)
        info = G.objets.get(uid)
        if info is None:
            continue
        mw = matrice(o.matrix_world)
        if mw != info["matrice"]:
            info["matrice"] = mw
            G.moteur.envoyer({"cmd": "modifier", "id": str(uid), "matrice": mw})
            G.version += 1
    for uid in list(G.objets):
        if uid not in vus:
            G.moteur.envoyer({"cmd": "retirer", "id": str(uid)})
            G.objets.pop(uid, None)
            G.version += 1


def sync_vue(space, shading):
    """Couleurs, fil de fer et visibilité de chaque objet pour une vue (envoyés s'ils ont changé).
    Rend une signature qui change quand l'image doit être refaite."""
    actif = bpy.context.view_layer.objects.active
    fil_ov, _ = reglages.fil_overlay(space)
    fil = fil_ov or shading.type == "WIREFRAME"
    sig = []
    for uid, o in par_uid().items():
        if uid not in G.objets:
            continue
        vis = o.visible_get(viewport=space) and o.mode == "OBJECT"
        etat = {"visible": vis, "couleurs": reglages.couleurs(o, shading)}
        if fil:
            etat["fil_couleur"] = reglages.couleur_fil(o, actif, shading.type == "WIREFRAME")
        if G.sync.get(uid) != etat:
            G.sync[uid] = etat
            G.moteur.envoyer(dict(etat, cmd="modifier", id=str(uid)))
        sig.append((uid, repr(etat)))
    return tuple(sig)


def pomper():
    """Envoie la demande d'image en attente (une seule à la fois)."""
    if G.en_vol is not None:
        return
    for cle_region, re in G.regions.items():
        if re.voulu is None or re.voulu[0] == re.image_cle:
            continue
        cle, demande, vp, taille = re.voulu
        n, _ = G.moteur.envoyer(demande)
        G.en_vol = (n, cle_region, cle, vp, taille)
        return


# ---------- dessin ----------


def etats_selection():
    actif = bpy.context.view_layer.objects.active
    out = [0.0] * (max(G.objets, default=0) + 1)
    for uid, o in par_uid().items():
        if uid < len(out) and o.select_get():
            out[uid] = 2.0 if o == actif else 1.0
    return out


def preparer_vue(region, rv3d, space, now=None):
    """Clé de la vue et demande d'image ; mémorise la vue voulue."""
    now = now or time.time()
    re = G.regions.setdefault(region.as_pointer(), Region())
    vm, wm = rv3d.view_matrix.copy(), rv3d.window_matrix.copy()
    w, h = region.width, region.height
    sig_vue = (tuple(map(tuple, vm)), tuple(map(tuple, wm)), w, h)
    if sig_vue != re.sig_vue:
        if re.sig_vue is not None:
            re.bouge = now
        re.sig_vue = sig_vue
    bouge = re.bouge and now - re.bouge < prefs().delai_plein
    k = prefs().reduction if bouge else 1.0
    iw, ih = max(1, int(w * k)), max(1, int(h * k))
    shading = space.shading
    sig_obj = sync_vue(space, shading)
    ecl = reglages.eclairage(shading, vm)
    fil, op = reglages.fil_overlay(space)
    ecl["fil"] = fil and shading.type != "WIREFRAME"
    ecl["fil_opacite"] = op
    cle = (sig_vue, iw, ih, G.version, sig_obj, repr(ecl))
    demande = {"cmd": "image", "vue": matrice(vm), "projection": matrice(wm), "largeur": iw, "hauteur": ih, "perspective": bool(rv3d.is_perspective), "eclairage": ecl}
    re.voulu = (cle, demande, wm @ vm, (iw, ih))
    re.taille = (w, h)
    re.persp = bool(rv3d.is_perspective)
    re.view_matrix = vm
    if not bouge and re.image_cle == cle:
        re.bouge = 0.0
    return re


def dessiner():
    if not G.actif() or not G.objets:
        return
    ctx = bpy.context
    region, rv3d, space = ctx.region, ctx.region_data, ctx.space_data
    if region is None or rv3d is None or space is None:
        return
    try:
        re = preparer_vue(region, rv3d, space)
        pomper()
        composer_region(re, rv3d.perspective_matrix.copy(), space)
        G.echecs_dessin = 0
    except Exception as e:
        G.echecs_dessin += 1
        G.erreur = "Dessin : %s: %s" % (type(e).__name__, e)
        if G.echecs_dessin > 30:
            # Le dessin échoue sans cesse : on rend la main à Blender plutôt que de laisser un trou
            arreter()


def composer_region(re, vp, space, lineaire=None):
    from . import dessin

    if re.image is None:
        return False
    if re.tex is None or re.tex_n != re.image_n:
        re.tex = dessin.Textures(*re.image)
        re.tex_n = re.image_n
    t = reglages.theme_vue()
    sel = tuple(t.object_selected) + (1.0,)
    act = tuple(t.object_active) + (1.0,)
    contour = space.overlay.show_overlays and getattr(space.overlay, "show_outline_selected", True)
    alpha = reglages.xray(space.shading)
    if lineaire is None:
        lineaire = prefs().srgb_viewport
    grille = dessin.composer(re.tex, etats_selection(), re.image_vp, vp, re.persp, sel, act, alpha, 1 if contour else 0, lineaire)
    if space.overlay.show_overlays and space.overlay.show_object_origins:
        pts, cols = [], []
        actif = bpy.context.view_layer.objects.active
        for o in par_uid().values():
            if o.select_get() and o.mode == "OBJECT":
                pts.append(tuple(o.matrix_world.translation))
                cols.append(act if o == actif else sel)
        dessin.origines(pts, cols, max(3.0, getattr(bpy.context.preferences.view, "object_origin_size", 6) - 2.0))
    return grille


# ---------- sélection ----------


def objet_sous(px, py, re=None):
    """Objet lourd visible au pixel (px, py) d'une vue (coordonnées de région, origine en bas à
    gauche), ou None. Si un objet de Blender est devant, rend None : la sélection de Blender vaut."""
    regions = [re] if re is not None else list(G.regions.values())
    for re in regions:
        if re.image is None:
            continue
        _, prof, ids = re.image
        ih, iw = prof.shape
        w, h = re.taille
        ix, iy = int(px * iw / max(w, 1)), int(py * ih / max(h, 1))
        if not (0 <= ix < iw and 0 <= iy < ih):
            continue
        uid = int(ids[iy, ix])
        if uid == 0:
            continue
        o = par_uid().get(uid)
        if o is None:
            continue
        # Point touché, dans le monde
        ndc = Vector(((ix + 0.5) / iw * 2 - 1, (iy + 0.5) / ih * 2 - 1, float(prof[iy, ix]) * 2 - 1, 1.0))
        p = re.image_vp.inverted() @ ndc
        p = p.xyz / p.w
        inv = re.view_matrix.inverted()
        if re.persp:
            orig = inv.translation
        else:
            d = (inv.to_3x3() @ Vector((0, 0, -1))).normalized()
            orig = p - d * 1e5
        dirv = p - orig
        dist = dirv.length
        if dist > 0:
            hit, loc, _n, _i, obj_touche, _m = bpy.context.scene.ray_cast(bpy.context.evaluated_depsgraph_get(), orig, dirv / dist, distance=dist * 0.999)
            if hit and (obj_touche is None or not obj_touche.original.aryadne_lourd):
                return None
        return o
    return None


def choisir(o, extend=False, deselect=False, toggle=False):
    vl = bpy.context.view_layer
    if deselect:
        o.select_set(False)
    elif toggle:
        if o.select_get() and vl.objects.active == o:
            o.select_set(False)
        elif o.select_get():
            vl.objects.active = o
        else:
            o.select_set(True)
            vl.objects.active = o
    elif extend:
        o.select_set(True)
        vl.objects.active = o
    else:
        for x in vl.objects:
            if x.select_get():
                x.select_set(False)
        o.select_set(True)
        vl.objects.active = o


def selection_auto():
    """Les clics de sélection passent par Blender (ses raccourcis, ses outils) ; après coup, si le
    clic tombait sur un objet lourd que Blender ne voit pas, on le choisit."""
    wm = bpy.context.window_manager
    if not wm.operators:
        return
    op = wm.operators[-1]
    p = op.as_pointer()
    if p == G.dernier_op:
        return
    G.dernier_op = p
    if bpy.context.mode != "OBJECT":
        return
    props = op.properties
    if op.bl_idname == "VIEW3D_OT_select":
        loc = tuple(props.location)
        o = objet_sous(loc[0], loc[1])
        if o is not None:
            choisir(o, props.extend, props.deselect, props.toggle)
            bpy.ops.ed.undo_push(message="Sélectionner")
            redessiner()
    elif op.bl_idname == "VIEW3D_OT_select_box":
        selection_cadre(props.xmin, props.xmax, props.ymin, props.ymax, props.mode)


def selection_cadre(x0, x1, y0, y1, mode):
    touches = set()
    for re in G.regions.values():
        if re.image is None:
            continue
        _, prof, ids = re.image
        ih, iw = prof.shape
        w, h = re.taille
        a = ids[max(0, int(y0 * ih / h)):min(ih, int(y1 * ih / h) + 1), max(0, int(x0 * iw / w)):min(iw, int(x1 * iw / w) + 1)]
        touches.update(int(u) for u in set(a.ravel().tolist()) if u)
    if not touches:
        return
    objs = par_uid()
    for uid in touches:
        o = objs.get(uid)
        if o is None:
            continue
        if mode == "SUB":
            o.select_set(False)
        elif mode == "XOR":
            o.select_set(not o.select_get())
        else:
            o.select_set(True)
    bpy.ops.ed.undo_push(message="Sélection au cadre")
    redessiner()


# ---------- gestionnaires ----------


@persistent
def apres_depsgraph(scene, depsgraph):
    if not G.actif():
        return
    now = time.time()
    for u in depsgraph.updates:
        idb = u.id
        if isinstance(idb, bpy.types.Object) and idb.type == "MESH" and idb.original.aryadne_lourd and u.is_updated_geometry:
            uid = idb.original.aryadne_uid
            if G.ignorer.get(uid, 0) > now:
                continue
            G.sale.add(uid)
        elif isinstance(idb, bpy.types.Mesh):
            for o in lourds(scene):
                if o.data == idb.original:
                    G.sale.add(o.aryadne_uid)


@persistent
def avant_sauvegarde(*_):
    # Le fichier enregistré ne garde aucune trace de l'affichage lourd : sans l'add-on, il s'ouvre normalement
    for o in lourds_partout():
        montrer_blender(o)


@persistent
def apres_sauvegarde(*_):
    if G.actif():
        for o in lourds_partout():
            masquer_blender(o)


@persistent
def apres_chargement(*_):
    G.objets.clear()
    G.sync.clear()
    G.regions.clear()
    G.en_vol = None
    if G.actif():
        G.moteur.envoyer({"cmd": "vider"})
        for o in lourds_partout():
            masquer_blender(o)
            G.sale.add(assurer_uid(o))
    elif lourds_partout() and prefs().demarrage_auto and chemin_exe():
        demarrer()
        for o in lourds_partout():
            masquer_blender(o)


@persistent
def apres_annuler(*_):
    # Annuler peut rendre ou retirer le modificateur et changer la géométrie : on revérifie tout
    if G.actif():
        for o in lourds_partout():
            G.sale.add(assurer_uid(o))
        for o in bpy.data.objects:
            if not o.aryadne_lourd and maillage.modificateur(o) is not None:
                montrer_blender(o)
        G.sync.clear()
        G.version += 1


# ---------- interface ----------


class AryadnePreferences(bpy.types.AddonPreferences):
    bl_idname = __package__

    chemin: StringProperty(name="Exécutable Aryadne", subtype="FILE_PATH", description="Le programme aryadne (aryadne.exe sous Windows)")
    demarrage_auto: BoolProperty(name="Démarrer le moteur au besoin", default=True)
    reduction: FloatProperty(name="Taille pendant le mouvement", default=0.5, min=0.2, max=1.0, subtype="FACTOR", description="Image réduite pendant que la vue bouge, pleine ensuite")
    delai_plein: FloatProperty(name="Délai avant l'image pleine", default=0.2, min=0.0, max=2.0, subtype="TIME_ABSOLUTE", unit="TIME_ABSOLUTE")
    srgb_viewport: BoolProperty(name="Couleurs linéaires vers la vue", default=True, description="La vue 3D de Blender encode elle-même ses couleurs en sRGB. Décoche si l'objet lourd paraît délavé ou trop sombre par rapport à Blender")
    methode: EnumProperty(
        name="Masquer l'objet dans Blender par",
        items=[
            ("MODIF", "Modificateur (recommandé)", "Un modificateur de vue, retiré à l'enregistrement, ne laisse que les bornes : rien à l'écran"),
            ("BOITE", "Boîte", "Affichage en boîte : Blender dessine la boîte de l'objet"),
        ],
        default="MODIF",
    )

    def draw(self, context):
        l = self.layout
        l.prop(self, "chemin")
        if not self.chemin and exe_par_defaut():
            l.label(text="Trouvé : " + exe_par_defaut(), icon="CHECKMARK")
        l.prop(self, "demarrage_auto")
        l.prop(self, "reduction")
        l.prop(self, "delai_plein")
        l.prop(self, "methode")
        l.prop(self, "srgb_viewport")
        etat_ui(l)


def etat_ui(l):
    r = l.row(align=True)
    if G.actif():
        r.label(text="Moteur en marche (%s)" % (G.moteur.version or "?"), icon="CHECKMARK")
        r.operator("aryadne.arreter", text="", icon="PAUSE")
    else:
        r.label(text="Moteur arrêté", icon="RADIOBUT_OFF")
        r.operator("aryadne.demarrer", text="", icon="PLAY")
    if G.erreur:
        l.label(text=G.erreur[:120], icon="ERROR")


class ARYADNE_OT_demarrer(bpy.types.Operator):
    bl_idname = "aryadne.demarrer"
    bl_label = "Démarrer le moteur"
    bl_description = "Lance le moteur Aryadne en arrière-plan"

    def execute(self, context):
        if not demarrer():
            self.report({"ERROR"}, G.erreur)
            return {"CANCELLED"}
        for o in lourds_partout():
            masquer_blender(o)
        redessiner()
        return {"FINISHED"}


class ARYADNE_OT_arreter(bpy.types.Operator):
    bl_idname = "aryadne.arreter"
    bl_label = "Arrêter le moteur"
    bl_description = "Arrête le moteur ; Blender redessine lui-même les objets lourds"

    def execute(self, context):
        arreter()
        redessiner()
        return {"FINISHED"}


class ARYADNE_OT_scene_lourde(bpy.types.Operator):
    bl_idname = "aryadne.scene_lourde"
    bl_label = "Tester avec une scène lourde"
    bl_description = "Ajoute une sphère bruitée de plusieurs millions de triangles, en affichage lourd"
    bl_options = {"REGISTER", "UNDO"}

    millions: FloatProperty(name="Millions de triangles", default=5.0, min=0.1, max=40.0)

    def execute(self, context):
        from . import scene_test

        t0 = time.perf_counter()
        o = scene_test.creer_objet("Sphère lourde", int(self.millions * 1e6), context.collection)
        o.location = context.scene.cursor.location
        for x in context.view_layer.objects:
            x.select_set(False)
        o.select_set(True)
        context.view_layer.objects.active = o
        o.aryadne_lourd = True
        self.report({"INFO"}, "%d triangles créés en %.1f s" % (len(o.data.polygons), time.perf_counter() - t0))
        return {"FINISHED"}


class VIEW3D_PT_aryadne(bpy.types.Panel):
    bl_space_type = "VIEW_3D"
    bl_region_type = "UI"
    bl_category = "Aryadne"
    bl_label = "Affichage lourd"

    def draw(self, context):
        l = self.layout
        o = context.object
        if o is not None and o.type == "MESH":
            l.prop(o, "aryadne_lourd", text="Affichage lourd (Aryadne)")
            if o.aryadne_lourd and o.mode != "OBJECT":
                l.label(text="Hors du mode Objet : Blender dessine l'objet", icon="INFO")
        etat_ui(l)
        if G.stats:
            c = l.column(align=True)
            s = G.stats
            if s.get("triangles"):
                c.label(text="%s triangles" % format(s["triangles"], ",").replace(",", " "))
            if s.get("envoi_ms") is not None:
                c.label(text="Envoi : %.0f ms" % s["envoi_ms"])
            if s.get("image_ms") is not None:
                c.label(text="Image %s : %.0f ms" % (s.get("image", ""), s["image_ms"]))
        l.operator("aryadne.scene_lourde", icon="MESH_UVSPHERE")


def menu_ajout(self, context):
    self.layout.operator("aryadne.scene_lourde", text="Sphère lourde (Aryadne)", icon="MESH_UVSPHERE")


CLASSES = (AryadnePreferences, ARYADNE_OT_demarrer, ARYADNE_OT_arreter, ARYADNE_OT_scene_lourde, VIEW3D_PT_aryadne)
HANDLERS = (
    (bpy.app.handlers.depsgraph_update_post, apres_depsgraph),
    (bpy.app.handlers.save_pre, avant_sauvegarde),
    (bpy.app.handlers.save_post, apres_sauvegarde),
    (bpy.app.handlers.load_post, apres_chargement),
    (bpy.app.handlers.undo_post, apres_annuler),
    (bpy.app.handlers.redo_post, apres_annuler),
)


def register():
    for c in CLASSES:
        bpy.utils.register_class(c)
    bpy.types.Object.aryadne_lourd = BoolProperty(name="Affichage lourd (Aryadne)", description="Le moteur Aryadne dessine cet objet : très rapide pour les maillages de millions de triangles", default=False, update=maj_lourd)
    bpy.types.Object.aryadne_uid = IntProperty(default=0, options={"HIDDEN"})
    for liste, f in HANDLERS:
        if f not in liste:
            liste.append(f)
    G.handle = bpy.types.SpaceView3D.draw_handler_add(dessiner, (), "WINDOW", "POST_VIEW")
    bpy.types.VIEW3D_MT_mesh_add.append(menu_ajout)


def unregister():
    arreter()
    # Tout redevient comme avant : plus de modificateur, affichage d'origine
    for o in bpy.data.objects:
        if maillage.modificateur(o) is not None or "aryadne_affichage" in o:
            montrer_blender(o)
    g = bpy.data.node_groups.get(maillage.NOM_GROUPE)
    if g is not None and g.users == 0:
        bpy.data.node_groups.remove(g)
    if bpy.app.timers.is_registered(tic):
        bpy.app.timers.unregister(tic)
    if G.handle is not None:
        bpy.types.SpaceView3D.draw_handler_remove(G.handle, "WINDOW")
        G.handle = None
    for liste, f in HANDLERS:
        if f in liste:
            liste.remove(f)
    bpy.types.VIEW3D_MT_mesh_add.remove(menu_ajout)
    for c in reversed(CLASSES):
        bpy.utils.unregister_class(c)
    # Les cases cochées restent dans le fichier (propriétés), sans effet sans l'add-on
    del bpy.types.Object.aryadne_lourd
    del bpy.types.Object.aryadne_uid
