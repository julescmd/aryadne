# Compositing en nœuds

Fichier : `.arycomp`. Étape **2D**, outil `Compositing`.

Un compositing retouche une image avec des nœuds : couleurs, flous, masques, incrustation. Le même genre de graphe sert à **étalonner un clip** du montage.

Pour les gestes de l'éditeur, voir [L'éditeur de nœuds](noeuds-editeur.md).

## Créer

`File › New › Node compositing`. Le graphe part avec un nœud de sortie `Output`.

## Le flux de base

1. Ajoute un nœud `Image` et choisis un fichier (ou glisse une image dans le graphe).
2. Ajoute des nœuds de couleur ou de filtre.
3. Relie le dernier à `Output`.

L'aperçu montre le résultat en direct. L'export écrit un PNG en pleine résolution.

## Les entrées d'image

| Nœud | Rôle |
|---|---|
| `Image` | Une image du coffre. Sorties : image, alpha |
| `Multilayer EXR` | Un EXR à plusieurs passes. Réglage `Layer` : le nom de la passe à lire |
| `Video texture` | Une image d'une vidéo ou d'un montage, à un `Time (s)` donné |
| `Image texture` | Une image répétée, avec une `Scale` |

Les rendus 3D enregistrés en `Multilayer EXR (passes)` se relisent ici, passe par passe. Voir [Rendu](3d-rendu.md#rendu-final).

## Filtres

| Nœud | Réglages | Effet |
|---|---|---|
| `Blur` | `Radius` (0 à 100) | Flou |
| `Directional blur` | `Angle`, `Length (px)` | Flou dans une direction |
| `Radial blur (zoom)` | `Strength` | Flou qui part du centre |
| `Masked blur (tilt-shift)` | `Radius`, entrée `Mask` | Flou dosé par un masque |
| `Depth of field (Z pass)` | `Focus distance`, `Sharp range`, `Falloff`, `Max radius (px)`, entrée `Depth` | Profondeur de champ à partir d'une passe de profondeur |
| `Sharpen` | `Strength` | Netteté |
| `Glow` | `Threshold`, `Radius`, `Strength` | Halo autour des zones claires |
| `Sun beams` | `Threshold`, `Length`, `Strength`, `Center x`, `Center y` | Rayons de lumière |
| `Vignette` | `Strength` | Assombrit les coins |
| `Grain` | `Strength` | Grain de film |
| `Grain match (measured on the plate)` | `Grain size`, `Amount`, entrée `Plate` | Mesure le grain d'une image filmée et le pose sur une autre. Sortie en plus : `Measured strength` |
| `Chromatic aberration` | `Offset (px)` | Décale le rouge et le bleu |
| `Lens distortion` | `Distortion` | Barillet ou coussinet |
| `Pixelate` | `Size (px)` | Gros pixels |
| `Posterize` | `Levels` | Réduit le nombre de tons |
| `Halftone` | `Size (px)`, `Angle` | Trame d'impression |
| `Scanlines` | `Strength`, `Count` | Lignes de balayage |
| `Letterbox` | `Aspect` | Bandes noires de cinéma |
| `Emboss` | `Strength`, `Angle` | Gaufrage |
| `Glitch` | `Strength`, `Blocks`, `Seed` | Décalages par blocs |
| `Edges` | | Détecte les contours |
| `Transform` | `x`, `y`, `Rotation`, `Scale` | Déplace, tourne, redimensionne |
| `Flip` | `Direction` : horizontal, vertical, les deux | Retourne |
| `Displace by image` | `Strength (px)`, entrée `Map` | Déforme l'image selon une autre |

## Couleur

Tous les nœuds de couleur des matériaux marchent ici : `Mix`, `Color ramp`, `RGB curves`, `Levels`, `Hue / saturation`, `White balance`, `Color balance`, `Tone mapping`… Voir [Matériaux en nœuds](noeuds-materiaux.md#color-couleur).

## Incrustation et masques

| Nœud | Entrées et réglages | Effet |
|---|---|---|
| `Chroma key (green screen)` | `Key color`, `Tolerance`, `Softness` | Retire un fond vert ou bleu. Sorties : image, masque |
| `Luma key` | `Low`, `High`, `Keep` : clair ou sombre | Garde les zones claires ou sombres |
| `Difference key` | `Clean plate`, `Tolerance`, `Softness` | Compare avec la même image sans le sujet, et garde ce qui diffère |
| `Despill` | `Strength`, `Color` : vert ou bleu | Retire le reflet vert ou bleu sur le sujet |
| `Roto mask (path)` | `Softness`, `Mode` : intérieur ou extérieur | Un masque tracé point par point |
| `Shape mask` | Centre, taille, `Softness`, forme | Un masque en ellipse, rectangle ou losange |
| `Set alpha` | Entrée `Alpha` | Donne un alpha à une image |
| `Alpha over` | `Below`, `Above`, `Opacity` | Pose une image sur une autre |
| `Unmultiply (remove black)` | | Change le noir en transparent (flammes, étincelles sur fond noir) |

### Exemple : un personnage sur fond vert

1. `Image` (le personnage) vers `Chroma key`. Règle `Key color` sur le vert du fond.
2. `Despill` sur la sortie image, pour retirer le reflet vert sur les cheveux.
3. `Image` (le décor) en `Below` d'un `Alpha over`, le personnage en `Above`.
4. `Grain match` : la plaque est le décor, pour que le personnage ait le même grain.
5. `Output`.

## Motifs, math et code

Les motifs (`Noise`, `Gradient`, `Checker`…), les nœuds de math et le `Custom shader` sont disponibles. Dans un compositing, les coordonnées `x` et `y` vont de 0 à 1 sur l'image, et `w`, `h` donnent sa taille.

## Étalonner un clip du montage

Dans le montage, sur un clip : `＋ Node grading` (ou `Grade in nodes`). Aryadne crée un `.arycomp` relié au clip. Le graphe s'applique à ses couleurs, à l'aperçu comme à l'export. `Nodes ✎` le rouvre. Voir [Montage](montage.md#étalonnage).

## Animation

Le nœud `Time` donne l'image courante. Un compositing peut donc varier dans le temps : branche `Seconds` sur le `Seed` d'un grain, ou sur le décalage d'un motif.

## Export

Le bouton d'export écrit un PNG à la résolution de l'image d'entrée.

## Limites

- Le calcul se fait sur le processeur. Les flous de grand rayon sur une image 4K prennent du temps.
- L'export est en PNG. Pour un EXR en sortie, ce n'est pas prévu par le code actuel.
- Le masque roto se trace point par point. Il n'est pas animable par clés dans le compositing. Pour un masque suivi, voir le [montage](montage.md#suivi-et-stabilisation).
- Les nœuds d'usure et de géométrie (`Edge wear`, `Curvature`…) n'ont pas de sens ici : ils n'apparaissent pas dans le menu.
