---
titre: Mode Dev et export
niveau: avancé
duree: 15 min
prerequis: 04-vectoriel/06-prototype-et-presentation
objectifs: Copier une forme en CSS, SwiftUI ou egui ; Exporter un plan en SVG, PNG ou PDF ; Animer une suite de plans et l'exporter en GIF ou MP4 ; Envoyer un plan dans Motion
---

## Ce que tu vas réussir à faire

Tu vas sortir ton dessin de quatre façons : du code prêt à coller, une image, un PDF pour l'imprimeur, et une petite animation en GIF.

```essayer
nouveau: aryvec
nom: export
```

```etape
Clique **Try it yourself**. Choisis le plan de travail, ou trace-en un avec `F`. Clique **Size…** et prends le format Square. Dessine dessus un bouton : un rectangle arrondi coloré, avec une ombre portée, et un texte.
verifier: calques >= 2
```

## Le mode Dev : copier le code

Le mode Dev traduit une forme en code. Il reprend la taille, la position dans le plan, les couleurs, les coins, l'ombre et la police.

```etape
Choisis le rectangle du bouton. En haut du panneau des propriétés, à côté du nom de la forme, clique l'icône de code `</>`.
```

Le menu propose trois cibles :

| Commande | Pour |
|---|---|
| **Copy as** CSS | Une page web |
| **Copy as** SwiftUI | Une application iPhone, iPad ou Mac |
| **Copy as** egui | Une application Rust avec egui |

```etape
Choisis **Copy as** SwiftUI. Le code est dans le presse-papiers.
```

```etape
Crée une note avec `Ctrl+N` et colle avec `Ctrl+V`. Lis le code : tu y retrouves la taille, la couleur et le rayon des coins de ton bouton.
verifier: fichier.ouvert = md
```

```astuce
Le mode Dev copie une forme à la fois. Pour un écran entier, copie chaque élément, ou donne le SVG exporté à ton développeur.
```

```etape
Reviens à ton dessin avec `Ctrl+Tab`, ou par son onglet dans l'étape 2D.
```

## Exporter une image

L'export se fait par plan de travail. Choisis un plan : la section **Export** apparaît dans ses propriétés.

| Format | Notes |
|---|---|
| SVG | Vectoriel. Le même SVG sert au rendu à l'écran : ce que tu vois est ce qui est exporté |
| PNG | En pixels. Les boutons `1×`, `2×` et suivants choisissent la résolution |
| PDF | Pour l'impression |

```etape
Choisis le plan de travail. Dans **Export**, choisis `2×`, puis clique **PNG**.
```

```etape
Clique **SVG** pour exporter la version vectorielle.
```

## Un PDF pour l'imprimeur

Le bouton **PDF** ouvre ses réglages.

| Réglage | Effet |
|---|---|
| **Bleed** | Les fonds perdus : le dessin dépasse du format coupé. 3 mm d'habitude |
| **Resolution** | En points par pouce |
| **Crop marks** | Les traits de coupe |
| **CMYK colors (print)** | Convertit les couleurs en CMJN |

```attention
Le PDF en CMJN convertit les couleurs sans profil ICC. Demande une épreuve à ton imprimeur avant un gros tirage.
```

## Animer une suite de plans

Des plans de même taille, posés à la suite, forment une animation. Les formes de **même nom** glissent d'un plan à l'autre.

```etape
Choisis ton plan de travail. Clique **Adapt…** et choisis Square, le format du plan. Une copie du plan, avec ses formes, se pose à côté. Refais-le pour avoir trois plans de même taille.
```

Il n'y a pas de commande « dupliquer le plan ». **Adapt…** vers le même format en tient lieu.

```etape
Sur le deuxième plan, déplace le bouton et change sa couleur. Sur le troisième, agrandis-le. Ne change pas les noms des formes.
```

```etape
Choisis le premier plan. Dans **Export**, ouvre le menu **Animation**.
```

| Réglage | Effet |
|---|---|
| Les images par seconde | La fluidité |
| **On each artboard** | Le temps passé sur chaque plan, en secondes |

| Sortie | Usage |
|---|---|
| **GIF** | Une boucle légère, lisible partout |
| **MP4** | Une vidéo |
| **Lottie** | Une animation vectorielle en JSON, pour le web et les applications |
| **Animated SVG** | Lisible par un navigateur |

```etape
Règle **On each artboard** sur 1 seconde. Clique **GIF**.
```

```attention
Les exports GIF et MP4 passent par ffmpeg. S'il manque, Aryadne propose de l'installer.
```

```astuce
Une forme ne glisse pas d'un plan à l'autre ? Vérifie son nom. Elle doit porter exactement le même nom sur les deux plans.
```

## Continuer dans Motion

L'icône de film, **Animate in Motion**, change le plan de travail en composition Motion. Chaque forme devient un calque que tu animes par clés.

```etape
Choisis un plan. Clique **Animate in Motion**. Une composition s'ouvre dans l'étape Motion.
verifier: onglet = motion
```

Tu apprendras à l'animer dans le parcours [Storyboard et motion design](../05-storyboard-motion/index.md).

## Vectoriser une image

Le chemin inverse existe : une image posée sur la toile peut devenir des tracés.

1. Choisis l'image.
2. Règle **Threshold** : plus haut, les gris clairs deviennent aussi des formes.
3. Règle **Simplify** : plus haut, moins de points et des courbes plus lisses.
4. Clique **Vectorize the image**. Les zones sombres deviennent des tracés, posés par-dessus.

## Limites à connaître

- Le rendu passe par un moteur SVG. Ce qu'il ne sait pas dessiner n'apparaît pas, à l'écran comme à l'export.
- Le décalage de tracé, le concepteur de formes et le contour à largeur variable peuvent produire plus de points qu'un calcul exact.

```quiz
Que copie le mode Dev ?
- [x] Le code d'une forme : taille, position, couleurs, coins, ombre, police.
- [ ] Une image de la forme.
- [ ] Tout le fichier en JSON.
explication: Copy as traduit la forme choisie en CSS, SwiftUI ou egui.
```

```quiz
Dans l'animation de plans, qu'est-ce qui relie une forme d'un plan à celle du plan suivant ?
- [ ] Sa couleur.
- [x] Son nom.
- [ ] Sa position dans la pile.
explication: Les formes de même nom glissent d'un plan à l'autre. Les autres apparaissent ou disparaissent.
```

```quiz
Quel format garde un dessin net à toutes les tailles ?
- [ ] PNG
- [x] SVG
- [ ] GIF
explication: Le SVG est vectoriel. Le PNG et le GIF sont en pixels.
```

```defi
Anime un logo en trois plans : il apparaît petit, grandit, puis change de couleur. Exporte en MP4 et en Lottie.
indice: Garde le même nom de forme sur les trois plans. Baisse **On each artboard** pour une animation plus vive.
```

## Récapitulatif

- L'icône `</>` copie une forme en CSS, SwiftUI ou egui.
- La section **Export** d'un plan sort du SVG, du PNG et du PDF.
- Des plans de même taille à la suite s'animent : **GIF**, **MP4**, **Lottie**, **Animated SVG**.

Parcours suivant : [Storyboard et motion design](../05-storyboard-motion/index.md)
