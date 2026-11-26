# Moteur d'affichage 3D

La vue 3D d'Aryadne est un **rasteriseur logiciel** : l'image est calculée par le processeur, sur tous les cœurs, puis montrée par egui comme une texture. La carte graphique ne sert pas à l'affichage de la vue. Elle sert au tracé de chemins et aux simulations.

Tout est dans `src/model3d.rs`. Le tracé de chemins est dans `src/pathtrace.rs` et `src/gputrace.rs`.

## L'idée : des pixols

Le nom vient de ZBrush. Un pixol est un pixel qui sait plus que sa couleur : il connaît sa profondeur et la surface qu'il montre.

Le rendu se fait en deux temps, bien séparés :

1. **Le tracé.** Pour chaque pixel : quel triangle voit-on, et où dessus ? Aucun éclairage.
2. **L'éclairage.** Pour chaque pixel : sa couleur, d'après le triangle trouvé, le matériau, les lumières et les ombres.

L'intérêt est le **cache**. Tant que la caméra ne bouge pas, le tracé reste valable. Changer une lumière, un matériau ou l'exposition refait seulement l'éclairage. Tant que les lumières ne bougent pas, les ombres restent valables aussi.

## Les données préparées

Un modèle chargé devient un `Prepared` : la forme prête à être dessinée.

| Champ | Contenu |
|---|---|
| `pos`, `nrm` | Positions et normales par sommet |
| `tris` | Les triangles (trois numéros de sommets) |
| `colors` | Couleur par triangle, en lumière linéaire |
| `edges` | Quelles arêtes dessiner en fil de fer (les diagonales des quads sont cachées) |
| `uv` | UV par coin de triangle |
| `mat` | ID de matériau par triangle |
| `floor` | Hauteur du sol |
| `curv` | Courbure par sommet, calculée à la demande (nœud `Curvature`) |
| `bvh` | Arbre de rayons, construit au premier besoin |
| `pix`, `vshadow`, `fshadow` | Les trois caches (voir plus bas) |

Les quads et les n-gones du fichier sont triangulés pour l'affichage. Le mode édition, lui, garde les polygones (`meshedit::EMesh`).

### Modèles lourds

`MAX_TRIS = 2 000 000`. Au-delà, le modèle est simplifié à l'ouverture par **regroupement de sommets** : une grille 3D est posée sur le modèle, et tous les sommets d'une même case fusionnent. La grille est resserrée jusqu'à passer sous la cible. La forme reste complète, avec moins de triangles.

`MAX_SUBDIV_TRIS = 2 500 000` plafonne la subdivision.

## Passe 1 : le buffer de visibilité

Fonction `raster`. Résultat : une structure `Pixols`.

| Champ de `Pixols` | Contenu, par pixel |
|---|---|
| `depth` | Profondeur. Plus grand = plus près (1/profondeur en perspective). `f32::MIN` : le fond |
| `tri` | Numéro du triangle visible |
| `bary` | Deux poids barycentriques : où est le pixel dans le triangle |
| `screen` | (par sommet) Position projetée à l'écran et profondeur |
| `shadows` | Cache d'ombres par pixel (voir plus bas) |

### Comment il est rempli

1. Tous les sommets sont projetés à l'écran, en parallèle (`rayon`).
2. Un tableau d'entiers atomiques de 64 bits, un par pixel, sert de buffer de visibilité. Les **32 bits hauts** portent la profondeur, rendue comparable comme un entier. Les **32 bits bas** portent le numéro du triangle.
3. Chaque triangle écrit directement dans ce tableau par un **maximum atomique** : la valeur la plus grande gagne, donc le triangle le plus proche. Aucun verrou, aucun tri préalable des triangles par zone d'image. C'est la méthode du rasteriseur logiciel de Nanite.
4. Les poids barycentriques de chaque pixel sont recalculés depuis son triangle gagnant.

La conversion de la profondeur en entier ordonnable : pour un nombre flottant positif, on met le bit de signe à 1. Pour un négatif, on inverse tous les bits. L'ordre des entiers obtenus est celui des flottants.

### Petits triangles : des points

Un triangle plus petit qu'un pixel n'est pas rasterisé. Il est posé comme **un point** en son centre, sans tests d'arêtes. Sur un maillage très dense, c'est presque tous les triangles. Aucun ne passe entre les centres de pixels. C'est l'idée de QSplat, reprise ici pour retirer le coût des micro-triangles.

### Perspective juste

En perspective, ce qui varie linéairement à l'écran est 1/profondeur, pas la profondeur. `persp_bary` convertit les poids à l'écran en poids dans l'espace, pour que textures et normales ne glissent pas.

### Bandes

`PIX_BAND = 16` : l'image est traitée par bandes de 16 lignes, chacune par un même cœur. Cela sert à la passe d'éclairage et au calcul des poids.

### Le cache du tracé

`pixols()` rend le tracé gardé dans `Prepared::pix` si la caméra et la taille de l'image n'ont pas changé. Sinon il relance `raster`.

## Passe 2 : l'éclairage

Pour chaque pixel qui montre un triangle :

1. **La normale.** Interpolée entre les trois sommets (ombrage lisse), ou celle de la face. Une face vue de dos a sa normale retournée, pour éviter les trous noirs.
2. **Le matériau.** Un des matériaux de la vue (argile, gris, blanc, chrome, normales, couleurs du fichier), ou un matériau en nœuds évalué en ce point. Le résultat : couleur, rugosité, métal, émission, et les réglages avancés (transmission, vernis, bandes toon, reflets, occlusion, velours, sous-surface).
3. **La peinture.** Si l'objet est peint, sa texture est lue aux UV de peinture et mélangée. Les masques peints sont transmis au matériau.
4. **L'environnement.** L'HDRI est lu dans la direction de la normale (lumière diffuse) et du reflet (lumière spéculaire). Sa rotation est calculée une fois, pas à chaque pixel.
5. **Les lumières.** Celles de la scène. S'il n'y en a pas, la lumière de la vue.
6. **Les ombres.** Voir ci-dessous.
7. **Le fil de fer** éventuel, par-dessus.

Pour un pixel de fond : l'environnement en fond s'il est demandé, et l'ombre au sol.

L'image est en lumière linéaire jusqu'au bout. L'exposition et la conversion pour l'écran viennent en dernier.

## Les ombres et leurs trois caches

Une ombre se calcule en lançant un rayon du point vers la lumière, à travers le `Bvh` du maillage (`src/bvh.rs` : arbre de boîtes englobantes construit par coupe médiane, parcouru sans allocation). Si le rayon touche quelque chose, le point est à l'ombre.

### Ombres douces

Plusieurs rayons légèrement écartés donnent un bord d'ombre doux. L'écart est un angle pour un soleil. Pour une lumière ponctuelle, c'est la taille de la source vue du point.

Deux astuces :

- le motif d'écart **tourne d'un pixel à l'autre**. Les marches d'escalier deviennent un grain fin ;
- on lance d'abord **un rayon sur deux**, du bord du disque vers le centre. S'ils sont tous d'accord (plein jour ou pleine ombre, soit presque toute l'image), les autres ne diraient rien de plus et ne sont pas lancés.

### Cache 1 : par pixel

`Pixols::shadows`. La part de lumière reçue par pixel et par lumière, avec une clé qui décrit les lumières. Valeur `NaN` : pas encore calculée.

Valable tant que ni la caméra ni les lumières ne changent. Changer un matériau, l'exposition ou l'environnement ne relance aucun rayon.

### Cache 2 : par sommet

`Prepared::vshadow`. La part de lumière reçue à chaque sommet, par lumière. Elle ne dépend **pas de la caméra** : elle sert encore quand on tourne autour du modèle.

Elle est utilisée pour les triangles petits à l'écran (`VERTEX_SHADOW_PX = 6` pixels) : l'interpolation entre sommets ne se voit pas. Les valeurs sont calculées à la demande, et stockées dans des entiers atomiques (les bits d'un `f32`, `NaN` pour « à calculer »).

### Cache 3 : le sol

`Prepared::fshadow`. L'ombre posée sur le sol, dans une grille de `FLOOR_GRID = 1024` cases de côté, dans le plan du sol, autour du modèle. Elle ne dépend que des lumières et du maillage.

L'ombre en un point du sol est interpolée entre quatre cases. Hors de la grille : un rayon direct.

### Quand chaque cache tombe

| Changement | Tracé | Ombres par pixel | Ombres par sommet | Ombres du sol |
|---|---|---|---|---|
| Matériau, exposition, environnement | Gardé | Gardé | Gardé | Gardé |
| Lumière | Gardé | Refait | Refait | Refait |
| Caméra, taille de la vue | Refait | Refait | Gardé | Gardé |
| Maillage (édition, modificateur, pose) | Tout est refait : un nouveau `Prepared` | | | |

## Recalculer seulement quand il le faut

La vue garde l'image calculée comme texture. Elle n'est recalculée que si quelque chose a changé : caméra, éclairage, matériau, scène, image d'animation. Entre deux, l'interface affiche la même texture.

Pour une scène animée, une pose qui change sans rien d'autre suit un chemin plus court (commentaire dans le code : « rien d'autre que la pose ne doit avoir changé »).

## La sélection à l'écran

`pick` retrouve le triangle sous la souris. Au-delà de `PICK_TREE_MIN = 20 000` triangles, un arbre accélère la recherche.

## Mesures

Le module de test `bench_pixols` construit une sphère bosselée dense comme un sculpt et mesure le tracé.

Le document de contexte des rapports de recherche (`CONTEXTE.md`) donne des chiffres d'un **état antérieur** du moteur : environ 29 ms sans ombres et 316 ms avec ombres en 1280 × 720 pour 109 000 triangles, avec un plafond de 150 000 triangles. Le code actuel a déjà franchi deux étapes du plan : le buffer de visibilité avec ses caches, et les petits triangles en points, avec un plafond relevé à 2 millions. **Les temps actuels n'ont pas été mesurés pour cette page.**

## Le tracé de chemins

Moteur « réaliste », dans l'esprit de Cycles.

### Sur le processeur (`pathtrace.rs`)

Chaque passe ajoute un échantillon par pixel dans une `Accum` :

| Champ | Contenu |
|---|---|
| `sum`, `cover` | Somme des échantillons (lumière linéaire), couverture (0 : fond transparent) |
| `nrm`, `alb` | Normale et couleur de surface du premier impact. Elles guident le débruitage |
| `spp` | Nombre d'échantillons accumulés |

Il gère : rebonds, ombres douces, reflets brillants ou mats, métaux, émission, transmission, ciel HDRI, lumières de la scène, sol infini, profondeur de champ, contours. Les matériaux en nœuds sont lus comme dans la vue. `Settings` porte les réglages (voir [Rendu](../utilisateur/3d-rendu.md#réglages)).

### Sur la carte graphique (`gputrace.rs`)

Le même rendu, par un shader de calcul wgpu. Le BVH, les normales et les matériaux partent sur la carte une fois. Ensuite, chaque passe ajoute un échantillon par pixel, relu dans une `Accum` pour l'affichage et le débruitage.

**Limite importante.** Les matériaux sont **évalués aux coins de chaque triangle**, puis interpolés. Un motif plus fin que les triangles est adouci.

### Débruitage (`oidn.rs`)

Intel Open Image Denoise est téléchargé une fois dans le dossier de l'utilisateur et chargé à l'exécution (`libloading`). Sans lui, un débruitage maison prend le relais, guidé par `nrm` et `alb`.

## Ce qui manque

- **Pas d'affichage sur carte graphique.** C'est l'étape suivante du plan : buffer de visibilité sur la carte, amas de triangles avec niveaux de détail.
- **Pas de niveaux de détail.** Un modèle lointain coûte autant de triangles qu'un modèle proche. La simplification se fait une fois, à l'ouverture.
- **Reflets approchés** dans l'aperçu : lus dans l'environnement, pas dans les autres objets.
- **Pas de transparence triée** dans l'aperçu : le buffer de visibilité ne garde qu'un triangle par pixel.
- **`model3d.rs` mêle** le rendu, la scène, les modes et l'interface dans un seul fichier.

La suite prévue est résumée dans la [feuille de route du moteur](feuille-de-route-moteur.md).
