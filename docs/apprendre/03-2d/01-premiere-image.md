---
titre: Ta première image
niveau: débutant
duree: 15 min
prerequis: 01-demarrer/03-creer-et-enregistrer
objectifs: Créer une image à calques ; Naviguer sur la toile ; Peindre, gommer et choisir une couleur ; Exporter en PNG
---

## Ce que tu vas réussir à faire

Tu vas créer une image, peindre un croquis simple dessus, corriger tes traits et sortir un PNG.

```essayer
nouveau: aryimg
nom: croquis
```

```etape
Clique **Essaie par toi-même**. Une image vide s'ouvre dans l'étape 2D, outil **Retouch**.
verifier: fichier.ouvert = aryimg
```

```astuce
Pour créer une image toi-même : **File › New › Other… (choose the kind)**, puis **Layered image**. Tu y choisis la taille et le fond, blanc ou transparent.
```

## L'écran

| Zone | Contenu |
|---|---|
| La barre du haut | Les menus **Image ▾**, **Select ▾**, **Filters ▾**, **Layer ▾**, puis les options de l'outil en cours |
| La colonne d'outils | Les outils, chacun avec sa touche |
| La toile | Ton image |
| Le panneau **Layers** | Les calques |
| La section **History** | Les états de l'image. Clique son titre pour la déplier |

## Naviguer

| Geste | Effet |
|---|---|
| Molette | Zoom |
| `Espace` + glisser | Déplacer la vue |
| `4`, `6` | Tourner la vue de 15° |
| `5` | Remettre la vue droite |

```etape
Zoome avec la molette. Déplace la vue avec `Espace` + glisser. Tourne-la avec `6`, puis remets-la droite avec `5`.
```

Tourner la vue ne tourne pas l'image. C'est comme tourner ta feuille pour tracer plus à l'aise.

## Peindre

```etape
Appuie sur `B` pour prendre le pinceau **Brush**. Trace quelques traits sur la toile.
```

La barre du haut montre les réglages du pinceau.

| Réglage | Effet |
|---|---|
| **Size** | Le diamètre. `[` le réduit, `]` l'agrandit |
| **Hardness** | La dureté du bord : net ou flou |
| **Opacity** | L'opacité du trait |
| **Smoothing** | Le stabilisateur : le trait suit la souris avec un peu de retard, pour des lignes propres |

```etape
Agrandis le pinceau avec `]` plusieurs fois. Baisse **Hardness**. Trace un trait large et doux.
```

## Choisir une couleur

Deux pastilles : **Foreground color**, la couleur avec laquelle tu peins, et **Background color**.

```etape
Clique la pastille **Foreground color**. Choisis un bleu. Peins un trait.
```

```raccourci
`X` échange les deux couleurs. `D` remet le noir et le blanc. `I` prend la pipette **Eyedropper** : un clic sur la toile reprend sa couleur.
```

```etape
Appuie sur `I`, clique sur ton trait bleu, puis reviens au pinceau avec `B`.
```

## Gommer et annuler

```etape
Appuie sur `E` pour la gomme **Eraser**. Efface un bout de trait.
```

```etape
Annule avec `Ctrl+Z`. Rétablis avec `Ctrl+Y`.
```

La section **History**, sur le côté, liste chaque état. Clique son titre pour la déplier. Un clic sur un état y revient d'un coup.

```etape
Dans **History**, clique un état plus ancien, puis le dernier.
```

## Remplir

Le pot de peinture **Paint bucket** (`G`) remplit une zone fermée.

```etape
Avec le pinceau, trace un cercle fermé. Appuie sur `G`, choisis une autre couleur, clique dans le cercle.
```

```astuce
La couleur déborde par un petit trou du trait ? Monte **Close gaps (px)** dans les options du pot. Le remplissage ne passe plus par les ouvertures plus petites que cette taille.
```

`G` sert aussi au dégradé **Gradient**. Il va de la couleur vers le transparent.

## Voir autrement

Trois boutons de la vue t'aident à juger ton image.

- **Mirror view** : l'image en miroir, pour repérer les défauts. L'image ne change pas.
- **Before / after** : l'image à l'ouverture à gauche, l'état actuel à droite. Glisse la séparation.
- **Rulers and guides** (`Ctrl+R`) : des règles. Tire depuis une règle pour poser un repère.

```etape
Active **Mirror view**, regarde ton croquis, puis désactive-la.
```

## Enregistrer et exporter

```etape
Enregistre avec `Ctrl+S`.
```

Le fichier `.aryimg` garde tous les calques. Pour une image à envoyer, exporte.

```etape
Ouvre le menu **Image ▾**, puis **Export as PNG, JPG, WebP or PSD…**. Choisis PNG.
```

```quiz
Que fait la touche 6 sur la toile ?
- [ ] Elle tourne l'image de 15°.
- [x] Elle tourne la vue de 15°. L'image ne change pas.
- [ ] Elle choisit le sixième outil.
explication: 4 et 6 tournent la vue, 5 la remet droite. Le fichier n'est pas touché.
```

```quiz
Quelles touches changent la taille du pinceau ?
- [ ] `+` et `-`
- [x] `[` et `]`
- [ ] `←` et `→`
explication: Les crochets règlent la taille dans la retouche, le storyboard et la sculpture.
```

```defi
Dessine une pomme : un contour fermé, un aplat rouge au pot de peinture, une ombre douce avec un pinceau large et peu opaque.
indice: Baisse **Opacity** et **Hardness** pour l'ombre. Monte **Close gaps (px)** si le rouge déborde.
```

## Récapitulatif

- `B` peint, `E` gomme, `G` remplit, `I` prend une couleur. `[` et `]` règlent la taille.
- `Espace` + glisser déplace la vue. `4`, `5`, `6` la tournent.
- **Image ▾ › Export as PNG, JPG, WebP or PSD…** sort une image à partager.

Leçon suivante : [Les calques](02-calques.md)
