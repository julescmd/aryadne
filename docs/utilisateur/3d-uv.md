# UV et UDIM

Le mode `UV & materials` réunit quatre onglets : `Materials`, `Seams`, `UV`, `Retopo`. Cette page couvre les trois premiers. Pour le quatrième, voir [Retopologie](3d-retopologie.md).

## Entrer

1. Dans une scène, choisis l'objet.
2. Mode `UV & materials` dans la barre.
3. `◀ Back to the scene` (ou `Échap`) pour revenir.

**Où vont les données.** IDs de matériau, coutures et UV sont gardés à côté du modèle, dans `.app/meshdata/` du coffre. Le fichier d'origine ne change pas. L'export (OBJ, FBX, GLB) les écrit.

La ligne d'état sous les onglets indique le nombre de faces, de faces choisies et de coutures.

## Onglet Materials : les IDs de matériau

Un ID de matériau est une étiquette posée sur des faces. Chaque ID peut recevoir son propre matériau.

### Choisir des faces

| Geste | Effet |
|---|---|
| Clic | Choisir une face |
| `Maj+clic` | Ajouter |
| `Ctrl+clic` | Retirer |
| `All`, `None`, `Invert` | Tout, rien, inverser |
| `Grow` | Ajoute les faces voisines d'orientation proche |
| `Island` | Les faces reliées, jusqu'aux coutures |

### Gérer les IDs

Chaque ID a une couleur et un nom.

| Bouton | Effet |
|---|---|
| `Assign` | Donne cet ID aux faces choisies |
| `Choose` | Sélectionne les faces de cet ID |
| `＋ Nodes` | Crée un matériau en nœuds pour cet ID et l'ouvre |
| `Nodes ✎` | Ouvre le matériau en nœuds de cet ID |
| Retirer | Retire le matériau en nœuds |

Les IDs sont exportés en OBJ (`usemtl` + fichier `.mtl`) et en GLB (un matériau par ID).

## Onglet Seams : les coutures

Une couture est une arête où la surface sera coupée pour se déplier à plat.

### À la main

| Geste | Effet |
|---|---|
| Clic sur une arête | La choisir |
| `Maj+clic` | Ajouter |
| `Alt+clic` | Toute la boucle |
| `Ctrl+clic` | Le chemin le plus court depuis la dernière arête |
| Clic droit, ou `Ctrl+E` | Menu : `Mark seam`, retirer, `Seam around selection`, `Seams from islands` |

### Automatiquement

| Bouton | Ce qu'il fait |
|---|---|
| `Automatic seams` | Coupe minimale pour que chaque îlot se déplie à plat. Un cylindre reçoit une seule couture droite. Les coutures vont dans les plis et les creux. L'outil découpe là où ça s'étire trop, puis fusionne les îlots voisins |
| `Smart seams` | Regroupe les faces en îlots presque plats (comme Smart UV Project de Blender), en partant des grandes surfaces, puis place les coutures sur leurs bords. Réglages : `Angle`, `Min. island size` |
| `On sharp edges` | Une couture sur chaque arête plus vive que `Sharp angle`. `Add the sharp ones` les ajoute aux coutures déjà posées |
| `Selection outline` | Une couture tout autour des faces choisies |
| `Seams from islands` | Recrée les coutures d'après les UV existantes |

`Unwrap with these seams →` déplie et passe à l'onglet UV.

### Réglages de l'automatique

| Réglage | Effet |
|---|---|
| Curseur `Fewer islands` / `Less stretching` | Peu d'îlots, quitte à étirer, ou beaucoup d'îlots sans étirement |
| `Always cut beyond (°)` | Une arête plus vive que cet angle est toujours coupée |
| `Separate materials` | Chaque ID de matériau a ses propres îlots |
| `Hide seams (hollows, underside)` | Préfère placer les coutures là où on ne les voit pas |

## Onglet UV

### Déplier en un clic

| Bouton | Ce qu'il fait |
|---|---|
| `Auto (seams + UVs)` | Coutures automatiques avancées, dépliage, îlots redressés et rangés |
| `Unwrap my seams` | Déplie avec tes coutures |
| `Pack` | Range les îlots : même densité, rotation, rangement serré |
| `Checker` | Affiche un damier sur le modèle pour juger l'étirement |

Sans UV, un message le rappelle : « Unwrap » pose des coutures automatiques si besoin, déplie (méthode LSCM) et range les îlots.

### Rangement

| Réglage | Effet |
|---|---|
| `Optimized packing` | Essaie plusieurs ordres, rotations et largeurs, et garde le plus serré |
| `Rotation` | Autorise la rotation des îlots |
| `Texture` | Taille de texture visée, en pixels |
| `Margin` | Espace entre îlots, en pixels |

La ligne `Islands` / `Fill` indique le nombre d'îlots et la part du carré UV occupée.

### Densité de texels

La densité est le nombre de pixels de texture par unité de surface (px/u).

- `Density:` affiche la densité moyenne actuelle.
- `Apply density` met tous les îlots à la valeur choisie. Ils peuvent déborder du carré si elle est trop forte.
- `= Current` reprend la densité moyenne comme valeur.

### L'éditeur UV

L'éditeur UV est à gauche de la vue 3D, ou dans son propre panneau (`UV editor in this panel`).

| Geste | Effet |
|---|---|
| Clic | Choisir un sommet ou un îlot |
| Glisser | Rectangle de sélection, ou déplacer |
| `G`, `R`, `S` | Déplacer, tourner, mettre à l'échelle. `X` ou `Y` impose un axe |
| `A`, `Alt+A` | Tout, rien |
| `Ctrl+I` | Inverser |
| `L` | L'îlot sous le pointeur |
| `Ctrl+L` | L'îlot des faces choisies |
| `P`, `Alt+P` | Épingler, désépingler |
| `U` | Déplier la sélection, en gardant les épingles |
| `V` | Souder (même sommet) |
| `Origine` | Cadrer |
| Clic droit | Menu |

Menu du clic droit : `Auto seams + unwrap`, `Smart seams (projection)`, `Align horizontally` (et verticalement), `Straighten as grid (quads)`, `Weld (same vertex)`, `Average island scale`, miroir, rotation, `Seams from islands`.

`Live unwrap` (dépliage en direct) : déplacer des sommets épinglés redéplie leur îlot autour d'eux.

## UDIM

Les UDIM répartissent les îlots sur plusieurs carrés UV (les tuiles 1001, 1002…). Chaque tuile a ses propres images à la cuisson : plus de détail par îlot.

1. Choisis le nombre de tuiles (1 à 20).
2. `Distribute`.

Avec 1, tout revient dans le carré de base.

## Cuisson des textures (bake)

`Bake textures` écrit, dans le dossier `textures/` du coffre : couleur, rugosité, métal, normale et courbure. Le calcul part des UV et du matériau de chaque ID.

| Réglage | Effet |
|---|---|
| `High-poly` | Chemin d'un sculpt, facultatif. Ses détails sont reportés dans une normale en espace tangent. Il doit occuper la même place que le modèle ouvert |
| Occlusion | `No occlusion`, ou un nombre de rayons : `8 (draft)` à `128 (smooth)` |
| Taille | La taille des images, en pixels |
| `Re-bake when the model changes` | Les textures sont recuites toutes seules, en arrière-plan, dès que la forme, les UV ou les matériaux changent |

## Limites

- Le dépliage utilise la méthode LSCM. Il n'y a pas de second algorithme au choix.
- Les UV sont liées au nombre de sommets. Un modificateur qui change la topologie (décimer, remaillage) ou une retopologie les invalide.
- Les UDIM sont gérées au rangement et à la cuisson. **Incertain :** l'affichage dans la vue 3D et la peinture sur plusieurs tuiles n'ont pas été vérifiés.
