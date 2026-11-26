# Sculpture

Des pinceaux qui déforment la surface, comme on modèle de l'argile.

## Entrer en sculpture

1. Dans une scène, choisis un objet à maillage.
2. Mode `Sculpt` dans la barre.
3. `Leave sculpting` pour sortir.

La sculpture travaille sur le même maillage que le [mode édition](3d-modelisation.md#mode-édition) : un OBJ à côté de la scène. Le modèle d'origine n'est pas touché.

## Préparer le maillage

Un pinceau ne peut déplacer que les sommets qui existent. Sur un cube à 8 sommets, il ne se passe presque rien. Trois façons d'avoir assez de sommets :

- le modificateur **Remesh (voxels)**, puis `Apply` : une topologie égale partout, en quads réguliers, sans trous ni faces qui se croisent ;
- les **niveaux de multirésolution** (voir plus bas) ;
- le **remaillage dynamique**, qui ajoute des sommets sous le pinceau.

## Les pinceaux

| Pinceau | Touche | Effet |
|---|---|---|
| `Draw` | `X` | Pousse la surface vers l'extérieur |
| `Clay` | `C` | Ajoute de la matière à plat, comme des bandes d'argile |
| `Inflate` | `I` | Gonfle le long des normales |
| `Smooth (Shift)` | `S` | Adoucit |
| `Grab` | `G` | Attrape et tire un morceau |
| `Flatten` | `T` | Aplatit vers un plan moyen |
| `Pinch` | `P` | Resserre les sommets vers le centre du pinceau |

Maintenir `Maj` pendant un trait passe au lissage, quel que soit le pinceau choisi.

## Réglages

| Réglage | Effet |
|---|---|
| `Radius ([ and ])` | Taille du pinceau à l'écran. `[` réduit, `]` agrandit |
| `Strength` | Force du pinceau |
| `X symmetry` | Chaque trait est répété de l'autre côté de l'axe X |

## Naviguer pendant qu'on sculpte

Le clic gauche sculpte. Pour tourner autour du modèle : molette enfoncée, ou `Alt` + glisser. `Maj` + molette enfoncée déplace la vue.

## Multirésolution

La multirésolution garde plusieurs niveaux de détail du même maillage. On monte pour les détails fins, on descend pour les grandes formes, sans rien perdre.

| Bouton | Effet |
|---|---|
| Niveau moins détaillé | Les grandes formes. Le détail fin est gardé |
| `Multiresolution level` | Le niveau actuel. 0 est la forme de base |
| Niveau plus détaillé | Remonte d'un niveau |
| `Add a level` | Subdivise tout le maillage (Catmull-Clark), pour plus de détails |

**Exemple.** Sculpte un visage au niveau 3. Le nez est trop court. Descends au niveau 0, tire le nez avec `Grab`, remonte au niveau 3 : les pores et les rides sont toujours là, posés sur le nouveau nez.

**Limite.** Aryadne refuse d'ajouter un niveau au-delà de 1,5 million de faces : la vue et la mémoire ne suivraient plus.

## Remaillage dynamique

Bouton **Dynamic topology** : la surface se subdivise sous le pinceau pendant que tu sculptes.

- `Detail size` : la taille du détail, en part du rayon du pinceau. Plus petit : plus fin.
- Le remaillage dynamique **remplace** les niveaux de multirésolution. Les deux ne se combinent pas.

Utile pour ébaucher une forme sans se soucier de la topologie. Le maillage obtenu est irrégulier : passe ensuite par la [retopologie](3d-retopologie.md).

## Annuler

`Ctrl+Z` annule le dernier trait. `Ctrl+Maj+Z` rétablit.

## Un flux de travail type

1. `Add › Sphere`.
2. Modificateur `Remesh (voxels)`, taille de voxel petite, `Apply`.
3. Mode `Sculpt`. `Grab` pour les grandes masses, `Clay` pour construire, `Smooth` pour nettoyer.
4. `Add a level` une ou deux fois pour les détails.
5. [Retopologie](3d-retopologie.md) pour un maillage propre.
6. [UV](3d-uv.md), puis cuisson : les détails du sculpt passent dans une carte de normales.

## Limites

- Sept pinceaux. Pas de masques, pas de pinceaux à texture (alphas), pas de couches de sculpt.
- La symétrie n'existe que sur X.
- Le pinceau trouve la surface par le sommet visible le plus proche du curseur. Sur un maillage très grossier, il peut sembler « sauter ».
- Les performances dépendent du processeur : la sculpture n'utilise pas la carte graphique.
