# Board et moodboard

Étape **2D**, outil `Moodboard`. Fichier : `.aryboard`.

Un board est une toile infinie, dans l'esprit de PureRef. On y pose des images de référence, des textes, des notes, des palettes, des vidéos et des modèles 3D, et on dessine par-dessus.

## Créer

`File › New › Board`, ou `New board` dans le menu du moodboard. Donne un nom : le fichier est créé dans `boards/`.

## Ajouter du contenu

| Contenu | Comment |
|---|---|
| Images | Glisse-les depuis l'explorateur, le bureau ou le navigateur. Ou `Ctrl+V` (image du presse-papiers, enregistrée dans `refs/`) |
| Un lien de page web | Glisse-le ou colle-le |
| Texte | Outil `Text` (`T`), puis clic |
| Note du coffre | Glisse une note : elle s'affiche sur le board |
| Palette | `Add › Palette from selection` |
| Vidéo, montage | Glisse le fichier. Il se lit sur place |
| Modèle 3D | Glisse le fichier. Il se tourne à la souris |

Les images gardent leurs proportions. Une image qui a disparu affiche « File not found », avec `Relink to a file…`.

## Naviguer

| Geste | Effet |
|---|---|
| Molette | Zoom |
| Clic milieu, clic droit ou `Espace` + glisser | Déplacer la vue |
| `View › Frame all` | Tout cadrer |
| `View › Actual size (100 %)` | Taille réelle |

## Outils

| Outil | Touche | Usage |
|---|---|---|
| `Select` | `V` | Choisir, déplacer, redimensionner |
| `Text` | `T` | Poser du texte |
| `Brush` | `B` | Dessin à main levée. `Maj` : ligne droite |
| `Eraser` | `E` | Gommer les traits |
| `Line` | `L` | Ligne |
| `Frame` | `R` | Rectangle |
| `Ellipse` | `O` | Ellipse |
| `Arrow` | `Maj+A` | Flèche |

Options du trait : couleur, `Thickness`, `Opacity`.

## Sélection et édition

| Geste | Effet |
|---|---|
| Clic, `Maj+clic`, lasso | Choisir |
| `Ctrl+A` | Tout choisir |
| Poignées | Redimensionner. `Maj` : sans garder les proportions |
| `Alt` + glisser un coin d'une image | Recadrer, sans toucher au fichier |
| `Ctrl+D` | Dupliquer |
| `Ctrl+C`, `Ctrl+X`, `Ctrl+V` | Copier, couper, coller |
| `Suppr` | Supprimer |
| `Ctrl+Z`, `Ctrl+Y` | Annuler, rétablir |

Menu `Select` : `Select all`, `All images`, `None`.

### Clic droit sur un élément

`Open`, `Foreground`, `Background`, `Flip horizontally`, `Grayscale`, `Remove crop`, `Original size`, `Duplicate`, `Delete`. Pour un texte : taille, police, couleur, fond, gras, italique, souligné, barré, alignement.

Dans un texte, la mise en forme s'écrit aussi à la main : `**gras**`, `*italique*`, `__souligné__`, `~~barré~~`.

## Disposer

Menu `Arrange`, sur la sélection ou sur tout le board :

| Commande | Effet |
|---|---|
| `Packed rows` | Range en rangées serrées |
| `Same height` | Toutes les images à la même hauteur |
| `In a row`, `In a column` | En ligne, en colonne |
| `Stack` | Empile |
| `Align selection` | `Left`, `Center`, `Right`, `Top`, `Middle`, `Bottom` |

## Calques

Le panneau `Layers (moodboard)` liste les éléments par genre : `Image`, `Text`, `Note`, `Palette`, `Stroke`. Un clic choisit l'élément.

## Inspecteur

Panneau `Inspector (moodboard)`, deux onglets.

### Properties

Pour l'élément choisi :

- **Source** : d'où vient l'image (adresse web ou chemin d'origine). `Open the source`, `Copy` ;
- `Scale` ;
- **Composition grid** par-dessus l'image : aucune, `Thirds`, `Golden ratio`, `Diagonals`, `Center` ;
- `Open`.

### Colors

Le générateur de palettes.

| Élément | Effet |
|---|---|
| `Board palette` / `Selection palette` | Les couleurs dominantes des images du board, ou de la sélection |
| Clic sur une couleur | Copie son code |
| `Copy` | Les codes hexadécimaux, un par ligne |
| `Export` | Un fichier `.gpl` (GIMP, Krita, Inkscape, Blender) dans le dossier `palettes/` du coffre |
| `On the board` | Pose la palette comme un élément du board |
| `Harmony` | Autour d'une couleur : analogue, complément, triade, monochrome, aléatoire |
| `Colors` | Nombre de couleurs |
| `Saturation`, `Contrast` | Réglages d'ensemble |
| `Readable text on each color (4.5:1)` | Vérifie qu'un texte reste lisible sur chaque couleur |
| `Variations` | Des variantes de la palette |

## Juger les valeurs et les couleurs

| Commande | Effet |
|---|---|
| `View › Everything in grayscale` | Tout le board en gris, pour juger les valeurs |
| Clic droit › `Grayscale` | Une seule image en gris |

## Moodboard automatique

Menu `Add › Automatic moodboard` :

| Commande | Effet |
|---|---|
| `Images by keywords` | Tape des mots-clés (« forest mist »). Ajoute les images dont le nom, le dossier, ou les notes qui les citent correspondent |
| `Images with similar colors` | Choisis d'abord une seule image du board. Ajoute les images du coffre aux couleurs proches |

## Diaporama pour le croquis de pose

`View › Slideshow (gesture drawing)` : les images choisies (ou toutes), une à la fois, avec un minuteur.

- Durées : `30 seconds`, `1 minute`, `2 minutes`, `5 minutes`, `No timer`.
- `←` `→` : image précédente, suivante. `Espace` : pause. `Échap` : fermer.

## Au-dessus des autres logiciels

`View › Always on top` garde la fenêtre au-dessus de Blender, de Krita ou d'un autre logiciel, comme PureRef. Avec le `Compact mode` (`Ctrl+Maj+M`), il ne reste que le board.

## Éléments vivants

- **Vidéos et montages** : clic droit, `Play / stop here`, `Loop`. Demande ffmpeg.
- **Modèles 3D** : `Alt` + glisser tourne le modèle. Clic droit, `Turntable` le fait tourner tout seul.

## Les références

La page `References` (dans les étapes 2D, 3D, Motion) est la galerie de toutes les images du coffre.

| Élément | Effet |
|---|---|
| Filtre | Par nom ou dossier |
| Tri | `Recent`, `Name`, `Color` |
| `Scale` | Taille des vignettes |
| Double-clic | Ouvrir |
| Glisser | Vers un board |
| `Ctrl+V` | Coller une image ou un lien |

Le détail d'une référence montre sa palette de 5 couleurs (clic pour copier), `Add to board`, et `Auto board (colors)` : ajoute cette image et les références aux couleurs proches à un board.

## Obsidian Canvas

- `Import a .canvas (Obsidian)…` : les nœuds texte, fichier, lien et groupe deviennent des éléments. Les liens entre nœuds deviennent des flèches. Le `.canvas` d'origine reste tel quel.
- `Export as .canvas…` : images, notes, textes et palettes. Les traits à main levée restent dans le board.

## Limites

- Les traits dessinés sont vectoriels et simples : pas de calques de dessin, pas de pression de tablette.
- Les images sont liées, pas copiées dans le fichier du board. Si tu déplaces une image hors d'Aryadne, il faut la re-lier.
- Pas de collaboration en direct : un board est un fichier.
