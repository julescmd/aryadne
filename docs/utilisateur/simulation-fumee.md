# Fumée et feu

La fumée et le feu se simulent dans l'outil **Simulation** de l'étape **3D**, avec des nœuds, comme le tissu. Le calcul se fait sur la carte graphique.

Pour apprendre pas à pas : leçon [Fumée et feu](../apprendre/14-simulation/09-fumee-et-feu.md). Pour les nœuds en général : [Géométrie et simulation](noeuds-geometrie.md).

## Trois exemples pour partir

Menu **Examples** de l'éditeur :

| Exemple | Ce qu'il montre |
|---|---|
| `rising smoke` | Une source, un solveur : la fumée chaude monte |
| `campfire` | Du carburant chaud, la combustion, des flammes |
| `smoke against an obstacle` | La fumée contourne une boule |

## Le volume

Une fumée n'est pas faite de points. C'est un **volume** : une boîte découpée en voxels. Il voyage dans les liens comme une géométrie, et peut être réuni à des points avec `Merge`.

Chaque voxel porte cinq champs : densité, température, carburant, flamme, vitesse.

## La chaîne type

`Empty volume` → début de zone → `Smoke source` → (`Combustion`) → `Smoke solver` → fin de zone → (`Smoke look`) → sortie.

Relie la sortie `Time step` du début de zone à l'entrée `Time step` de chaque nœud de la zone.

## Les nœuds

### `Empty volume`

| Entrée | Défaut | Sens |
|---|---|---|
| `Center` | (0, 1, 0) | Centre de la boîte |
| `Scale` | (2, 2, 2) | Ses trois tailles |
| `Resolution` | 64 | Voxels le long du plus grand côté |

Les nombres de voxels sont arrondis à des multiples de 8. La couche de voxels du bord sert de frontière.

### `Smoke source`

Pose de la fumée, de la chaleur ou du carburant dans une zone, à chaque image.

| Entrée | Défaut | Sens |
|---|---|---|
| `Volume` | | Le volume à remplir |
| `Geometry` | | Avec la forme `points of a geometry` : la fumée naît autour de ses points (et sur ses faces) |
| `Center`, `Radius` | (0, 0,2, 0), 0,15 | La sphère. `Radius` sert aussi autour des points |
| `Scale` | (0,3, 0,3, 0,3) | Les tailles de la boîte |
| `Density`, `Temperature`, `Fuel` | 1, 1, 0 | Ce qui est posé |
| `Speed` | (0, 0, 0) | Vitesse donnée à l'air dans la zone |
| `Soft edge` | 0,03 | Largeur du bord doux |
| `Irregularity` | 0,4 | Hasard de la source, par bouffées |

Réglages : la forme (`sphere`, `box`, `points of a geometry`) et le mode. `set` impose au moins ces valeurs. `add` les ajoute chaque seconde.

### `Combustion`

Le carburant plus chaud que `Ignition` brûle.

| Entrée | Défaut | Sens |
|---|---|---|
| `Ignition` | 0,3 | Température d'allumage |
| `Burn rate` | 2 | Carburant brûlé par seconde |
| `Heat` | 3 | Chaleur donnée par unité de carburant |
| `Smoke` | 0,3 | Fumée donnée par unité de carburant |
| `Expansion` | 0,5 | Gonflement des gaz |
| `Cooling` | 0,5 | Refroidissement par rayonnement : les flammes les plus chaudes refroidissent le plus vite |

### `Smoke solver`

| Entrée | Défaut | Sens |
|---|---|---|
| `Substeps` | 1 | Pas de calcul par image. À monter si la fumée va très vite |
| `Pressure iterations` | 4 | Justesse du mouvement. Chaque itération divise par cinq environ l'erreur qui reste |
| `Buoyancy` | 4 | Poussée de l'air chaud vers le haut |
| `Smoke weight` | 0,5 | Poids de la fumée |
| `Swirls` | 2 | Rend les petits tourbillons |
| `Dissipation` | 0,1 | Part de la fumée qui disparaît par seconde |
| `Cooling` | 0,4 | Chaleur perdue par seconde |
| `Wind` | (0, 0, 0) | Vent |
| `Ball center`, `Ball radius` | rayon 0 : pas de boule | Obstacle |
| `Box center`, `Box size` | taille 0 : pas de boîte | Obstacle |
| `Air drag` | 0 | Frein de tout le mouvement |

Réglages :

| Réglage | Choix |
|---|---|
| Advection | `MacCormack` : garde le détail. `simple` : plus rapide, plus flou |
| Bords | `open, closed floor` : l'air sort par les côtés et le haut, le sol est fermé. `all open`. `closed, open top` : une cheminée |
| Calcul | `graphics card` ou `CPU`. Sans carte utilisable, le processeur prend le relais tout seul |

Pour qu'un obstacle suive un objet d'une scène 3D, branche un nœud `Scene object` sur `Ball center` et `Ball radius`, ou sur `Box center` et `Box size`.

### `Smoke look`

Change l'aspect sans refaire la simulation.

| Entrée | Défaut | Sens |
|---|---|---|
| `Opacity` | 12 | Ce que la fumée cache, par unité de densité |
| `Color` | gris clair | Couleur de la fumée |
| `Flame glow` | 5 | Force de la lueur des flammes |
| `Shadow` | 1 | Ombre de la fumée sur elle-même |

## Ce que dit l'aperçu

En haut : la taille de la grille, en voxels.

En bas, après un pas : où la fumée se calcule (carte graphique ou processeur), le temps du dernier pas, et `divergence`. Ce nombre mesure l'air créé ou perdu par erreur, par seconde. Sous 0,01, le mouvement est juste à l'œil.

`no readback` (sans relecture) : l'image vient directement de la carte, rien n'est recopié vers le processeur.

## Choisir la résolution

| Résolution | Usage | Mémoire sur la carte |
|---|---|---|
| 64 | Chercher les réglages | environ 15 Mo |
| 128 | Aperçu fin | environ 120 Mo |
| 256 | Interactif sur une bonne carte | environ 950 Mo |
| 512 | Final | environ 7,5 Go |

Les chiffres valent pour une boîte cubique. Le banc de mesure donne les temps réels de ta carte : [Banc de mesure](banc-gpu.md).

## Dans une scène 3D

Pose le fichier `.arygeo` dans une scène : menu **Add**, **Node geometry**. La fumée apparaît dans la vue 3D, derrière les objets qui sont devant elle, et suit la timeline.

Elle sort dans le **rendu final** et dans **l'export vidéo**, image par image.

## Cache

Le cache marche comme pour le tissu : revenir en arrière ne recalcule rien, et la fin de zone peut être figée sur le disque. Voir [Géométrie et simulation](noeuds-geometrie.md).

Les champs sont rangés en demi-précision : 14 octets par voxel dans un fichier de cache.

## Limites

- La grille est dense : toute la boîte est calculée, même vide.
- Obstacles : une boule et une boîte par solveur. Pas encore de maillage quelconque.
- Un obstacle qui bouge arrête la fumée mais ne la pousse pas.
- Dans l'onglet **Simulation**, la fumée est dessinée par-dessus les points et les surfaces. Dans une scène 3D, la profondeur est respectée.
- Dans la vue 3D et les rendus, la fumée ne projette pas d'ombre sur les objets et ne les éclaire pas. Elle s'ombre elle-même.
- `Move, rotate` déplace les points, pas le volume. Pour placer une fumée, règle `Center` de `Empty volume`, ou déplace l'objet dans la scène.
- En 256 et plus, garder chaque image pour le cache coûte du temps : le banc le mesure.
