# Composition de l'image du moteur dans la vue 3D de Blender : un rectangle plein écran qui écrit
# la profondeur du moteur (gl_FragDepth) avec le test de profondeur de Blender, le contour de
# sélection tiré du tampon d'identifiants, et, quand la caméra a bougé depuis la dernière image,
# une grille qui reprojette cette image selon la nouvelle caméra (pas d'image qui « glisse »).
import gpu
import numpy as np
from gpu_extras.batch import batch_for_shader
from mathutils import Matrix

_shaders = {}
_grilles = {}
_quad = {}
_erreur = [None]

COMMUN = """
float etat_de(float id)
{
  int i = int(id + 0.5);
  ivec2 ts = textureSize(etats, 0);
  if (i <= 0 || i >= ts.x) {
    return 0.0;
  }
  return texelFetch(etats, ivec2(i, 0), 0).r;
}

/* Contour : un pixel voisin d'un autre objet, dont l'un des deux est choisi.
 * Rend l'état (0 rien, 1 choisi, 2 actif) et la profondeur du côté choisi. */
float contour(ivec2 p, out float dc)
{
  ivec2 ts = textureSize(ids, 0);
  float id0 = texelFetch(ids, p, 0).r;
  float d0 = texelFetch(profondeur, p, 0).r;
  float s0 = etat_de(id0);
  float best = 0.0;
  dc = d0;
  for (int dy = -rayon; dy <= rayon; dy++) {
    for (int dx = -rayon; dx <= rayon; dx++) {
      if (dx == 0 && dy == 0) {
        continue;
      }
      ivec2 q = clamp(p + ivec2(dx, dy), ivec2(0), ts - 1);
      float id1 = texelFetch(ids, q, 0).r;
      if (abs(id1 - id0) > 0.5) {
        float s1 = etat_de(id1);
        float s = max(s0, s1);
        if (s > best) {
          best = s;
          dc = (s1 >= s0) ? texelFetch(profondeur, q, 0).r : d0;
        }
      }
    }
  }
  return best;
}

/* Les couleurs du moteur et du thème sont en sRGB ; le framebuffer de la vue 3D de Blender
 * attend du linéaire (il encode lui-même en sRGB), un framebuffer hors écran non. */
vec4 sortie(vec4 c)
{
  if (lineaire != 0) {
    vec3 lo = c.rgb / 12.92;
    vec3 hi = pow((c.rgb + 0.055) / 1.055, vec3(2.4));
    c.rgb = mix(hi, lo, vec3(lessThanEqual(c.rgb, vec3(0.04045))));
  }
  return c;
}
"""

QUAD_VS = """
void main()
{
  uv = pos * 0.5 + 0.5;
  gl_Position = vec4(pos, 0.0, 1.0);
}
"""

QUAD_FS = """
void main()
{
  ivec2 ts = textureSize(profondeur, 0);
  ivec2 p = clamp(ivec2(uv * vec2(ts)), ivec2(0), ts - 1);
  float d = texelFetch(profondeur, p, 0).r;
  float dc;
  float s = (rayon > 0) ? contour(p, dc) : 0.0;
  if (s > 0.5) {
    vec4 oc = s > 1.5 ? actcol : selcol;
    fragColor = sortie(vec4(oc.rgb, oc.a * (passe == 1 ? 0.5 : 1.0)));
    gl_FragDepth = min(d, dc);
    return;
  }
  if (passe == 1) {
    discard;
  }
  vec4 c = texelFetch(couleur, p, 0);
  if (d >= 1.0 || c.a <= 0.0) {
    discard;
  }
  fragColor = sortie(vec4(c.rgb, c.a * alpha));
  gl_FragDepth = d;
}
"""

GRILLE_VS = """
float prof(ivec2 p, ivec2 ts)
{
  return texelFetch(profondeur, clamp(p, ivec2(0), ts - 1), 0).r;
}

vec4 reprojeter(vec2 u, float d)
{
  return reproj * vec4(u * 2.0 - 1.0, d * 2.0 - 1.0, 1.0);
}

void main()
{
  ivec2 ts = textureSize(profondeur, 0);
  ivec2 p = ivec2(pos * vec2(ts));
  uv = pos;
  /* Un pixel de fond voisin de l'objet prend la profondeur de son voisin le plus proche : le bord
   * (et le contour de sélection) suit l'objet au lieu d'être rogné */
  float d = prof(p, ts);
  ivec2 os[4] = ivec2[4](ivec2(1, 0), ivec2(-1, 0), ivec2(0, 1), ivec2(0, -1));
  float dn[4];
  float dmin = 1.0;
  for (int k = 0; k < 4; k++) {
    dn[k] = prof(p + os[k], ts);
    dmin = min(dmin, dn[k]);
  }
  if (d >= 1.0) {
    d = dmin;
  }
  vec4 c = reprojeter(pos, d);
  float ok = d < 1.0 ? 1.0 : 0.0;
  vec2 texel = 1.0 / vec2(ts);
  for (int k = 0; k < 4; k++) {
    if (dn[k] >= 1.0 || ok == 0.0) {
      continue;
    }
    if (persp != 0) {
      vec4 cn = reprojeter(pos + vec2(os[k]) * texel, dn[k]);
      float r = max(cn.w, c.w) / max(min(cn.w, c.w), 1e-6);
      if (r > 1.03) {
        ok = 0.0;
      }
    }
    else if (abs(dn[k] - d) > 0.01) {
      ok = 0.0;
    }
  }
  valide = ok;
  gl_Position = c;
}
"""

GRILLE_FS = """
void main()
{
  if (valide < 0.999) {
    discard;
  }
  ivec2 ts = textureSize(profondeur, 0);
  ivec2 p = clamp(ivec2(uv * vec2(ts)), ivec2(0), ts - 1);
  float dc;
  float s = (rayon > 0) ? contour(p, dc) : 0.0;
  if (s > 0.5) {
    vec4 oc = s > 1.5 ? actcol : selcol;
    fragColor = sortie(vec4(oc.rgb, oc.a * (passe == 1 ? 0.5 : 1.0)));
    return;
  }
  if (passe == 1) {
    discard;
  }
  vec4 c = texture(couleur, uv);
  if (c.a <= 0.0) {
    discard;
  }
  fragColor = sortie(vec4(c.rgb, c.a * alpha));
}
"""


def _info(nom, grille):
    info = gpu.types.GPUShaderCreateInfo()
    iface = gpu.types.GPUStageInterfaceInfo("aryadne_" + nom)
    iface.smooth("VEC2", "uv")
    if grille:
        iface.smooth("FLOAT", "valide")
    info.vertex_in(0, "VEC2", "pos")
    info.vertex_out(iface)
    info.sampler(0, "FLOAT_2D", "couleur")
    info.sampler(1, "FLOAT_2D", "profondeur")
    info.sampler(2, "FLOAT_2D", "ids")
    info.sampler(3, "FLOAT_2D", "etats")
    if grille:
        info.push_constant("MAT4", "reproj")
    info.push_constant("VEC4", "selcol")
    info.push_constant("VEC4", "actcol")
    info.push_constant("FLOAT", "alpha")
    info.push_constant("INT", "rayon")
    info.push_constant("INT", "lineaire")
    info.push_constant("INT", "passe")
    if grille:
        info.push_constant("INT", "persp")
    info.fragment_out(0, "VEC4", "fragColor")
    if not grille and hasattr(info, "depth_write"):
        info.depth_write("ANY")
    info.vertex_source(GRILLE_VS if grille else QUAD_VS)
    info.fragment_source(COMMUN + (GRILLE_FS if grille else QUAD_FS))
    return info


def shader(grille):
    k = "grille" if grille else "quad"
    if k not in _shaders:
        _shaders[k] = gpu.shader.create_from_info(_info(k, grille))
    return _shaders[k]


def batch_quad():
    if "b" not in _quad:
        sh = shader(False)
        _quad["b"] = batch_for_shader(sh, "TRI_FAN", {"pos": ((-1, -1), (1, -1), (1, 1), (-1, 1))})
    return _quad["b"]


def batch_grille(w, h):
    """Une maille par pixel de l'image (sommets aux centres des pixels). Gardée par taille."""
    k = (w, h)
    if k not in _grilles:
        if len(_grilles) > 3:
            _grilles.clear()
        xs = (np.arange(w, dtype=np.float32) + 0.5) / w
        ys = (np.arange(h, dtype=np.float32) + 0.5) / h
        gx, gy = np.meshgrid(xs, ys)
        pos = np.stack([gx.ravel(), gy.ravel()], axis=1).astype(np.float32)
        i = np.arange(w * h, dtype=np.int32).reshape(h, w)
        a, b, c, d = i[:-1, :-1].ravel(), i[:-1, 1:].ravel(), i[1:, 1:].ravel(), i[1:, :-1].ravel()
        idx = np.concatenate([np.stack([a, b, c], 1), np.stack([a, c, d], 1)]).astype(np.int32)
        _grilles[k] = batch_for_shader(shader(True), "TRIS", {"pos": pos}, indices=idx)
    return _grilles[k]


class Textures:
    """Textures d'une image du moteur (créées dans le contexte de dessin)."""

    def __init__(self, rgba, prof, ids):
        h, w = prof.shape
        self.w, self.h = w, h
        c = gpu.types.Buffer("FLOAT", w * h * 4, (rgba.astype(np.float32) * (1.0 / 255.0)).ravel())
        self.couleur = gpu.types.GPUTexture((w, h), format="RGBA8", data=c)
        self.profondeur = gpu.types.GPUTexture((w, h), format="R32F", data=gpu.types.Buffer("FLOAT", w * h, prof.ravel()))
        self.ids = gpu.types.GPUTexture((w, h), format="R32F", data=gpu.types.Buffer("FLOAT", w * h, ids.astype(np.float32).ravel()))


def texture_etats(etats):
    """États de sélection par numéro d'objet (0 rien, 1 choisi, 2 actif)."""
    n = max(2, len(etats))
    a = np.zeros(n, dtype=np.float32)
    a[: len(etats)] = etats
    return gpu.types.GPUTexture((n, 1), format="R32F", data=gpu.types.Buffer("FLOAT", n, a))


def composer(tex, etats, img_vp, vp, persp, selcol, actcol, alpha=None, rayon=1, lineaire=True):
    """Dessine l'image `tex` (rendue avec la matrice vue-projection `img_vp`) dans la vue
    courante (matrice `vp`). `alpha` : opacité en rayons X (None : opaque, avec profondeur).
    Deux passes : l'image avec la profondeur, puis le contour de sélection en transparence par-dessus
    tout (comme Blender, qui montre le contour d'un objet choisi même caché)."""
    meme = all(abs(img_vp[i][j] - vp[i][j]) < 1e-6 * (1.0 + abs(vp[i][j])) for i in range(4) for j in range(4))
    grille = not meme
    sh = shader(grille)
    te = texture_etats(etats)
    batch = batch_grille(tex.w, tex.h) if grille else batch_quad()
    for passe in (0, 1) if rayon > 0 else (0,):
        if alpha is None and passe == 0:
            gpu.state.depth_test_set("LESS_EQUAL")
            gpu.state.depth_mask_set(True)
        else:
            gpu.state.depth_test_set("NONE")
            gpu.state.depth_mask_set(False)
        gpu.state.blend_set("ALPHA")
        sh.bind()
        sh.uniform_sampler("couleur", tex.couleur)
        sh.uniform_sampler("profondeur", tex.profondeur)
        sh.uniform_sampler("ids", tex.ids)
        sh.uniform_sampler("etats", te)
        sh.uniform_float("selcol", selcol)
        sh.uniform_float("actcol", actcol)
        sh.uniform_float("alpha", 1.0 if alpha is None else float(alpha))
        sh.uniform_int("rayon", int(rayon))
        sh.uniform_int("lineaire", 1 if lineaire else 0)
        sh.uniform_int("passe", passe)
        if grille:
            try:
                inv = img_vp.inverted()
            except ValueError:
                inv = Matrix.Identity(4)
            sh.uniform_float("reproj", vp @ inv)
            sh.uniform_int("persp", 1 if persp else 0)
        batch.draw(sh)
    gpu.state.blend_set("NONE")
    gpu.state.depth_mask_set(False)
    gpu.state.depth_test_set("NONE")
    return grille


def origines(points, couleurs, taille):
    """Points d'origine des objets lourds choisis, par-dessus l'image (comme ceux de Blender)."""
    if not points:
        return
    try:
        sh = gpu.shader.from_builtin("POINT_UNIFORM_COLOR")
    except Exception:
        sh = gpu.shader.from_builtin("UNIFORM_COLOR")
    gpu.state.depth_test_set("NONE")
    gpu.state.blend_set("ALPHA")
    for p, c in zip(points, couleurs):
        b = batch_for_shader(sh, "POINTS", {"pos": [p]})
        sh.bind()
        gpu.state.point_size_set(taille + 2.0)
        sh.uniform_float("color", (0.0, 0.0, 0.0, 0.9))
        b.draw(sh)
        gpu.state.point_size_set(taille)
        sh.uniform_float("color", c)
        b.draw(sh)
    gpu.state.point_size_set(1.0)
    gpu.state.blend_set("NONE")
