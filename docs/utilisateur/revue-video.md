# Revue vidéo

Étape **Video**, outil `Review`. Formats : MP4, MOV, WebM, MKV, AVI.

Regarder un plan image par image, dessiner dessus et le commenter, dans l'esprit de SyncSketch. C'est aussi le lecteur qui s'ouvre quand tu ouvres une vidéo.

ffmpeg est nécessaire. S'il manque : « Videos need ffmpeg », avec le bouton `Install ffmpeg`.

## Lire

| Touche | Effet |
|---|---|
| `Espace` | Lire, mettre en pause |
| `←` `→` | Image précédente, suivante |
| `Maj+←`, `Maj+→` | Une seconde. Si des images sont dessinées : image dessinée précédente, suivante |
| `I`, `O` | Début et fin de la boucle |
| `M` | Signet sur cette image |
| `Alt+←`, `Alt+→` | Signet précédent, suivant |

### Cadence

`Frame rate` : 12, 24, 30, ou n'importe quelle valeur (glisse ou double-clique pour taper).

### Lecture « en 1, en 2, en 4, en 8 »

Comme en animation : chaque image est tenue N fois. Lire un plan à 24 i/s « en 2 » montre 12 dessins par seconde.

### Boucle

`Loop start here (I)`, `Loop end here (O)`, `Remove the loop`. Les crochets de boucle se glissent sur la timeline. Clic droit pour les enlever. `Back and forth (ping-pong)` lit en aller-retour.

## Zoom

| Geste | Effet |
|---|---|
| Molette | Zoom vers le curseur |
| Glisser | Déplacer |
| Double-clic | 100 % ou ajuster |
| `F` | Ajuster |

## Pelures d'oignon

`Onion skin` : les images voisines en transparence.

| Réglage | Effet |
|---|---|
| `Frames before`, `Frames after` | Jusqu'à 6 de chaque côté |
| `Opacity` | Dégressive |
| `Tint (before red, after green)` | Ou couleurs naturelles |
| `Include drawings` | Montre aussi les dessins des images voisines |

L'écart entre les pelures suit le réglage « en 1 / 2 / 4 ».

## Repères d'image

Menu `Guides ▾` : `Rule of thirds`, `Center`, `Safe zones (action 93%, titles 90%)`, `Format mask` (2.39, 1.85, 16:9, 9:16…).

## Dessiner sur l'image

Active `Draw`. Les dessins sont posés **image par image**.

| Outil | Touche |
|---|---|
| `Brush` | `B` |
| `Eraser` | `E` |
| `Line`, `Rectangle`, `Ellipse`, `Arrow` | Boutons |

Menu `Stroke ▾` : `Thickness`, `Opacity`, `Stabilizer`, `Taper`. `Clear the image` efface le dessin de l'image courante. `Ctrl+Z`, `Ctrl+Maj+Z` : annuler, rétablir.

Sur la timeline, les images dessinées sont marquées. La tête de lecture s'y aimante (`Alt` pour l'empêcher).

Les dessins sont gardés dans `.app/annotations/` du coffre. La vidéo n'est pas modifiée.

## Bande d'images

`Filmstrip` : toutes les images sous la vidéo. Clic pour y aller. Les dessins et la boucle y sont repérés.

- `Scale` : taille des vignettes.
- `Follow playback`.
- `Skipped frames` : en 2, en 3… montre aussi les images qui ne sont pas lues, à 50 %.

## Signets

`Bookmark this frame (M)`. Chaque signet peut porter un nom. `Go to this frame`, `Remove bookmark`.

## Retours

Panneau `Feedback (review)`.

1. Va sur une image.
2. Écris dans `Comment on frame`.
3. `Ctrl+Entrée`, ou `Add`.

Chaque retour garde son image. Un clic y ramène. Coche `Resolved` quand c'est réglé. `To do only` cache les retours réglés.

`Export as a note` : une note Markdown à côté de la vidéo, avec une case à cocher par retour. Elle apparaît ensuite dans les [tâches](notes.md#tâches).

## Comparer deux versions

`Compare with another video…` : deux versions du même plan, synchronisées.

| Mode | Effet |
|---|---|
| `Side by side` | Les deux côte à côte |
| `Wipe (drag the line)` | Un volet : glisse la ligne |
| `Stop comparing` | Revient à une seule vidéo |

## Exporter

`Export…` :

| Sortie | Réglages |
|---|---|
| Séquence d'images PNG ou JPG | `Frame rate`, une image sur N, `Height`, `With drawings` |
| MP4 retimé | Par exemple 24 i/s « en 2 » |

`Open the folder` montre le résultat.

## Limites

- Pas de son pendant la lecture image par image de la revue. **Incertain :** à vérifier à l'écran.
- Les dessins sont liés au numéro d'image. Si la vidéo est remplacée par une version de durée différente, ils se décalent.
- La revue est locale. Pour partager des retours, exporte la note ou la séquence avec les dessins.
