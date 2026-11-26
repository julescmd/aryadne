# Retouche à calques

Étape **2D**, outil `Retouch`. Fichier : `.aryimg`. Dans l'esprit de Photoshop : calques, masques, modes de fusion, pinceaux, réglages non destructifs.

## Ouvrir ou créer

- `File › New › Layered image` : choisis la taille (dont `A4 at 150 dpi`) et le fond (`White background` ou `Transparent background`).
- Ouvrir une image ordinaire (PNG, JPG…) la montre telle quelle. La modifier en fait une image à calques.
- Un `.psd` s'ouvre comme une image à calques, avec ses calques, groupes, masques et modes de fusion.

## Naviguer

| Geste | Effet |
|---|---|
| Molette | Zoom |
| `Espace` + glisser | Déplacer la vue |
| `4`, `6` | Tourner la vue de 15° |
| `5` | Remettre la vue droite |

Boutons de la vue :

| Bouton | Effet |
|---|---|
| `Before / after` | L'image à l'ouverture à gauche, l'état actuel à droite. Glisse la séparation |
| `Rulers and guides (Ctrl+R)` | Affiche les règles. Tire depuis une règle pour poser un repère |
| `Mirror view` | Voir l'image en miroir pour repérer les défauts. L'image ne change pas |
| Tourner la vue | Comme `4` et `6` |

## Les outils

| Outil | Touche | Usage |
|---|---|---|
| `Move layer` | `V` | Déplacer le calque actif |
| `Rectangle select` | `M` | Sélection en rectangle |
| `Elliptical selection` | `O` | Sélection en ellipse |
| `Lasso` | `L` | Sélection à main levée |
| `Magic wand` | `W` | Sélection par couleur proche |
| `Crop` | `C` | Recadrer |
| `Brush` | `B` | Peindre |
| `Clone stamp` | `S` | Recopier une zone. `Alt+clic` choisit la source |
| `Spot healing` | `J` | Peindre sur une tache pour l'effacer |
| `Smudge` | `R` | Étaler la peinture. Le flux règle la force |
| `Liquify` | `U` | Pousser les pixels pour déformer |
| `Eraser` | `E` | Gommer |
| `Paint bucket` | `G` | Remplir une zone |
| `Gradient` | `G` | Dégradé de la couleur vers le transparent |
| `Text` | `T` | Poser du texte |
| `Eyedropper` | `I` | Prendre une couleur |

`G` est partagé par le pot de peinture et le dégradé.

### Couleurs

Deux pastilles : `Foreground color` et `Background color`. `X` les échange. `D` remet noir et blanc.

## Options du pinceau

| Réglage | Effet |
|---|---|
| `Size` | Diamètre. `[` et `]` le changent |
| `Hardness` | Dureté du bord |
| Flux | Quantité de peinture déposée à chaque passage |
| `Opacity` | Opacité du trait |
| `Smoothing` | Stabilisateur : le trait suit la souris avec du retard, pour des lignes propres |
| Pression → diamètre | La pression de la tablette règle la taille |
| Pression → opacité | La pression de la tablette règle l'opacité |
| `Grain` | Grain du papier : le pinceau accroche comme un crayon ou une craie |

### Pinceaux prêts

Crayon, encre, feutre, craie, aérographe, peinture sèche, pinceau rond. Chacun règle d'un coup taille, dureté, flux, grain, pression et lissage.

### Aides au tracé

| Bouton | Effet |
|---|---|
| Symétrie verticale | Le trait se reflète de l'autre côté de l'axe vertical |
| Symétrie horizontale | Le trait se reflète de l'autre côté de l'axe horizontal |
| `Wrap around` | L'image se répète autour d'elle-même. Le trait passe d'un bord à l'autre. Pour les textures qui se raccordent |
| `Perspective` | Des points de fuite, à déplacer à la souris. Le trait suit leurs lignes |
| `Snap strokes to the vanishing lines` | Aimante le trait sur les lignes de fuite |

## Sélections

| Commande | Raccourci |
|---|---|
| `Select all` | `Ctrl+A` |
| `Deselect` | `Ctrl+D` |
| `Invert selection` | `Ctrl+Maj+I` |
| Ajouter à la sélection | `Maj` pendant le tracé |
| Retirer de la sélection | `Alt` pendant le tracé |
| Effacer le contenu | `Suppr` |

Une sélection limite la peinture, les réglages et les filtres à sa zone.

## Recadrer

1. Outil `Crop` (`C`).
2. Glisse pour tracer le cadre.
3. Choisis un format : `Free`, `Image ratio`, 1:1, 4:3, 16:9, 9:16, 4:5.
4. `Entrée` applique. `Échap` annule.

## Transformer

| Commande | Raccourci | Effet |
|---|---|---|
| `Free transform` | `Ctrl+T` | Déplacer, redimensionner, tourner le calque. `Maj` : angle par pas de 15° |
| `Warp grid` | | Tire les points d'une grille pour courber le calque : tissus, reflets, étiquette sur un objet |
| `Flip horizontally`, `Flip vertical` | | Retourner |

`Entrée` garde. `Échap` annule. `Reset the grid` remet la grille droite.

Un calque de réglage, un calque verrouillé ou un masque ne se transforment pas.

## Calques

Le panneau `Layers` liste les calques, du haut vers le bas.

| Action | Raccourci |
|---|---|
| `New layer` | `Ctrl+Maj+N` |
| `Layer from an image…` | |
| `Duplicate layer` | `Ctrl+J` |
| `Merge down` | `Ctrl+E` |
| `Group layer` | `Ctrl+G` |

Chaque calque a : un œil, une `Opacity`, un `Fill` (l'opacité du contenu sans ses effets), un mode de fusion, deux verrous (`Lock transparent pixels`, et le verrou complet).

### Modes de fusion

Rangés comme dans Photoshop :

| Groupe | Modes |
|---|---|
| Normal | `Normal` |
| Assombrir | `Darken`, `Multiply`, `Color Burn`, `Linear Burn` |
| Éclaircir | `Lighten`, `Screen`, `Color Dodge`, `Linear Dodge (Add)` |
| Contraste | `Overlay`, `Soft Light`, `Hard Light`, `Vivid Light`, `Linear Light`, `Pin Light` |
| Comparaison | `Difference`, `Exclusion`, `Subtract`, `Divide` |
| Couleur | `Hue`, `Saturation`, `Color`, `Luminosity` |

### Masques

Un masque de fusion est une image grise attachée au calque : le blanc montre, le noir cache.

| Action | Effet |
|---|---|
| `Add Layer Mask (Reveal All)` | Masque blanc : tout est visible |
| `Add Layer Mask (Reveal Selection)` | Seule la sélection reste visible |
| `Add Layer Mask (Hide Selection)` | La sélection est cachée |
| Clic sur la vignette du masque | Peindre dans le masque |
| `Alt+clic` | Voir le masque seul |
| `Maj+clic` | Activer ou désactiver le masque |

`Create Clipping Mask` : le calque n'apparaît que sur les pixels du calque de dessous. `Release Clipping Mask` l'annule.

### Groupes

Un groupe est un dossier de calques. Il a sa propre opacité, son mode de fusion (`Pass Through` par défaut : les calques se mélangent directement avec le dessous) et son masque. Les groupes s'emboîtent.

### Calques de réglage

`New adjustment layer` : un calque sans pixels qui change la couleur de tout ce qui est dessous. Non destructif.

| Réglage | Curseurs |
|---|---|
| Luminosité/Contraste | `Brightness`, `Contrast` |
| Niveaux | `Blacks`, `Gamma`, `Whites` |
| Exposition | EV |
| Teinte/Saturation | `Hue`, `Saturation`, `Brightness` |
| Noir et blanc | |
| Inverser | |
| Seuil | Seuil |
| `Posterize` | Nombre de niveaux |
| Grain | `Amount`, `Grain size` |
| `Clarity` | Contraste des tons moyens. Les noirs et les blancs bougent peu |
| `Dehaze` | Retire la brume claire qui délave l'image. Négatif : en ajoute |
| Vibrance | Sature surtout les couleurs ternes |

### Styles de calque

`Blending Options (styles)…` : ombre portée, ombre interne, lueur externe, biseau, incrustation de couleur, `Gradient Overlay`, contour.

`Blend if` (fusion conditionnelle) : montre le calque seulement sur certains tons du dessous, par la luminosité, sans masque.

## Réglages et filtres directs

Appliqués aux pixels du calque actif (ou à la sélection) :

- `Brightness and contrast…`, `Hue and saturation…` ;
- `Invert colors` (`Ctrl+I`) ;
- filtres : `Light blur`, `Strong blur`, `Sharpness`, `Noise`, `Pixelate`.

Préfère un calque de réglage quand tu veux pouvoir revenir en arrière.

## Pot de peinture et mise en couleur

Options du pot :

| Réglage | Effet |
|---|---|
| Tolérance | Écart de couleur accepté |
| `Close gaps (px)` | Le remplissage ne passe pas par les ouvertures du trait plus petites que cette taille, et va jusqu'au trait |
| Palette de référence | Reprend les couleurs du calque actif (un dessin déjà colorié). `Maj+clic` : depuis une image de l'ordinateur. Clic sur une couleur : peindre avec. Clic droit : la retirer |

## Animation image par image

`Frame-by-frame animation` : les calques d'un groupe « Animation » deviennent les images.

| Élément | Effet |
|---|---|
| `,` et `.` | Image précédente, suivante |
| `New empty frame after this one` | Ajoute une image vide |
| `Copy this frame after it` | Duplique l'image |
| Cadence | Images par seconde |
| Tenue | Chaque dessin tenu un nombre d'images : animation en 1, en 2, en 3 |
| Clic droit sur une image | Combien d'images ce dessin reste à l'écran |
| `Onion skin` | L'image d'avant en rouge, celle d'après en vert |
| Colorier d'après l'image d'avant | Chaque zone fermée du trait reprend la couleur de la même zone sur l'image précédente |
| `MP4…`, `GIF…` | Exporter l'animation |

Pour le coloriage automatique : colorie la première image à la main. Les suivantes la reprennent. Si le trait n'est pas fermé, monte `Close gaps`.

## Historique

Le panneau `History` liste les états. Clique un état pour y revenir. `Ctrl+Z` annule, `Ctrl+Y` rétablit.

## Exporter

`Export as PNG, JPG, WebP or PSD…`.

À l'export PSD, ce qui ne passe pas encore est listé : « Not carried to the PSD yet: … ».

Le bouton d'envoi passe l'image à un autre outil (board, montage, 3D, nœuds). Une image `.aryimg` s'utilise directement dans le montage, le board, la 3D et les nœuds, comme une image ordinaire.

## Format du fichier

Un `.aryimg` contient un en-tête lisible (taille, réglages des calques) puis chaque calque en PNG. Voir [Formats](../technique/formats.md#image-à-calques-aryimg).

## Limites

- 8 bits par canal, RVB. Pas de 16 ou 32 bits, pas de CMJN.
- Le PSD est lu et écrit en 8 bits RVB. Les calques de texte, les objets dynamiques et les effets d'un PSD ne sont pas tous repris.
- Le texte posé devient des pixels.
- Pas de filtres dynamiques sur un calque (les filtres s'appliquent aux pixels).
- Les très grandes images demandent beaucoup de mémoire : chaque calque est gardé en entier.
