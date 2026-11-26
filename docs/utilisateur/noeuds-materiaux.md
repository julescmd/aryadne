# Matériaux en nœuds

> **En cours de refonte.** Le flux des matériaux change : l'[Atelier](3d-atelier.md) doit devenir l'entrée des matériaux, et ses calques s'ouvriront en nœuds. Cette page décrit le code actuel. La façon de créer et d'appliquer un matériau peut changer. La liste des nœuds reste valable.

Fichier : `.arymat`. Un matériau en nœuds décrit l'aspect d'une surface : sa couleur, sa rugosité, son caractère métallique, son relief.

Pour les gestes de l'éditeur (ajouter, relier, grouper), voir [L'éditeur de nœuds](noeuds-editeur.md).

## Créer et appliquer

| Depuis | Geste |
|---|---|
| La vue 3D | `Nodes ▾ › New material…` : crée le matériau, l'applique à l'objet et l'ouvre à côté du modèle |
| Les propriétés d'un objet | Onglet `Material` : choisir un `.arymat` du coffre, ou `Create a node material for this object` |
| Un ID de matériau | Mode `UV & materials`, onglet `Materials`, bouton `＋ Nodes` sur l'ID |
| Le menu Fichier | `File › New › Node material`, avec un préréglage ou `Empty (output only)` |

Un matériau appliqué se met à jour en direct dans la vue 3D pendant que tu modifies ses nœuds.

`Open this material in the node editor` rouvre le graphe depuis l'objet.

## L'aperçu

Dans l'éditeur : sur `Sphere`, sur `Cube` (arêtes adoucies, pour juger l'usure), ou sur un modèle 3D déposé dans l'aperçu. `Light ▾` : `Studio` ou `Outdoor`.

`Preview node` (clic droit sur un nœud) montre ce nœud seul. `Back to full material` revient au matériau complet.

## Le nœud de sortie

`Material output`. Tout ce qui y arrive définit la surface.

| Entrée | Sens |
|---|---|
| `Color` | Couleur de base |
| `Roughness` | 0 : miroir. 1 : mat |
| `Metal` | 0 : non métallique. 1 : métal |
| `Emission` | Couleur émise (la surface éclaire) |
| `Emission strength` | Force de l'émission |
| `Transmission` | 0 : opaque. 1 : transparent comme du verre |
| `Index of refraction` | Déviation de la lumière dans le verre. 1,45 par défaut |
| `Clearcoat` | Couche de vernis brillante par-dessus (peinture de voiture) |
| `Toon bands` | Nombre de bandes d'ombrage, pour un rendu dessin animé. 0 : désactivé |
| `Specular` | Force des reflets |
| `Occlusion` | Assombrit les creux |
| `Bump`, `Bump strength` | Relief simulé à partir d'une hauteur |
| `Velvet` | Reflet doux sur les bords, comme un tissu |
| `Subsurface`, `Subsurface tint` | La lumière entre dans la matière et ressort teintée (cire, peau) |

## Préréglages

Vingt et un matériaux prêts : carrelage en relief, béton fissuré, métal brossé rayé, velours, cire, tissu tissé, métal peint usé, pierre aux creux sombres, bois, pierre, métal rouillé, toon, céramique, or, verre, verre dépoli, eau, toon 3 bandes, peinture de voiture, plastique, néon.

Ouvre-les pour apprendre : chacun est un petit graphe lisible.

## Les nœuds

Les noms sont ceux de l'interface anglaise. Dans l'interface, la prise appelée « image » en français s'affiche `Frame` en anglais.

### Input (entrées)

| Nœud | Sorties | Rôle |
|---|---|---|
| `Color` | Couleur | Une couleur fixe |
| `Value` | Valeur | Un nombre fixe |
| `Exposed parameter` | Couleur, valeur | Un réglage visible depuis l'Atelier. Voir plus bas |
| `Coordinates` | `u`, `v`, `uv` | Les UV du modèle |
| `Texture coordinates` | `Object`, `uv` | Position sur l'objet, ou UV |
| `Mapping` | Coordonnées | Déplace, tourne et étire des coordonnées. Réglages : `x`, `y`, `z`, `Rotation X/Y/Z`, `Stretch X/Y/Z` |
| `Warp coordinates` | Coordonnées | Déforme des coordonnées par un bruit. Entrées : `Strength`, `Scale`, `Detail` |
| `Geometry` | `Normal`, `Location`, `Height`, `Slope` | Ce que la surface sait d'elle-même |
| `Curvature` | `Edges`, `Cavities`, `Raw` | Arêtes saillantes et creux du modèle |
| `Fresnel` | `Fresnel` | Fort sur les bords vus de biais, faible de face |
| `File color` | Couleur | Les couleurs enregistrées dans le fichier du modèle |
| `Image texture` | Couleur, alpha | Une image du coffre. `Projection` : triplanaire, dessus (XZ), face (XY), côté (YZ), sphérique |
| `Video texture` | Couleur, alpha | Une vidéo ou un montage (`.aryseq`). Réglage `Time (s)` |
| `Painted mask` | Masque | Un masque peint à la main sur l'objet. `Channel` : 1 à 4 |
| `Time` | `Frame`, `Seconds` | L'image courante |

La projection **triplanaire** pose la texture sans UV : trois projections mélangées selon l'orientation de la surface. C'est le choix par défaut. Les libellés des projections ne sont pas tous traduits en anglais.

### Pattern (motifs)

Tous prennent des `Coordinates` en entrée (facultatif) et une `Scale`.

| Nœud | Sorties | Réglages propres |
|---|---|---|
| `Noise` | Valeur, couleur | `Detail`, `Roughness` |
| `Ridged noise` | Valeur | `Detail`, `Sharpness`. Des crêtes, pour la roche |
| `White noise` | Valeur, couleur | Du hasard pur, sans lissage |
| `Voronoi` | `Distance`, `Cell` | Des cellules |
| `Checker` | Couleur, valeur | `Color A`, `Color B` |
| `Gradient` | Valeur | `Type` : horizontal, vertical, radial, hauteur (3D) |
| `Stripes` | Valeur | `Count`, `Thickness`, `Axis` |
| `Bricks` | Couleur, masque | `Mortar`, `Brick`, `Grout`, `Variation` |
| `Wood` | Couleur, valeur | `Rings`, `Distortion`, `Light`, `Dark` |
| `Marble` | Couleur, valeur | `Veins`, `Turbulence`, `Base`, `Vein` |
| `Hexagons` | `Edge`, `Cell` | `Edge` |
| `Dots` | Valeur | `Radius`, `Grid` : carrée ou décalée |
| `Waves` | Valeur | `Distortion`, `Type` : bandes ou anneaux |
| `Cracks` | `Cracks`, `Cells` | `Width`, `Distortion` |
| `Tiles` | `Hue`, `Grout`, `Height` | `Grout`, `Bevel`, `Variation` |
| `Weave` | `Height`, `Direction` | Un tissage |
| `Shape mask` | Masque | `Center x`, `Center y`, `Width`, `Height`, `Softness`, `Shape` : ellipse, rectangle, losange |

### Wear (usure)

Ces nœuds donnent un **masque** : blanc là où l'effet s'applique. Ils lisent la forme du modèle.

| Nœud | Où le masque est blanc | Entrées |
|---|---|---|
| `Edge wear` | Sur les arêtes saillantes | `Intensity`, `Amount`, `Contrast`, `Noise scale` |
| `Cavity dirt` | Dans les creux | `Intensity`, `Amount`, `Contrast`, `Noise scale` |
| `Dust (top)` | Sur les faces tournées vers le haut | `Amount`, `Contrast`, `Noise scale` |
| `Drips` | En coulures verticales | `Scale`, `Length`, `Amount` |
| `Scratches` | En rayures fines | `Scale`, `Amount`, `Fineness` |
| `Grunge` | En taches irrégulières | `Scale`, `Contrast`, `Distortion` |

### Color (couleur)

| Nœud | Rôle |
|---|---|
| `Mix` | Mélange A et B selon un `Factor`. Modes : mélange, multiplier, écran, incrustation, ajouter, soustraire, différence, éclaircir, assombrir, diviser |
| `Color ramp` | Change une valeur en couleur par un dégradé à plusieurs couleurs. Double-clic sur le dégradé ajoute une couleur. `Interpolation` : linéaire, lisse, constante |
| `3-input ramp` | Dégradé simple à trois couleurs, avec un `Middle` |
| `RGB curves` | Courbes tracées à la souris : ensemble, rouge, vert, bleu |
| `Curve (S contrast)` | `Shadows`, `Highlights`, `S contrast` |
| `Hue / saturation` | `Hue`, `Saturation`, `Value` |
| `Brightness / contrast` | `Brightness`, `Contrast` |
| `Levels` | `Black`, `White`, `Gamma` |
| `Gamma`, `Exposure`, `Vibrance` | Un réglage chacun |
| `White balance` | `Temperature`, `Hue` |
| `Color balance` | `Shadows (lift)`, `Midtones (gamma)`, `Highlights (gain)` |
| `Tint (duotone)` | Une couleur pour les ombres, une pour les lumières |
| `Split toning` | Teinte des ombres et des lumières |
| `Tone mapping` | `Curve` : ACES, filmique, Reinhard |
| `Invert` | Inverse |
| `Threshold` | Noir ou blanc selon un seuil |
| `Black and white` | La luminance |
| `Separate RGB`, `Combine RGB` | Sépare ou réunit les canaux |
| `Alpha over` | Pose une image sur une autre selon son alpha |

### Math

| Nœud | Rôle |
|---|---|
| `Math` | Une opération sur A et B. Voir la liste plus bas |
| `Vector math` | Opérations sur des vecteurs : ajouter, soustraire, multiplier, diviser, échelle, produit scalaire, produit vectoriel, longueur, distance, normaliser, mélange, réfléchir, projeter, minimum, maximum, absolu, arrondi, fraction, sinus, cosinus |
| `Map range` | Ramène une valeur d'une plage à une autre |
| `Clamp` | Borne une valeur |
| `Mix values` | Mélange deux nombres |
| `Separate XYZ`, `Combine XYZ` | Sépare ou réunit un vecteur |
| `Compare` | Égal, différent, inférieur, inférieur ou égal, supérieur, supérieur ou égal. Avec une `Tolerance` |
| `Logic` | Et, ou, non, ou exclusif, non-et, non-ou |
| `Switch` | Choisit `If true` ou `If false` selon une `Condition` |
| `Random` | Une valeur au hasard entre `Min` et `Max`, selon une `Seed` |
| `Expression` | Une formule écrite. Voir [Expressions](noeuds-editeur.md#expressions) |

Opérations du nœud `Math` : ajouter, soustraire, multiplier, diviser, puissance, minimum, maximum, modulo, sinus, cosinus, tangente, arc sinus, arc cosinus, arc tangente, arc tangente (A / B), absolu, signe, arrondi, arrondi inférieur, arrondi supérieur, tronquer, fraction, racine, exponentielle, logarithme, inférieur à, supérieur à, smoothstep, aimanter (pas B), boucler (0 à B), ping-pong (0 à B), inverse (1 / A), opposé, en radians, en degrés, minimum doux, maximum doux.

### Shader

`Custom shader` : un nœud de code. Voir [Expressions](noeuds-editeur.md#nœud-custom-shader). Exemples fournis : mélange par bruit, dégradé radial, rayures animables, noir et blanc contrasté, teinte cinéma, cel shading.

### Structure

`Reroute`, `Repeat zone`, groupes. Voir [L'éditeur de nœuds](noeuds-editeur.md#groupes).

## Paramètre exposé

Le nœud `Exposed parameter` rend un réglage visible **de l'extérieur** du graphe. C'est ce qui fait d'un matériau un **matériau maître** pour l'Atelier.

1. Ajoute un `Exposed parameter`.
2. Choisis son `Type` : couleur ou valeur.
3. Renomme-le (`F2`) : « couleur de la peinture », « usure ». Sans nom, il s'appelle « paramètre » suivi de son numéro.
4. Branche-le là où tu aurais mis une couleur ou une valeur fixe.

Dans l'Atelier, chaque calque posé avec ce maître montre ce réglage, et peut le changer sans toucher au graphe.

## Exemple : une rouille qui ronge les arêtes

1. `Noise` (échelle 8) vers un `Color ramp` brun-orangé : la rouille.
2. `Color` gris-bleu : la peinture.
3. `Edge wear` : le masque.
4. `Mix`, avec le masque en `Factor`, la peinture en A, la rouille en B. Branche sur `Color` de la sortie.
5. Branche le même masque sur `Roughness` à travers un `Map range` (0,3 pour la peinture, 0,9 pour la rouille).
6. Passe l'aperçu sur `Cube`.

## Les UV

Les nœuds `Coordinates` et `Texture coordinates` utilisent les vraies UV quand le modèle en a. Sinon, les motifs se posent selon la position sur l'objet.

## Limites

- Le relief (`Bump`) change l'ombrage, pas la forme. Pour un vrai déplacement de la surface, utilise le modificateur `Displace (noise)`.
- Les matériaux sont calculés sur le processeur dans la vue. Un graphe très lourd (beaucoup de bruits fractals) ralentit l'affichage.
- Le tracé de chemins sur carte graphique évalue le matériau aux coins de chaque triangle, puis interpole. Un motif fin sur de grands triangles y perd du détail. Subdivise le modèle si besoin.
- Les nœuds de la catégorie `Wear`, `Curvature` et `Geometry` ont besoin d'un modèle : ils ne donnent rien d'utile dans un compositing.
