---
titre: Références, palettes et croquis de pose
niveau: intermédiaire
duree: 15 min
prerequis: 02-notes/06-premier-moodboard
objectifs: Parcourir toutes les images du coffre ; Tirer une palette d'un board ; Juger les valeurs en gris ; Lancer un diaporama de croquis ; Garder le board au-dessus d'un autre logiciel
---

## Ce que tu vas réussir à faire

Tu vas tirer une palette de couleurs de tes références, vérifier tes valeurs en gris, puis lancer une séance de croquis chronométrée. À la fin, ton board reste affiché au-dessus de ton logiciel de dessin.

```essayer
nouveau: aryboard
nom: palette-et-poses
```

```etape
Clique **Essaie par toi-même**. Glisse quatre ou cinq images sur le board, comme dans la leçon précédente.
verifier: onglet = 2d
```

## La page References

Les étapes 2D, 3D et Motion ont une page **References** dans le rail. C'est la galerie de toutes les images du coffre.

```etape
Clique l'icône **References** dans le rail.
```

| Élément | Effet |
|---|---|
| Le filtre | Cherche par nom ou par dossier |
| Le tri | **Recent**, **Name**, **Color** |
| **Scale** | La taille des vignettes |
| Double-clic | Ouvre l'image |
| Glisser | Vers un board |
| `Ctrl+V` | Colle une image ou un lien |

```etape
Change le tri pour **Color**. Les images se rangent par couleur.
```

Le détail d'une référence montre sa palette de 5 couleurs. Un clic sur une couleur copie son code. **Add to board** l'ajoute au board. **Auto board (colors)** ajoute cette image et celles aux couleurs proches.

```etape
Reviens au board : clique l'outil **Moodboard**, ou appuie sur `Alt+1`.
```

## Une palette depuis les images

Le panneau **Inspector (moodboard)** a deux onglets : **Properties** et **Colors**.

```etape
Ouvre l'onglet **Colors** de l'inspecteur. **Board palette** montre les couleurs dominantes de tout le board.
```

```etape
Choisis deux images. La palette devient **Selection palette** : les couleurs de ces images seulement.
```

| Élément | Effet |
|---|---|
| Clic sur une couleur | Copie son code |
| **Copy** | Tous les codes, un par ligne |
| **Export** | Un fichier `.gpl` dans le dossier `palettes/` du coffre. GIMP, Krita, Inkscape et Blender le lisent |
| **On the board** | Pose la palette comme un élément du board |
| **Colors** | Le nombre de couleurs |
| **Harmony** | Une harmonie autour d'une couleur : analogue, complément, triade, monochrome |
| **Variations** | Des variantes de la palette |

```etape
Clique **On the board**. La palette est posée sur la toile, à côté de tes images.
```

```astuce
Le menu **Add › Palette from selection** fait la même chose depuis la barre du board.
```

## Juger les valeurs

Une image qui marche en gris marche en couleur. Le board te le montre en un clic.

```etape
Ouvre le menu **View** du board et coche **Everything in grayscale**. Regarde quelles images gardent une lecture claire.
```

```etape
Décoche **Everything in grayscale**.
```

Pour une seule image : clic droit, **Grayscale**.

## Une grille de composition

Dans l'onglet **Properties** de l'inspecteur, **Composition grid** pose une grille sur l'image choisie : **Thirds**, **Golden ratio**, **Diagonals** ou **Center**.

```etape
Choisis une image. Dans **Properties**, essaie la grille **Thirds**.
```

Le même onglet montre la **Source** de l'image : l'adresse web ou le chemin d'origine.

## Le moodboard automatique

Menu **Add**, section **Automatic moodboard**.

| Commande | Effet |
|---|---|
| **Images by keywords** | Tape des mots-clés. Aryadne ajoute les images dont le nom, le dossier ou les notes qui les citent correspondent |
| **Images with similar colors** | Choisis d'abord une seule image. Aryadne ajoute les images du coffre aux couleurs proches |

```attention
Ces deux commandes cherchent dans ton coffre, pas sur internet. Elles ne trouvent que des images que tu as déjà importées.
```

## Le diaporama de croquis

```etape
Ouvre **View › Slideshow (gesture drawing)**. Choisis une durée : **30 seconds**, **1 minute**, **2 minutes**, **5 minutes** ou **No timer**.
```

Les images choisies défilent une à une, avec un minuteur. Sans sélection, toutes les images passent.

| Touche | Effet |
|---|---|
| `←` `→` | Image précédente, suivante |
| `Espace` | Pause |
| `Échap` | Fermer |

```etape
Dessine une pose sur papier pendant une image, puis ferme le diaporama avec `Échap`.
```

## Garder le board au-dessus

```etape
Dans le menu **View** du board, coche **Always on top**. Appuie ensuite sur `Ctrl+Maj+M` pour le mode compact.
```

Il ne reste que le board, au-dessus de Blender, de Krita ou d'un autre logiciel.

```etape
Appuie encore sur `Ctrl+Maj+M` pour retrouver les panneaux. Décoche **Always on top**.
```

## Échanger avec Obsidian

Le menu qui porte le nom du board contient **Import a .canvas (Obsidian)…** et **Export as .canvas…**. L'export reprend images, notes, textes et palettes. Les traits à main levée restent dans le board.

```quiz
Où va le fichier créé par le bouton Export de la palette ?
- [ ] Sur le bureau.
- [x] Dans le dossier `palettes/` du coffre, au format `.gpl`.
- [ ] Dans `refs/`.
explication: Le format .gpl est lu par GIMP, Krita, Inkscape et Blender.
```

```quiz
À quoi sert Everything in grayscale ?
- [ ] À alléger le fichier du board.
- [x] À juger les valeurs sans être distrait par la couleur.
- [ ] À convertir les fichiers en noir et blanc.
explication: C'est un affichage. Les fichiers ne changent pas.
```

```defi
Monte un board pour une scène de nuit : huit images, une palette de cinq couleurs posée sur la toile, et son export en `.gpl`.
indice: Importe d'abord les images. Dans **Colors**, règle le nombre de couleurs sur 5, puis **On the board** et **Export**.
```

## Récapitulatif

- La page **References** réunit toutes les images du coffre, triables par couleur.
- L'onglet **Colors** tire une palette du board ou de la sélection, et l'exporte.
- **Slideshow** chronomètre tes croquis. **Always on top** garde le board devant.

Parcours suivant : [Retouche à calques](../03-2d/index.md)
