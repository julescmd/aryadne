# Storyboard

Étape **Video**, outil `Storyboard`. Fichier : `.arystory`.

Un storyboard est une suite de **cases** dessinées. Chaque case porte sa valeur de plan, son mouvement de caméra, son action, son dialogue et sa durée. L'ensemble se lit comme une **animatique** et part au montage.

## Créer

`File › New › Storyboard`. Choisis le format de l'image.

Chaque case est une image PNG. Elles sont rangées dans un dossier « <nom> - cases » à côté du fichier.

## L'écran

| Zone | Contenu |
|---|---|
| Au centre | La case dessinée |
| À droite | Les informations de la case |
| En bas | La bande des cases |

## Dessiner

| Outil | Touche | Usage |
|---|---|---|
| `Pencil` | `P` | Le crayon d'esquisse |
| `Ink` | `I` | Le trait net |
| `Tone, light gray` | `T` | Un gris léger pour les ombres |
| `Fill` | `F` | Entoure une zone pour la remplir |
| `Eraser` | `E` | Gomme |
| Pot de peinture | `G` | Remplit une zone fermée |

| Geste | Effet |
|---|---|
| `[` et `]` | Taille du trait |
| `O` | Pelure d'oignon : la case précédente en transparence |
| `Ctrl+Z`, `Ctrl+Y` | Annuler, rétablir |

### Mise en couleur

Le bouton **Color from the previous panel** : chaque zone fermée du trait reprend la couleur de la même zone sur la case d'avant.

- Colorie d'abord la première case à la main.
- Si rien n'est repris : la case d'avant n'a pas de couleur dans ces zones, ou le trait n'est pas fermé.

## Les cases

| Geste | Raccourci |
|---|---|
| Case précédente, suivante | `←` `→` |
| `New panel after` | `N` |
| `Duplicate panel` | `Ctrl+D` |
| `Move earlier`, `Move later` | `Alt+←`, `Alt+→` |
| `Delete panel` | Bouton. Son dessin reste sur le disque |

## Les informations d'une case

| Champ | Contenu |
|---|---|
| `Scene` | La séquence ou la scène : « 1 », « INT. CUISINE – JOUR ». Les cases d'une même scène se suivent |
| Valeur de plan | Plan d'ensemble, plan large, plan moyen, plan américain, plan rapproché, gros plan, très gros plan, plongée, contre-plongée |
| `Camera` | Fixe, panoramique, travelling avant, arrière ou latéral, zoom, caméra à l'épaule, grue |
| `Duration` | Durée dans l'animatique, en secondes. Le total et l'instant de la case s'affichent |
| `Action` | Ce qui se passe à l'image |
| `Dialogue` | Répliques, voix off |
| `Notes` | Son, effets, transitions |

## Caméra 2D

`2D camera` : un pano, un zoom ou un travelling **sur le dessin**. Le cadre passe d'un cadre A à un cadre B pendant la durée de la case.

1. Active `2D camera` sur la case.
2. Règle les deux cadres sur le dessin : glisser pour déplacer, molette pour zoomer.
3. Le mouvement se voit à la lecture, dans le MP4 et au montage.

## L'animatique

`Espace`, ou le bouton **Play the animatic** : les cases se suivent, chacune à sa durée, avec ses mouvements de caméra 2D.

## Exporter

| Commande | Résultat |
|---|---|
| `Animatic as MP4…` | Une vidéo de l'animatique. Demande ffmpeg |
| `Contact sheet as PNG…` | La planche : toutes les cases sur une image |
| `Send to edit` | Crée un montage avec une image par case, à sa durée, et un repère à chaque scène |

`Send to edit` est le passage normal vers l'étape suivante : le montage créé s'ouvre dans l'outil [Montage](montage.md), et tu remplaces peu à peu les cases par les plans.

## Depuis un scénario

Un scénario `.fountain` donne les scènes, les personnages et un minutage estimé. Voir [Notes](notes.md#scénario-fountain). Le lien entre un scénario et un storyboard n'est pas automatique : recopie les intitulés de scène dans le champ `Scene`.

## Limites

- Un seul calque de dessin par case. Pour un dessin à calques, passe par la [retouche](2d-retouche.md).
- Les valeurs de plan et les mouvements de caméra sont des listes fixes. Plusieurs de ces libellés ne sont pas traduits en anglais.
- Pas de son dans l'animatique. Ajoute-le au montage.
- La caméra 2D va d'un cadre A à un cadre B, en ligne droite. Pas de trajectoire à plusieurs points.
