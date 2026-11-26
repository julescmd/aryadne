---
titre: La plume et les tracés
niveau: débutant
duree: 15 min
prerequis: 04-vectoriel/01-premieres-formes
objectifs: Tracer des droites et des courbes à la plume ; Corriger un tracé point par point ; Dessiner à main levée ; Faire varier l'épaisseur d'un contour
---

## Ce que tu vas réussir à faire

Tu vas dessiner une feuille à la plume, corriger sa courbe point par point, puis donner à son contour un plein et un délié.

Un tracé est une suite de points. Entre deux points, le trait est droit ou courbe. Les poignées d'un point règlent sa courbe.

```essayer
nouveau: aryvec
nom: feuille
```

```etape
Clique **Essaie par toi-même**.
verifier: fichier.ouvert = aryvec
```

## Des segments droits

```etape
Appuie sur `P` pour l'outil **Pen**. Clique trois fois à des endroits différents : tu obtiens deux segments droits.
```

Un clic pose un point d'angle.

```etape
Reviens cliquer sur le premier point : le tracé se ferme. Tu as un triangle.
verifier: calques >= 1
```

## Des courbes

Pour une courbe, ne clique pas : glisse. Le glissement tire les poignées du point.

```etape
Avec la plume, clique pour poser la base de la feuille. Plus haut, clique et glisse vers le haut : une courbe se forme.
```

```etape
Pose la pointe de la feuille par un clic simple. Redescends de l'autre côté avec un clic glissé. Ferme sur le premier point.
verifier: calques >= 2
```

```astuce
Pose peu de points. Une belle courbe tient souvent en deux points bien réglés. Tu corrigeras ensuite.
```

## Corriger avec la sélection directe

L'outil **Direct selection** (`A`) montre les points d'un tracé.

```etape
Appuie sur `A`. Clique la feuille. Ses points apparaissent.
```

```etape
Glisse un point pour le déplacer. Glisse une de ses poignées pour changer la courbe.
```

| Action | Geste |
|---|---|
| Déplacer un point ou une poignée | Glisser |
| Retirer les points choisis | `Suppr` |
| Fermer ou ouvrir le tracé | **Closed** |
| Réduire le nombre de points | **Simplify** |
| Couper le tracé à un point | **Cut the path at the chosen point** |
| Joindre deux tracés ouverts, ou fermer un tracé | `Ctrl+J` |

```etape
Choisis un point de la feuille et appuie sur `Suppr`. La forme se simplifie. Annule avec `Ctrl+Z`.
```

## Dessiner à main levée

L'outil **Pencil** (`N`) trace un trait à main levée. Aryadne le lisse.

```etape
Appuie sur `N`. Dessine la nervure centrale de la feuille d'un seul geste.
verifier: calques >= 3
```

Finir un trait près de son point de départ ferme le tracé.

```etape
Le trait a trop de points ? Choisis-le, puis clique **Simplify** : moins de points, même allure.
```

## Un contour vivant

Un contour n'a pas à être uniforme.

| Réglage de **Stroke** | Choix |
|---|---|
| **Brush** | **Uniform**, **Calligraphic**, **Taper**, **Scatter** |
| Les extrémités | **Butt cap**, **Round cap**, **Square cap** |
| Les angles | **Miter join**, **Round join**, **Bevel join** |
| **Dashed** | Des pointillés |
| La flèche | Au bout d'une ligne ou d'un tracé ouvert |

```etape
Choisis la nervure avec `V`. Dans **Stroke**, passe **Brush** sur **Taper** : le trait s'affine vers ses bouts.
```

### L'outil Width

L'outil **Width** (`W`) épaissit un contour à l'endroit que tu veux.

```etape
Appuie sur `W`. Glisse sur le contour de la feuille, à mi-hauteur : le trait s'épaissit à cet endroit.
```

`Alt+clic` sur un point de largeur le retire.

## Le contour devient une forme

**Outline stroke** change le contour en forme pleine. Utile avant d'envoyer un logo : l'épaisseur ne dépend plus d'un réglage.

```etape
Choisis la nervure. Clique **Outline stroke**. Passe en sélection directe (`A`) : le trait est devenu une forme avec ses propres points.
```

## Importer un tracé

- **Import an SVG** ouvre un fichier SVG. Ses tracés deviennent des formes.
- `Ctrl+V` colle un SVG copié depuis un autre logiciel.
- **Lucide icon library** donne plus de 2 400 icônes au trait. Cherche en anglais : heart, arrow, user. L'icône arrive en tracés modifiables.

```etape
Ouvre **Lucide icon library**. Cherche `leaf`. Pose l'icône à côté de ta feuille et compare.
```

```quiz
Avec la plume, comment obtenir une courbe ?
- [ ] En double-cliquant.
- [x] En cliquant et en glissant.
- [ ] En tenant Maj.
explication: Un clic simple pose un angle. Un clic glissé tire les poignées et courbe le trait.
```

```quiz
Quel outil montre et déplace les points d'un tracé ?
- [ ] Select (V)
- [x] Direct selection (A)
- [ ] Width (W)
explication: V agit sur la forme entière. A agit sur ses points et leurs poignées.
```

```defi
Dessine la lettre S à la plume, en quatre points au plus, avec un contour **Calligraphic**.
indice: Deux points aux extrémités, deux au milieu des courbes. Règle les poignées avec `A`.
```

## Récapitulatif

- Plume : clic pour un angle, clic glissé pour une courbe.
- `A` corrige les points. **Simplify** en retire. `Ctrl+J` joint ou ferme.
- **Brush**, l'outil **Width** et **Outline stroke** travaillent le contour.

Leçon suivante : [Combiner des formes](03-combiner-des-formes.md)
