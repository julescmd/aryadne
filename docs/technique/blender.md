# Aryadne dans Blender : l'affichage lourd

Le but : montrer dans la vue 3D de Blender des maillages de plusieurs millions de triangles, vite, sans que l'utilisateur voie autre chose que Blender. Tout le reste reste à Blender : outliner, modificateurs, annuler, sauvegarde, gizmos, outils, overlays.

Deux morceaux :

- **Le moteur** : l'exécutable Aryadne lancé en mode serveur, `aryadne --moteur`, sans interface (`src/moteur.rs`). Il garde les maillages et rend des images avec le rasteriseur « pixols » de `src/model3d.rs`.
- **L'add-on** : une extension Blender en Python (`blender/aryadne_blender/`), qui parle avec le moteur.

Les deux sont des programmes séparés. Ils ne partagent que le protocole décrit plus bas.

## Le chemin d'une image

1. On coche « Affichage lourd (Aryadne) » sur un objet maillage.
2. L'add-on lit le maillage évalué (avec ses modificateurs) par `foreach_get`, directement dans un fichier temporaire projeté en mémoire. Les triangles viennent de `loop_triangles`. Le moteur lit ce fichier et le garde. C'est fait une seule fois, puis seulement quand la géométrie change.
3. Blender ne dessine plus l'objet (voir « Masquer l'objet » plus bas).
4. À chaque dessin de la vue, un draw handler `POST_VIEW` envoie la matrice de vue, la matrice de projection et la taille de la région. Il ne bloque jamais : il montre la dernière image reçue et redemande.
5. Le moteur rend l'image et l'écrit dans un fichier d'images à deux cases. Un timer de l'add-on (`bpy.app.timers`) voit la réponse, copie l'image et redessine la vue.
6. L'image est posée sur la vue par un shader qui écrit la profondeur du moteur (`gl_FragDepth`). Le test de profondeur se fait contre celle de Blender : les objets de Blender et ceux du moteur se cachent correctement.

Pendant que la vue bouge, l'image demandée est réduite (moitié par défaut). Elle redevient pleine 0,2 s après l'arrêt.

## Pas d'image qui glisse : la reprojection

Si la caméra a bougé depuis la dernière image, on ne pose pas cette image telle quelle. On la reprojette : une grille d'un sommet par pixel, chaque sommet remis dans le monde avec sa profondeur et l'ancienne caméra, puis projeté avec la nouvelle. Les mailles qui enjambent un bord (saut de profondeur, fond) sont jetées. Le fond voisin de l'objet prend la profondeur de l'objet, pour que le bord et le contour suivent.

Mesuré dans l'essai (rotation de 3,4°) : écart moyen avec la vraie nouvelle image de 0,5 niveau sur 255 avec la reprojection, 1,7 sans.

## Masquer l'objet dans Blender

Méthode par défaut : un modificateur Geometry Nodes « Aryadne (affichage lourd) », ajouté en dernier, visible seulement dans la vue (pas au rendu, pas en mode édition). Il remplace la géométrie par les 8 coins de sa boîte, en points de rayon nul. Blender ne dessine donc rien, mais garde les bonnes bornes (« . » du pavé numérique cadre bien l'objet), l'origine et les gizmos.

Le modificateur est retiré juste avant chaque enregistrement et remis après : le fichier `.blend` n'en garde aucune trace. Désactiver l'add-on le retire aussi. La case cochée reste dans le fichier, sans effet sans l'add-on.

Méthode « Boîte » (préférences) : `display_type = 'BOUNDS'`. Blender dessine alors la boîte de l'objet, mais le maillage évalué reste complet pour les autres objets qui le visent.

## Ce qui est pareil que Blender

- **Ombrage Solid** lu à chaque dessin dans `space_data.shading` : Studio (les lumières du studio choisi, lues dans les préférences, y compris « éclairage dans le monde » et sa rotation), MatCap (le fichier EXR du matcap, passes diffuse et spéculaire, avec le miroir), Flat.
- **Couleurs** : Material (couleur d'affichage `diffuse_color`, rugosité et métal de chaque emplacement, par triangle), Object, Single. Texture, Attribute et Vertex prennent la couleur des matériaux.
- **Faces arrière** (backface culling), **rayons X** avec leur opacité, **reflets spéculaires** on/off.
- **Overlay Wireframe** : arêtes réelles du maillage (pas les diagonales de triangulation), couleurs du thème (noir, orange si choisi, jaune si actif), avec l'opacité de l'overlay.
- **Sélection** : les clics passent par Blender (ses raccourcis, gauche ou droite). Après coup, si le clic tombait sur un objet lourd, l'add-on le choisit (clic, Maj+clic). Si un objet de Blender est devant, c'est lui qui gagne. La sélection au cadre marche aussi.
- **Contour** orange (jaune pour l'actif), aux couleurs du thème, tiré du tampon d'identifiants du moteur. Il se voit en transparence à travers les objets qui le cachent, comme dans Blender.
- **Origine** des objets lourds choisis, redessinée par-dessus.
- **Mode édition, sculpture, peinture** : l'affichage lourd se met en pause pour cet objet et Blender le dessine lui-même. Au retour en mode Objet, la géométrie est renvoyée.
- **Annuler** : après un annuler ou un refaire, tout est revérifié. Une géométrie identique (même empreinte) n'est pas renvoyée.

## Ce qui ne l'est pas encore

- **Cavity** (cavité) : pas encore rendue.
- **Couleur Random** : la teinte de chaque objet vient d'un hachage de son nom, pas du même que Blender. Les couleurs diffèrent.
- **Seuil du fil de fer** (`wireframe_threshold`) : toutes les arêtes réelles sont dessinées. En overlay Wireframe, les 8 points de la boîte de l'objet se voient.
- **Rayons X** : seule la surface la plus proche est rendue, posée en transparence. Blender superpose toutes les surfaces.
- **Material Preview et Rendered** (Eevee, Cycles) : l'objet est montré avec l'ombrage Solid Studio du moteur, pas avec ses matériaux. Le rendu final (F12) n'est pas concerné : il voit l'objet complet.
- **Une seule source d'éclairage** : le studio de la vue. Pas d'ombres portées, pas les lampes de la scène.
- **Objet déplacé** : la matrice suit sans renvoyer la géométrie, mais l'image arrive une image plus tard (environ 0,1 s sur 5 M triangles). La reprojection ne couvre que la caméra.
- **Autres objets qui visent l'objet lourd** (Shrinkwrap, Booléen, accrochage à la surface) : avec la méthode par modificateur, ils voient la boîte en points. Prendre la méthode « Boîte » pour ces objets.
- **Sélection au lasso et au cercle** : pas encore prises en charge pour les objets lourds.
- **Couleurs** : la vue 3D de Blender attend des couleurs linéaires (elle encode elle-même en sRGB). L'add-on convertit. Si l'objet paraît délavé ou trop sombre, décocher « Couleurs linéaires vers la vue » dans les préférences.

## Le protocole

Version 1. Une commande JSON par ligne sur l'entrée standard du moteur, une réponse JSON par ligne sur sa sortie standard. Les messages de diagnostic vont sur la sortie d'erreur. Le moteur s'arrête sur `quitter` ou quand son entrée se ferme (Blender parti).

Chaque commande peut porter `"n"` (un numéro) : la réponse le reprend. Chaque réponse a `"ok"` (vrai ou faux), `"cmd"`, `"ms"`, et `"erreur"` en cas d'échec.

Les matrices sont 16 nombres rangés par lignes (`m[ligne][colonne]`, vecteurs colonnes, comme mathutils). Les nombres binaires sont petit-boutistes.

| Commande | Champs | Réponse |
|---|---|---|
| `bonjour` | | `moteur`, `version`, `protocole` |
| `objet` | `id`, `fichier`, `sommets`, `triangles`, `matrice`, `num`, `couleur` ou `couleurs`, `fil_couleur`, `decalage_triangles`, `decalage_materiaux`, `decalage_aretes` | `id`, `num`, `triangles`, `lecture_ms` |
| `modifier` | `id`, et au choix `matrice`, `couleur`, `couleurs`, `fil_couleur`, `visible` | `id` |
| `retirer` | `id` | `id` |
| `vider` | | |
| `vue` | `vue`, `projection`, `largeur`, `hauteur`, `perspective` (sinon déduit de la projection) | |
| `eclairage` | `mode` (`studio`, `matcap`, `plat`, `fil`), `lampes`, `ambiante`, `speculaire`, `matcap`, `matcap_miroir`, `culling`, `fil`, `fil_opacite` | |
| `image` | rien, ou les champs de `vue`, et `eclairage` (objet) | voir plus bas |
| `scene` | | `objets`, `triangles`, `sommets` |
| `quitter` | | |

**Fichier d'un maillage** (écrit par le client) : sommets en `f32 × 3` à partir de 0, puis triangles en `u32 × 3` (à `12 × sommets`, ou `decalage_triangles`). En option : matériau de chaque triangle en `u32` à `decalage_materiaux`, arêtes réelles de chaque triangle en `u8` à `decalage_aretes` (bit 0 : v0-v1, bit 1 : v1-v2, bit 2 : v2-v0). Le moteur copie tout ; le client peut effacer le fichier dès la réponse.

`couleurs` : une liste de matériaux `[r, g, b, a, rugosité, métal]`, couleurs linéaires. `lampes` : `{direction, diffuse, speculaire, douceur}`, direction vers la lumière dans le repère de la vue (x à droite, y en haut, z vers l'œil).

**Fichier d'images** (créé par le moteur, dans le dossier temporaire) :

| Position | Contenu |
|---|---|
| 0 | `ARYI` |
| 4 | version du protocole (`u32`) |
| 8 | pixels par case (`u32`) |
| 16 + 16 k | case k : numéro de la dernière image écrite (`u64`), largeur, hauteur (`u32`) |
| 64 + k × cap × 12 | case k : RGBA8 (cap × 4 octets), profondeur `f32` (cap × 4), numéro d'objet `u32` (cap × 4) |

Réponse à `image` : `image` (numéro), `case`, `largeur`, `hauteur`, `fichier`, `decalage_rgba`, `decalage_profondeur`, `decalage_ids`, `triangles`, `trace_ms`, `rendu_ms`.

L'image n va dans la case n % 2. Le numéro de la case est écrit en dernier : une case dont le numéro est celui annoncé est complète. Le client copie la case dès la réponse, et ne demande qu'une image à la fois : le moteur écrit donc toujours dans la case que le client ne lit pas. Si la fenêtre grandit au-delà de la capacité, le moteur crée un nouveau fichier et l'annonce dans `fichier`.

**L'image** : lignes de bas en haut, comme OpenGL. Couleurs sRGB. Fond transparent. **Profondeur** : la profondeur de fenêtre de Blender (OpenGL), 0 au plan proche, 1 au plan lointain et pour le fond. **Identifiants** : le `num` de l'objet visible, 0 pour le fond.

## La projection par matrices

`moteur::projeter` prend la matrice vue-projection de Blender telle quelle. Un sommet donne x et y en pixels (origine en bas à gauche), le z NDC (pour la profondeur), et une clé pour le buffer de visibilité : `-w` en perspective (le buffer compare `1/w`, linéaire à l'écran), `-z NDC` en orthographique. Un sommet derrière la caméra ou devant le plan proche écarte ses triangles.

Le buffer de visibilité atomique (`model3d::visibility_pass`) est le même que celui de la visionneuse d'Aryadne. Plusieurs objets y écrivent l'un après l'autre, chacun avec son premier numéro de triangle. La visionneuse continue d'utiliser sa caméra yaw/pitch/dist.

Le mode serveur ne simplifie rien : la limite de 2 M triangles de l'import (`MAX_TRIS`) ne s'applique pas.

## Mesures

Machine de test : 2 cœurs, sans carte graphique (rendu logiciel llvmpipe pour Blender), moteur en release, image 1280 × 720.

| Triangles | Envoi du maillage (total) | dont lecture Blender | dont lecture moteur | Image (rendu) | dont tracé | Aller-retour |
|---|---|---|---|---|---|---|
| 1 M | 333 ms | 258 ms | 29 ms | 62 ms | 39 ms | 65 ms |
| 5 M | 2,1 s | 1,6 s | 154 ms | 93 ms | 62 ms | 97 ms |
| 10 M | 4,0 s | 3,0 s | 331 ms | 177 ms | 132 ms | 180 ms |

L'envoi n'a lieu qu'à l'activation et quand la géométrie change. Son temps est surtout la lecture côté Blender (`foreach_get` et le calcul des arêtes réelles des quadrilatères). L'image se refait à chaque mouvement de la vue, en taille réduite pendant le mouvement.

## Essai sans interface

`blender/tests/essai_sans_interface.py` tourne avec le module `bpy` (roue Python de Blender). Après un premier rendu Workbench, le module `gpu` marche en arrière-plan (EGL) : l'essai rend la vue 3D de Blender hors écran (`GPUOffScreen.draw_view3d`), y compose l'image du moteur avec le vrai shader de l'add-on, et écrit des PNG côte à côte, Blender seul à gauche, Blender et moteur à droite. Il vérifie aussi le clic, l'enregistrement, le mode édition et la désactivation.

```
uv venv -p 3.11 venv && VIRTUAL_ENV=venv uv pip install bpy==5.0.1 pillow
python blender/tests/essai_sans_interface.py --moteur target/release/aryadne --zip aryadne_blender-0.1.0.zip --sortie sortie --millions 1,5,10
```

Le zip se construit avec `blender --command extension build` dans `blender/aryadne_blender`, ou par la CI (simple zip du dossier).
