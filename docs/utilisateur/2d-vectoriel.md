# Dessin vectoriel

Étape **2D**, outil `Vector`. Fichier : `.aryvec`. Dans l'esprit d'Illustrator et de Figma : des formes nettes à toutes les tailles, pour les logos, les illustrations et les maquettes d'écrans.

## L'écran

| Zone | Contenu |
|---|---|
| À gauche | Les outils |
| Au centre | La toile, infinie, avec ses plans de travail |
| `Layers (vector)` | `Artboards` et `Layers` |
| `Properties (vector)` | Les réglages de la sélection |
| `Tool options (vector)` | Les options de l'outil en cours |

## Naviguer

| Geste | Effet |
|---|---|
| Molette | Zoom |
| `Espace` + glisser | Déplacer la vue |
| `Ctrl+0` | Tout cadrer |

## Plans de travail

Un plan de travail est une page : une affiche, un écran d'application, une vignette. Un fichier peut en contenir plusieurs, posés sur la toile.

- Outil `Artboard` (`F`) : glisse pour en tracer un.
- `Size…` propose des formats : Square (1080 × 1080), Story (1080 × 1920), Post 4:5, YouTube banner, YouTube thumbnail, HD screen, iPhone (393 × 852), Desktop (1440 × 1024), US Letter, A4.
- `Adapt…` : une copie du plan dans un autre format, posée à côté.

Réglages d'un plan :

| Réglage | Effet |
|---|---|
| `Background color` | Fond |
| `Column grid` | Des colonnes qui guident et aimantent la mise en page. Jamais exportées. Réglages : `Columns`, `Gutter`, `Margin` |
| `Device` | Montre le plan dans un cadre d'appareil (iPhone, Android, écran), barre d'état comprise. Affiché et exporté en SVG |
| `Mode` | Le mode des variables de couleur : clair, sombre… |
| `Perspective grid` | Un horizon et deux points de fuite, pour dessiner des décors. Jamais exportée. Réglages : `Horizon`, `Left vanishing`, `Right vanishing` |

## Les outils

| Outil | Touche | Usage |
|---|---|---|
| `Select` | `V` | Choisir, déplacer, redimensionner, tourner |
| `Direct selection` | `A` | Les points d'un tracé |
| `Pen` | `P` | Clic pour un point, glisser pour une courbe |
| `Pencil` | `N` | Un trait à main levée, lissé. Finir près du départ ferme le tracé |
| `Rectangle` | `R` | `Maj` : carré |
| `Ellipse` | `E` | `Maj` : cercle |
| `Polygon` | `Y` | Réglages : `Sides`, `Radius` |
| `Star` | `S` | Réglages : `Points`, `Outer`, `Inner` |
| `Line` | `L` | `Maj` : angles de 45° |
| `Text` | `T` | Clic pour un texte, glisser pour une zone |
| `Artboard` | `F` | Tracer un plan de travail |
| `Width` | `W` | Glisse sur un contour pour l'épaissir à cet endroit. `Alt+clic` retire le point |
| `Shape builder` | `M` | Glisse sur les zones des formes choisies pour les fusionner. `Alt` : effacer |
| `Comment` | `C` | Épingle une note sur la toile |
| `Live paint` | `K` | Clic dans une zone fermée par des traits pour la remplir. La couleur suit les traits quand on les retouche. `Alt+clic` prend la couleur |

## Sélectionner et ranger

| Geste | Effet |
|---|---|
| Clic, `Maj+clic`, rectangle | Choisir |
| `Ctrl+A` | Tout choisir |
| Flèches | Décaler de 1. `Maj` : de 10 |
| `]` et `[` | Avancer, reculer d'un cran dans la pile |
| `Ctrl+D` | Dupliquer |
| `Alt` + glisser | Dupliquer en déplaçant |
| `Ctrl+C`, `Ctrl+V` | Copier, coller |
| `Suppr` | Supprimer |
| `Ctrl+G` | Grouper |
| `Ctrl+Maj+G` | Dissocier |
| `Ctrl+Z`, `Ctrl+Y` | Annuler, rétablir |

Des repères magnétiques apparaissent pendant un déplacement : bords et centres des autres formes.

### Aligner

Avec plusieurs formes : `Left`, `Center`, `Right`, `Top`, `Middle`, `Bottom`, par rapport à `The selection` ou à `The artboard`. `Distribute horizontally` et `Vertically` répartissent.

## Remplissage et contour

### Fill (remplissage)

| Type | Effet |
|---|---|
| `Solid` | Une couleur |
| `Linear`, `Radial` | Dégradé. Les points se déplacent sur la toile. Clic droit retire un point |
| `Pattern` | Un motif : pois, rayures, quadrillage, damier, vagues, hachures croisées. Avec une couleur de fond, qui peut être transparente |
| `Freeform` | Des points de couleur libres qui se fondent |
| `Mesh` | Un filet de dégradé : `Mesh columns`, `Mesh rows`. Clique un nœud pour sa couleur, tire-le pour plier le filet |

### Stroke (contour)

| Réglage | Choix |
|---|---|
| Épaisseur, couleur | |
| `Brush` | `Uniform`, `Calligraphic`, `Taper`, `Scatter` |
| Extrémités | `Butt cap`, `Round cap`, `Square cap` |
| Angles | `Miter join`, `Round join`, `Bevel join` |
| `Dashed` | Pointillés |
| Flèche | Au bout d'une ligne ou d'un tracé ouvert |

### Appearance (aspect)

`Opacity`, mode de fusion (`Normal`, `Multiply`, `Screen`, `Overlay`, `Darken`, `Lighten`, `Color dodge`, `Difference`, `Color`), `Drop shadow`, ombre `Inner` (un bouton enfoncé, un creux), `Blur`.

### Coins

`Corners` règle les quatre coins d'un rectangle. Chaque coin se règle aussi à part : `Top left`, `Top right`, `Bottom right`, `Bottom left`.

## Tracés

Avec la sélection directe (`A`) :

- déplace les points et leurs poignées ;
- `Suppr` retire les points choisis ;
- `Closed` ferme ou ouvre le tracé ;
- `Simplify` : moins de points, même allure ;
- `Cut the path at the chosen point` ;
- `Ctrl+J` : joindre deux tracés ouverts, ou fermer un tracé.

## Combiner des formes

Avec plusieurs formes choisies, section **Combine** :

| Opération | Résultat |
|---|---|
| Union | Une seule forme, de la couleur de celle du dessous |
| `Subtract` | Les formes du dessus creusent celle du dessous |
| `Intersect` | Seulement la partie commune |
| Exclude | Tout sauf la partie commune |
| Even-odd | Les formes qui se recouvrent font des trous |
| Clipping mask | La forme du dessous découpe celles du dessus |
| `Opacity mask` | La clarté de la forme du dessous (un dégradé blanc vers noir) rend le dessus plus ou moins visible |

Ces opérations sont **non destructives** : les formes d'origine restent modifiables dans le groupe booléen.

Autres outils sur la sélection :

| Outil | Effet |
|---|---|
| `Blend between them` | Des formes intermédiaires entre deux formes pleines. Contour et couleur passent en douceur |
| `Offset path` | Une copie agrandie ou rétrécie, en tracé. Distance négative : vers l'intérieur |
| `Outline stroke` | Le contour devient une forme pleine |
| `Deform` | Déforme la sélection dans une enveloppe : `Arc`, `Arch`, `Bulge`, `Flag`, `Wave`, `Fish`, `Rise`, `Inflate`, `Twist`. La forme devient un tracé |
| `Stick` | Avec une grille de perspective : pose la forme `On the left wall`, `On the right wall` ou `On the floor` |
| `Recolor` | Change une couleur dans toute la sélection |

## Texte

| Réglage | Effet |
|---|---|
| Police, taille | |
| `Spacing`, `Leading` | Espace entre lettres, interligne |
| `Text box (wraps)` | Une zone avec retour à la ligne, de `Width` réglable |
| Texte sur un tracé | Les lettres suivent la forme choisie |
| `Outline text` | Le texte devient des tracés modifiables point par point |
| Lisibilité (WCAG) | Indique le contraste avec le fond : 4,5:1 pour un texte courant, 3:1 pour un grand titre |

## Couleurs partagées

### Swatches (nuances)

Une nuance globale est une couleur nommée. La changer change toutes les formes qui l'utilisent. `Add the document colors as swatches` les crée d'un coup.

### Variables

Une variable est une couleur nommée avec **une valeur par mode** (clair, sombre). Une forme liée à une variable change quand son plan de travail change de mode.

- `New color variable` ;
- `Add a mode` : une colonne de valeurs en plus ;
- sur une forme : lier le remplissage à une variable.

### Styles

`New style from this shape` enregistre couleurs, contour, effets et police. `Update the style from this shape` change toutes les formes du style. `Detach from style` libère une forme.

## Composants

Un composant est une forme (ou un groupe) réutilisable. Ses copies sont des **instances** : modifier le composant maître change toutes ses instances.

| Action | Geste |
|---|---|
| `Create component` | `Ctrl+Alt+K` |
| `New instance` | Bouton, ou `Alt` + glisser le maître |
| `Go to main component` | Depuis une instance |
| `Detach instance` | L'instance redevient un groupe ordinaire |
| `New variant` | Un composant frère : « Nom / Variante 2 » |

Le panneau `Library` montre les composants des autres dessins du coffre, à poser dans celui-ci. `Update components from the library` les met à jour.

## Mise en page automatique

`Auto layout` (`Maj+A` sur une sélection) : les formes se suivent dans un cadre, comme dans Figma.

| Réglage | Choix |
|---|---|
| Sens | `Horizontal`, `Vertical` |
| Alignement | `Align start`, `Center`, `Align end` |
| `Gap` | Écart entre les formes |
| `Horizontal padding`, `Vertical padding` | Marges intérieures |
| `Background color`, `Corners` | Fond et coins du cadre |

### Contraintes

Quand le plan de travail change de taille, chaque forme suit une règle : `Left`, `Right`, `Left and right`, `Center`, `Scale` en largeur ; `Top`, `Bottom`, `Top and bottom`, `Center`, `Scale` en hauteur.

## Prototype

- Sur une forme : `Prototype: on click, go to this artboard`.
- `Hover state` : en présentation, la forme change au survol. Réglages : `Color`, `Opacity`, `Lifts` (une ombre apparaît).
- Bouton **Present the prototype** : les formes liées mènent à leur plan. `Échap` pour sortir, flèches pour changer de plan.

## Images et vectorisation

Une image posée sur la toile peut être vectorisée :

1. Choisis l'image.
2. `Threshold` : plus haut, les gris clairs deviennent aussi des formes.
3. `Simplify` : plus haut, moins de points et des courbes plus lisses.
4. `Vectorize the image` : les zones sombres deviennent des tracés, posés par-dessus.

## Bibliothèque d'icônes

Bouton **Lucide icon library** : plus de 2 400 icônes au trait. Cherche en anglais (heart, arrow, user). L'icône arrive en tracés modifiables.

## Importer

`Import an SVG`, ou colle un SVG avec `Ctrl+V`. Les tracés deviennent des formes.

## Commentaires

Outil `Comment` (`C`) : clic pour épingler une note. Clic sur une bulle pour la lire, la marquer `Resolved` ou la retirer.

## Mode Dev

`Copy as` : copie la forme en CSS, SwiftUI ou egui. Taille, position dans le plan, couleurs, coins, ombre, police.

## Exporter

Par plan de travail, section **Export** :

| Format | Notes |
|---|---|
| SVG | Le document est écrit en SVG. Le même SVG sert au rendu et aux aperçus |
| PNG | À la résolution choisie |
| PDF | Avec `Bleed` (fonds perdus, 3 mm d'habitude), `Resolution`, `Crop marks`, `CMYK colors (print)` |

### Animation

Les plans de même taille, à la suite, forment une animation : les formes de **même nom** glissent d'un plan à l'autre.

- Réglages : `fps`, durée `On each artboard`.
- Sorties : vidéo, `Lottie`, `Animated SVG`.
- `Animate in Motion` : chaque forme du plan devient un calque modifiable dans le [Motion](motion.md).

## Limites

- Les opérations de géométrie fine (décalage de tracé, concepteur de formes, contour à largeur variable) passent par une grille de pixels fine, puis revectorisent. Le résultat peut porter plus de points qu'un calcul exact.
- Le rendu passe par resvg : ce que resvg ne sait pas dessiner n'apparaît pas.
- Les images et les groupes ne se déforment pas avec `Deform`.
- La fusion (`Blend`) demande deux formes pleines.
- Le PDF en CMJN convertit les couleurs sans profil ICC. **Incertain :** à vérifier chez l'imprimeur.
