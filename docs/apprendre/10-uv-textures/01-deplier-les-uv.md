---
titre: Déplier les UV
niveau: intermédiaire
duree: 15 min
prerequis: 07-3d-bases/02-selectionner-et-transformer
objectifs: Comprendre ce que sont les UV ; Déplier un modèle en un clic ; Juger l'étirement au damier ; Lire îlots et remplissage ; Retoucher dans l'éditeur UV
---

## Ce que tu vas réussir

Tu vas déplier un cube, puis une forme plus retorse. Tu sauras dire d'un coup d'œil si un dépliage est bon.

```essayer
nouveau: aryscene
nom: deplier-les-uv
```

## L'idée, avec une boîte en carton

Prends une boîte. Coupe quelques arêtes, ouvre-la, pose-la à plat. Tu obtiens un patron.

Déplier un modèle, c'est la même chose.

- Les arêtes coupées sont les **coutures**.
- Chaque morceau à plat est un **îlot**.
- Le patron entier tient dans un carré : le carré UV. L'image de texture se pose dessus.

## Entre dans le mode

```etape
Choisis le cube. Dans la barre de la vue, passe en mode **UV & materials**.
verifier: scene.selection
```

Le mode a quatre onglets : **Materials**, **Seams**, **UV**, **Retopo**. Une ligne d'état, dessous, compte les faces, les faces choisies et les coutures.

```etape
Ouvre l'onglet **UV**.
```

## Déplie en un clic

```etape
Clique **Auto (seams + UVs)**. L'éditeur UV, à gauche de la vue, montre le patron du cube.
```

Ce bouton fait quatre choses d'affilée : il pose des coutures, déplie, redresse les îlots et les range.

La ligne **Islands** / **Fill** donne le nombre d'îlots et la part du carré UV qu'ils occupent.

## Juge au damier

```etape
Clique **Checker**. Un damier s'affiche sur le modèle.
```

Le damier dit tout.

| Ce que tu vois | Ce que ça veut dire |
|---|---|
| Des carrés réguliers, de même taille partout | Bon dépliage |
| Des carrés étirés en rectangles | La surface est étirée à cet endroit |
| Des carrés plus gros d'un côté | La densité n'est pas égale |
| Un motif qui casse | Une couture passe là |

Sur un cube, les carrés sont parfaits : six faces plates se déplient sans étirement.

## Une forme plus difficile

```etape
Reviens à la scène avec **Back to the scene**. Ajoute une **Sphere**. Choisis-la et reviens en mode **UV & materials**, onglet **UV**.
verifier: scene.objets >= 4
```

```etape
Clique **Auto (seams + UVs)**, puis **Checker**.
```

Une sphère ne se met pas à plat sans tricher : pense à une carte du monde. Regarde où les carrés se déforment, et où passent les coutures.

Il y a toujours un compromis.

- Peu d'îlots : peu de coutures, mais de l'étirement.
- Beaucoup d'îlots : pas d'étirement, mais des coutures partout.

## L'éditeur UV

L'éditeur UV montre le patron. Tu peux y retoucher les îlots.

| Geste | Effet |
|---|---|
| Clic | Choisir un sommet ou un îlot |
| L | L'îlot sous le pointeur |
| G, R, S | Déplacer, tourner, mettre à l'échelle |
| X ou Y, après G | Imposer un axe |
| A, Alt+A | Tout, rien |
| Origine | Cadrer |
| Clic droit | Le menu |

```etape
Dans l'éditeur UV, survole un îlot et appuie sur L. Appuie sur G et déplace-le. Valide d'un clic.
```

```etape
Appuie sur R et tourne l'îlot. Le damier tourne sur le modèle, à cet endroit.
```

```etape
Clique **Pack**. Les îlots sont rangés de nouveau, serrés, à la même densité.
```

```astuce
**UV editor in this panel** place l'éditeur UV dans son propre panneau. Pratique sur un second écran.
```

## Retour à la scène

```etape
Clique **Back to the scene**, ou appuie sur Échap. Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```attention
Les UV sont liées au nombre de sommets. Un modificateur qui change la topologie, comme **Decimate** ou **Remesh (voxels)**, ou une retopologie, les invalide. Déplie en dernier.
```

```quiz
À quoi sert le damier ?
- [ ] À colorer le modèle.
- [x] À juger l'étirement et la densité du dépliage.
- [ ] À placer les coutures.
explication: Des carrés réguliers et de même taille partout signalent un bon dépliage.
```

```quiz
Qu'est-ce qu'un îlot ?
- [x] Un morceau de surface mis à plat.
- [ ] Une face du modèle.
- [ ] Une image de texture.
explication: Les coutures découpent la surface en îlots. Chaque îlot est posé à plat dans le carré UV.
```

```defi
Déplie un Torus et un Cone. Lequel donne le damier le plus régulier, et pourquoi ?
indice: Un cône et un tore se déroulent presque sans étirement, comme un cylindre. Regarde où passent leurs coutures.
```

## En trois lignes

- Les UV posent une image plate sur une surface en volume.
- **Auto (seams + UVs)** déplie en un clic. **Checker** montre la qualité.
- L'éditeur UV se manie comme la vue : L, G, R, S.

[Suite : Les coutures](02-les-coutures.md)
