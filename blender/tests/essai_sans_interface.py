# Essai de l'add-on sans interface, avec le module bpy (roue Python de Blender) :
#
#   python essai_sans_interface.py --moteur /chemin/aryadne --zip aryadne_blender-x.zip --sortie dossier [--millions 1,5,10]
#
# Installe l'extension, crée une scène (une sphère lourde et un cube de Blender qui la traverse),
# active l'affichage lourd, demande des images au moteur, puis compose dans un rendu hors écran de
# la vue 3D de Blender (le module gpu marche en arrière-plan après un premier rendu Workbench).
# Écrit des PNG côte à côte (Blender seul / Blender + moteur) et mesure les temps.
import argparse
import json
import os
import sys
import time

import bpy
import gpu
import numpy as np
from mathutils import Matrix, Vector
from PIL import Image

ap = argparse.ArgumentParser()
ap.add_argument("--moteur", required=True)
ap.add_argument("--zip", required=True)
ap.add_argument("--sortie", required=True)
ap.add_argument("--millions", default="1")
ap.add_argument("--taille", default="960x540")
args = ap.parse_args()
os.makedirs(args.sortie, exist_ok=True)
W, H = map(int, args.taille.split("x"))
os.environ["ARYADNE_MOTEUR"] = args.moteur
resultats = {}


def log(*a):
    print("[essai]", *a, flush=True)


# Le module gpu n'est prêt qu'après un premier rendu (contexte EGL créé par Blender)
sc = bpy.context.scene
sc.render.engine = "BLENDER_WORKBENCH"
sc.render.resolution_x = sc.render.resolution_y = 16
bpy.ops.render.render()

# ---------- installation de l'extension, comme depuis le .zip ----------
bpy.ops.extensions.package_install_files(filepath=args.zip, repo="user_default", enable_on_install=True)
import addon_utils  # noqa: E402

nom = "bl_ext.user_default.aryadne_blender"
addon_utils.enable(nom, default_set=True)
ab = sys.modules[nom]
log("extension installée :", nom, "moteur :", ab.chemin_exe())

# ---------- scène ----------
cube = bpy.data.objects["Cube"]
cube.location = (0.95, -0.2, 0.3)
cube.scale = (0.45, 0.45, 0.45)
scr = bpy.data.screens["Layout"]
area = [a for a in scr.areas if a.type == "VIEW_3D"][0]
space = area.spaces.active
region = [r for r in area.regions if r.type == "WINDOW"][0]
cam = sc.camera
cam.location = (4.2, -4.6, 2.9)
dg = bpy.context.evaluated_depsgraph_get()
dg.update()


def matrices(cam_obj):
    vm = cam_obj.matrix_world.inverted()
    pm = cam_obj.calc_matrix_camera(bpy.context.evaluated_depsgraph_get(), x=W, y=H)
    return vm, pm


def vue_blender(vm, pm):
    """Rendu hors écran de la vue 3D de Blender (avec overlays) ; rend l'objet offscreen lié."""
    off = gpu.types.GPUOffScreen(W, H)
    off.draw_view3d(sc, bpy.context.view_layer, space, region, vm, pm, do_color_management=True)
    return off


def lire(off):
    with off.bind():
        fb = gpu.state.active_framebuffer_get()
        c = np.array(fb.read_color(0, 0, W, H, 4, 0, "UBYTE").to_list(), dtype=np.uint8).reshape(H, W, 4)
    return c[::-1].copy()


def png(a, nom):
    p = os.path.join(args.sortie, nom)
    Image.fromarray(a).save(p)
    log("écrit", p)
    return p


def cote_a_cote(a, b, nom, titres=None):
    sep = np.full((a.shape[0], 6, 4), 255, dtype=np.uint8)
    png(np.concatenate([a, sep, b], axis=1), nom)


class FausseRegion:
    def __init__(self, w, h):
        self.width, self.height = w, h

    def as_pointer(self):
        return 12345


class FausseRv3d:
    def __init__(self, vm, pm):
        self.view_matrix, self.window_matrix = vm, pm
        self.is_perspective = True
        self.perspective_matrix = pm @ vm


def image_moteur(vm, pm, plein=True):
    """Demande une image comme le fait le dessin de la vue, et attend la réponse."""
    from importlib import import_module

    # Une vue qui change « bouge » : image réduite ; la même vue un instant plus tard : image pleine
    t = time.time()
    re = ab.preparer_vue(FausseRegion(W, H), FausseRv3d(vm, pm), space, now=t)
    if plein:
        re = ab.preparer_vue(FausseRegion(W, H), FausseRv3d(vm, pm), space, now=t + 10)
    t0 = time.perf_counter()
    ab.G.en_vol = None
    re.image_cle = None  # toujours redemander, pour mesurer
    ab.pomper()
    n = ab.G.en_vol[0]
    while ab.G.en_vol is not None:
        ab.recevoir()
        time.sleep(0.0005)
    aller_retour = (time.perf_counter() - t0) * 1000
    return re, aller_retour


def composer(off, re, vp, vm=None, pm=None):
    with off.bind():
        # Comme dans la vue (POST_VIEW) : les matrices de la vue sont chargées
        if vm is not None:
            gpu.matrix.load_matrix(vm)
            gpu.matrix.load_projection_matrix(pm)
        # Le framebuffer hors écran est en octets sRGB tels quels : pas de conversion
        ab.composer_region(re, vp, space, lineaire=False)


# ---------- objets lourds ----------
space.shading.type = "SOLID"
space.shading.light = "STUDIO"
space.shading.color_type = "MATERIAL"
space.overlay.show_overlays = True
millions = [float(x) for x in args.millions.split(",")]
ab.prefs().demarrage_auto = True
mesures = []
for k, mi in enumerate(millions):
    for o in [o for o in bpy.data.objects if o.name.startswith("Sphère lourde")]:
        bpy.data.objects.remove(o)
    t0 = time.perf_counter()
    from importlib import import_module

    st = import_module(nom + ".scene_test")
    sph = st.creer_objet("Sphère lourde", int(mi * 1e6))
    t_creation = time.perf_counter() - t0
    for x in bpy.context.view_layer.objects:
        x.select_set(False)
    sph.select_set(True)
    bpy.context.view_layer.objects.active = sph
    sph.aryadne_lourd = True
    assert ab.G.actif(), ab.G.erreur
    t0 = time.perf_counter()
    ab.synchroniser()
    envoi = (time.perf_counter() - t0) * 1000
    stats = dict(ab.G.stats)
    vm, pm = matrices(cam)
    re, ar_plein = image_moteur(vm, pm)
    # Deuxième image identique de taille : mesure stable
    ab.G.version += 1
    re, ar_plein = image_moteur(vm, pm)
    img_ms = ab.G.stats.get("image_ms")
    trace_ms = ab.G.stats.get("trace_ms")
    m = {"millions": mi, "triangles": stats.get("triangles"), "creation_blender_s": round(t_creation, 2), "envoi_ms": round(envoi, 1), "lecture_blender_ms": stats.get("blender_ms"), "moteur_lecture_ms": stats.get("moteur_ms"), "image_ms": img_ms, "trace_ms": trace_ms, "aller_retour_ms": round(ar_plein, 1), "taille": "%dx%d" % (W, H)}
    mesures.append(m)
    log("mesure", json.dumps(m))
resultats["mesures"] = mesures

# ---------- comparaison avec Blender ----------
vm, pm = matrices(cam)
vp = pm @ vm


def comparer(nom_png, regler=None):
    if regler:
        regler()
    re, _ = image_moteur(vm, pm)
    off = vue_blender(vm, pm)
    composer(off, re, vp, vm, pm)
    a = lire(off)
    # Blender seul : on lui rend l'objet le temps d'une image
    ab.montrer_blender(sph)
    bpy.context.evaluated_depsgraph_get().update()
    b = lire(vue_blender(vm, pm))
    ab.masquer_blender(sph)
    bpy.context.evaluated_depsgraph_get().update()
    cote_a_cote(b, a, nom_png)
    # Écart moyen sur les pixels de l'objet
    diff = np.abs(a[..., :3].astype(int) - b[..., :3].astype(int)).mean()
    return round(float(diff), 2), a, b


resultats["ecarts"] = {}
d, a, b = comparer("comparaison-studio.png")
resultats["ecarts"]["studio"] = d


def matcap():
    space.shading.light = "MATCAP"
    space.shading.studio_light = "clay_brown.exr"


d, *_ = comparer("comparaison-matcap.png", matcap)
resultats["ecarts"]["matcap"] = d


def plat_aleatoire():
    space.shading.light = "FLAT"
    space.shading.color_type = "RANDOM"


d, *_ = comparer("comparaison-plat-hasard.png", plat_aleatoire)
resultats["ecarts"]["plat_hasard"] = d


def fil():
    space.shading.light = "STUDIO"
    space.shading.color_type = "MATERIAL"
    space.overlay.show_wireframes = True


d, *_ = comparer("comparaison-fil.png", fil)
resultats["ecarts"]["fil"] = d
space.overlay.show_wireframes = False

# ---------- profondeur : le cube de Blender traverse la sphère ----------
re, _ = image_moteur(vm, pm)
_, prof, ids = re.image
resultats["profondeur"] = {"min": float(prof[prof < 1].min()), "max": float(prof[prof < 1].max()), "pixels_objet": int((ids > 0).sum())}

# ---------- reprojection : la caméra tourne, l'image du moteur est encore l'ancienne ----------
rot = Matrix.Rotation(0.06, 4, "Z")
cam2 = cam.copy()
sc.collection.objects.link(cam2)
cam2.matrix_world = rot @ cam.matrix_world
vm2, pm2 = matrices(cam2)
off = vue_blender(vm2, pm2)
composer(off, re, pm2 @ vm2, vm2, pm2)
reproj = lire(off)
off_old = vue_blender(vm2, pm2)
# Sans reprojection, l'ancienne image posée telle quelle (ce qu'on voyait « glisser »)
re_meme = ab.Region()
re_meme.__dict__.update(re.__dict__)
re_meme.image_vp = pm2 @ vm2
composer(off_old, re_meme, pm2 @ vm2, vm2, pm2)
glisse = lire(off_old)
re2, _ = image_moteur(vm2, pm2)
off_new = vue_blender(vm2, pm2)
composer(off_new, re2, pm2 @ vm2, vm2, pm2)
juste = lire(off_new)
cote_a_cote(glisse, reproj, "reprojection-sans-avec.png")
cote_a_cote(reproj, juste, "reprojection-avec-vs-nouvelle.png")
resultats["reprojection"] = {
    "ecart_sans": round(float(np.abs(glisse[..., :3].astype(int) - juste[..., :3].astype(int)).mean()), 2),
    "ecart_avec": round(float(np.abs(reproj[..., :3].astype(int) - juste[..., :3].astype(int)).mean()), 2),
}
bpy.data.objects.remove(cam2)

# ---------- sélection au clic ----------
re, _ = image_moteur(vm, pm)
_, prof, ids = re.image
ys, xs = np.nonzero(ids)
# Un pixel de la moitié gauche de la sphère (le cube de Blender cache une partie de la droite)
k = np.argmin(xs + 0 * ys)
cy, cx = int(ys[k]), int(xs[k]) + 25
for x in bpy.context.view_layer.objects:
    x.select_set(False)
o = ab.objet_sous(cx, cy, re)
resultats["clic_sur_sphere"] = o.name if o else None
# Un pixel du cube (devant la sphère) : la sélection de Blender doit l'emporter
cube_px = None
bp = lire(vue_blender(vm, pm))
for (yy, xx) in zip(*np.nonzero(ids)):
    pass
co = vp @ Vector(tuple(cube.matrix_world.translation) + (1.0,))
co = co.xyz / co.w
px, py = int((co.x * 0.5 + 0.5) * W), int((co.y * 0.5 + 0.5) * H)
o2 = ab.objet_sous(px, py, re)
resultats["clic_sur_cube_devant"] = o2.name if o2 else None
ab.choisir(sph)
re, _ = image_moteur(vm, pm)
off = vue_blender(vm, pm)
composer(off, re, vp, vm, pm)
a = lire(off)
ab.montrer_blender(sph)
bpy.context.evaluated_depsgraph_get().update()
b = lire(vue_blender(vm, pm))
ab.masquer_blender(sph)
cote_a_cote(b, a, "comparaison-selection.png")

# ---------- enregistrement : le fichier ne garde pas le modificateur ----------
fichier = os.path.join(args.sortie, "essai.blend")
bpy.ops.wm.save_as_mainfile(filepath=fichier, copy=True)
resultats["modificateur_apres_enregistrement"] = ab.maillage.modificateur(sph) is not None
import subprocess

verif = subprocess.run([sys.executable, "-I", "-c", "import bpy; bpy.ops.wm.open_mainfile(filepath=%r); o=bpy.data.objects['Sphère lourde']; print('MODS', [m.name for m in o.modifiers], o.display_type)" % fichier], capture_output=True, text=True)
resultats["fichier_enregistre"] = [l for l in verif.stdout.splitlines() if l.startswith("MODS")]

# ---------- mode édition : Blender reprend la main ----------
bpy.context.view_layer.objects.active = sph
with bpy.context.temp_override(active_object=sph, object=sph):
    try:
        bpy.ops.object.mode_set(mode="EDIT")
        ab.synchroniser()
        m = ab.maillage.modificateur(sph)
        resultats["edition_modificateur_actif"] = m.show_viewport if m else None
        bpy.ops.object.mode_set(mode="OBJECT")
        ab.synchroniser()
        resultats["objet_modificateur_actif"] = ab.maillage.modificateur(sph).show_viewport
    except Exception as e:
        resultats["edition"] = "non vérifié : %s" % e

# ---------- désactivation : tout redevient comme avant ----------
addon_utils.disable(nom, default_set=True)
resultats["apres_desactivation"] = {"modificateurs": [m.name for m in sph.modifiers], "affichage": sph.display_type}
log("RESULTATS", json.dumps(resultats, ensure_ascii=False, indent=1))
with open(os.path.join(args.sortie, "resultats.json"), "w") as f:
    json.dump(resultats, f, ensure_ascii=False, indent=1)
