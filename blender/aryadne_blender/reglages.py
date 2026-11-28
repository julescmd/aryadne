# Réglages d'ombrage de la vue (space_data.shading) traduits pour le moteur, et couleurs des objets
# selon le type de couleur choisi, pour ressembler au mode Solid de Blender.
import colorsys
import math
import zlib

import bpy
from mathutils import Matrix, Vector


def theme_vue():
    return bpy.context.preferences.themes[0].view_3d


def studio(nom, type_):
    for sl in bpy.context.preferences.studio_lights:
        if sl.type == type_ and sl.name == nom:
            return sl
    return None


def eclairage(shading, view_matrix):
    """Dictionnaire « eclairage » du protocole pour une vue."""
    e = {
        "culling": bool(getattr(shading, "show_backface_culling", False)),
        "speculaire": bool(getattr(shading, "show_specular_highlight", True)),
    }
    if shading.type == "WIREFRAME":
        e["mode"] = "fil"
        return e
    light = shading.light if shading.type == "SOLID" else "STUDIO"
    if light == "FLAT":
        e["mode"] = "plat"
    elif light == "MATCAP":
        sl = studio(shading.studio_light, "MATCAP")
        e["mode"] = "matcap"
        if sl is not None and sl.path:
            e["matcap"] = sl.path
        e["matcap_miroir"] = bool(getattr(shading, "use_matcap_flip", False))
    else:
        e["mode"] = "studio"
        sl = studio(shading.studio_light, "STUDIO")
        if sl is None:
            sl = studio("Default", "STUDIO")
        if sl is not None:
            lampes = []
            monde = bool(getattr(shading, "use_world_space_lighting", False))
            rot = Matrix.Rotation(getattr(shading, "studiolight_rotate_z", 0.0), 3, "Z")
            vr = view_matrix.to_3x3().normalized()
            for l in sl.solid_lights:
                if not l.use:
                    continue
                d = Vector(l.direction)
                if monde:
                    # Lumières fixées au monde, tournées autour de Z, puis vues depuis la caméra
                    d = vr @ (rot @ d)
                lampes.append({
                    "direction": list(d),
                    "diffuse": list(l.diffuse_color),
                    "speculaire": list(l.specular_color),
                    "douceur": float(l.smooth),
                })
            e["lampes"] = lampes
            e["ambiante"] = list(sl.light_ambient)
    return e


def couleur_aleatoire(nom):
    """Couleur « au hasard » stable pour un nom d'objet (teinte variée, douce)."""
    h = zlib.crc32(nom.encode("utf-8")) & 0xFFFFFFFF
    r, g, b = colorsys.hsv_to_rgb((h % 997) / 997.0, 0.5 + 0.25 * ((h >> 10) % 100) / 100.0, 0.85)
    # Valeurs d'affichage (sRGB) ramenées en linéaire
    return [srgb_vers_lin(r), srgb_vers_lin(g), srgb_vers_lin(b), 1.0]


def srgb_vers_lin(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def couleurs(obj, shading):
    """Liste de matériaux [r, g, b, a, rugosité, métal] (linéaires) pour le type de couleur de la vue."""
    ct = getattr(shading, "color_type", "MATERIAL") if shading.type == "SOLID" else "MATERIAL"
    if ct == "OBJECT":
        return [list(obj.color[:4]) + [0.5, 0.0]]
    if ct == "SINGLE":
        return [list(shading.single_color[:3]) + [1.0, 0.5, 0.0]]
    if ct == "RANDOM":
        return [couleur_aleatoire(obj.name) + [0.5, 0.0]]
    # MATERIAL, et repli pour TEXTURE, ATTRIBUTE et VERTEX : couleur d'affichage des matériaux
    out = []
    for slot in obj.material_slots:
        m = slot.material
        if m is None:
            out.append([0.8, 0.8, 0.8, 1.0, 0.5, 0.0])
        else:
            out.append(list(m.diffuse_color[:4]) + [float(m.roughness), float(m.metallic)])
    return out or [[0.8, 0.8, 0.8, 1.0, 0.5, 0.0]]


def couleur_fil(obj, actif, mode_fil):
    """Couleur des arêtes (sRGB) : noire, orange si choisi, jaune si actif, comme le thème."""
    t = theme_vue()
    if obj.select_get():
        c = t.object_active if obj == actif else t.object_selected
    else:
        c = t.wire
    return [c[0], c[1], c[2], 1.0]


def xray(shading):
    """Opacité en rayons X (None : opaque)."""
    if shading.type == "WIREFRAME":
        return shading.xray_alpha_wireframe if shading.show_xray_wireframe else None
    if shading.type == "SOLID" and shading.show_xray:
        return shading.xray_alpha
    return None


def fil_overlay(space):
    """Overlay « Wireframe » de la vue : (actif, opacité)."""
    ov = space.overlay
    if not ov.show_overlays:
        return False, 1.0
    return bool(ov.show_wireframes), float(ov.wireframe_opacity)
