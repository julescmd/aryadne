---
titre: Tes premières formes
niveau: débutant
duree: 15 min
prerequis: 01-demarrer/03-creer-et-enregistrer
objectifs: Tracer un plan de travail ; Dessiner rectangles, ellipses, polygones et étoiles ; Régler remplissage et contour ; Sélectionner, déplacer, empiler et aligner
---

## Ce que tu vas réussir à faire

Tu vas dessiner un petit paysage en formes simples : un ciel, un soleil, une colline, une étoile. Tu sauras choisir, colorer et ranger chaque forme.

```essayer
nouveau: aryvec
nom: paysage
```

```etape
Clique **Essaie par toi-même**. Un dessin vide s'ouvre dans l'étape 2D, outil **Vector**.
verifier: fichier.ouvert = aryvec
```

## Naviguer

| Geste | Effet |
|---|---|
| Molette | Zoom |
| `Espace` + glisser | Déplacer la vue |
| `Ctrl+0` | Tout cadrer |

## Le plan de travail

Un plan de travail est une page : une affiche, un écran, une vignette. Un fichier peut en contenir plusieurs, posés sur la toile.

```etape
Si la toile n'a pas encore de plan de travail, appuie sur `F` pour l'outil **Artboard** et glisse pour en tracer un.
```

**Size…** propose des formats tout prêts : Square, Story, Post 4:5, YouTube thumbnail, HD screen, iPhone, Desktop, US Letter, A4.

## Un rectangle pour le ciel

```etape
Appuie sur `R` pour l'outil **Rectangle**. Glisse sur le plan de travail pour couvrir toute sa surface.
verifier: calques >= 1
```

La forme est choisie. Le panneau **Properties (vector)** montre ses réglages.

```etape
Dans la section **Fill**, clique la couleur. Choisis un bleu clair.
```

## Remplissage et contour

Chaque forme a deux couleurs.

- **Fill** : l'intérieur.
- **Stroke** : le contour, avec son épaisseur.

```etape
Appuie sur `E` pour l'outil **Ellipse**. Tiens `Maj` et glisse : tu traces un cercle parfait. C'est ton soleil.
verifier: calques >= 2
```

```etape
Donne au soleil un **Fill** jaune. Dans **Stroke**, choisis un orange et monte l'épaisseur.
```

```raccourci
`Maj` contraint la forme : carré avec le rectangle, cercle avec l'ellipse, angles de 45° avec la ligne.
```

## Polygone et étoile

| Outil | Touche | Réglages |
|---|---|---|
| **Polygon** | `Y` | **Sides**, **Radius** |
| **Star** | `S` | **Points**, **Outer**, **Inner** |
| **Line** | `L` | |

```etape
Appuie sur `S`. Trace une étoile dans le ciel. Dans les options de l'outil, change **Points** et **Inner**.
verifier: calques >= 3
```

```etape
Appuie sur `Y`. Dans les options de l'outil, règle **Sides** sur 3. Trace ce triangle en bas du plan : une colline. Donne-lui un vert.
verifier: calques >= 4
```

## Sélectionner et déplacer

```etape
Appuie sur `V` pour l'outil **Select**. Clique le soleil. Glisse-le vers un coin du ciel.
```

| Geste | Effet |
|---|---|
| Clic | Choisir |
| `Maj+clic`, ou tracer un rectangle | Choisir plusieurs formes |
| Glisser | Déplacer |
| Poignées | Redimensionner, tourner |
| Flèches | Décaler de 1. Avec `Maj` : de 10 |
| `Ctrl+D` | Dupliquer |
| `Alt` + glisser | Dupliquer en déplaçant |
| `Suppr` | Supprimer |

Pendant un déplacement, des repères magnétiques apparaissent : bords et centres des autres formes.

```etape
Tiens `Alt` et glisse l'étoile : tu en fais une copie. Refais-le pour avoir trois étoiles.
verifier: calques >= 6
```

## L'ordre de la pile

Les formes s'empilent. La dernière dessinée est devant.

```etape
Choisis la colline. Appuie sur `]` pour l'avancer, `[` pour la reculer. Place-la devant le ciel.
```

Le panneau **Layers (vector)** liste les formes sous **Layers**. Tu peux y choisir une forme cachée derrière une autre.

## Aligner

```etape
Choisis les trois étoiles avec `Maj+clic`. Dans le panneau des propriétés, clique **Top** pour les aligner en haut, puis **Distribute horizontally**.
```

L'alignement se fait par rapport à **The selection** ou à **The artboard**.

## Coins et aspect

| Section | Réglages |
|---|---|
| **Corners** | Les quatre coins d'un rectangle, ensemble ou un par un |
| **Appearance** | **Opacity**, le mode de fusion, **Drop shadow**, **Blur** |

```etape
Trace un petit rectangle : une maison. Monte **Corners** pour arrondir ses coins.
```

```etape
Enregistre avec `Ctrl+S`.
```

```quiz
Comment tracer un cercle parfait ?
- [ ] Avec l'outil Polygon.
- [x] Avec l'outil Ellipse, en tenant Maj.
- [ ] Avec l'outil Ellipse, en tenant Alt.
explication: Maj contraint les proportions. Alt + glisser, lui, duplique une forme déjà tracée.
```

```quiz
Que font les touches ] et [ ?
- [ ] Elles changent l'épaisseur du contour.
- [x] Elles avancent ou reculent la forme dans la pile.
- [ ] Elles zooment.
explication: L'ordre de la pile décide de ce qui passe devant.
```

```defi
Ajoute un dégradé au ciel : bleu en haut, rose en bas.
indice: Dans **Fill**, passe de **Solid** à **Linear**. Les points du dégradé se déplacent directement sur la toile.
```

## Récapitulatif

- `R`, `E`, `Y`, `S`, `L` tracent les formes. `Maj` les contraint.
- **Fill** colore l'intérieur, **Stroke** le contour.
- `V` choisit et déplace. `]` et `[` changent l'ordre. `Alt` + glisser duplique.

Leçon suivante : [La plume et les tracés](02-plume-et-traces.md)
