# Musique

Étape **Sound**, outil `Music`. Fichier : `.arymusic`.

Un séquenceur : des pistes d'instruments, des **régions** posées sur la grille des mesures, et dans chaque région des **notes**. Les instruments sont intégrés : rien à installer, le rendu se fait dans Aryadne.

## L'écran

| Panneau | Contenu |
|---|---|
| `Arrangement (music)` | Les pistes et leurs régions |
| `Piano roll` | Les notes de la région choisie |
| `Transport (music)` | Lecture, tonalité, grille, export |
| `Inspector (music)` | Onglets `Track` et `Ideas` |

## Les instruments

| Instrument | Caractère |
|---|---|
| `Piano` | Attaque franche, notes qui s'éteignent |
| `Electric piano` | Doux, un peu cloche |
| `Pluck` | Corde pincée : guitare, harpe |
| `Pad` | Nappe large et lente, pour le fond |
| `Bass` | Ronde, filtrée |
| `Lead` | Carré, avec vibrato |
| `Drums` | Batterie synthétisée |

Pièces de la batterie, de haut en bas dans le rouleau : `Crash`, `Open hi-hat`, `Hi-hat`, `High tom`, `Mid tom`, `Low tom`, `Clap`, `Snare`, `Kick`.

## Transport

| Élément | Touche | Effet |
|---|---|---|
| `Back to start` | `Origine` | Début |
| `Play` | `Espace` | Lire |
| `Start over at the end` | `L` | Reprendre au début à la fin |
| Tonalité | | La note de base, `Major` ou `Minor` |
| Tempo | | Battements par minute |
| `Grid` | | Le pas de la grille : noire, croche, double croche… |
| Longueur | | En `Bars` (mesures) |

## Arrangement

- Double-clic dans une piste : nouvelle région.
- Glisse une région pour la déplacer. Glisse son bord pour l'allonger.
- `Ctrl+D` duplique la région.
- `Suppr` la supprime.
- Sur chaque piste : `Mute`, `Solo`, `Delete track`.

## Rouleau de piano

Choisis une région. Le rouleau montre ses notes : le temps en largeur, la hauteur en hauteur.

| Geste | Effet |
|---|---|
| Clic | Ajouter une note |
| Glisser une note | La déplacer |
| Glisser son bord | L'allonger |
| Clic droit | La retirer |
| Cadre | Choisir plusieurs notes |
| `Ctrl+A` | Toutes les notes |
| `Ctrl+C`, `Ctrl+X`, `Ctrl+V` | Copier, couper, coller |
| `↑` `↓` | Transposer d'un demi-ton. `Maj` : d'une octave |
| `←` `→` | Décaler d'un pas de grille |
| `Ctrl+molette` | Zoom |
| `Suppr` | Supprimer |
| `Ctrl+Z`, `Ctrl+Y` | Annuler, rétablir |

## Inspecteur : Track

| Section | Réglages |
|---|---|
| `Instrument` | Choix de l'instrument |
| `Mix` | `Volume`, `Pan` |
| `Region` | `Name`, `−1 octave`, `+1 octave`, `Quantize` (cale les notes sur la grille), `Clear the region` |
| `Notes` | Pour les notes choisies : `Strength` (la force de la frappe) |

## Inspecteur : Ideas

Des points de départ calculés sur la tonalité du morceau. Ils remplissent la région choisie, ou une nouvelle région de 4 mesures à la tête de lecture.

| Idée | Choix |
|---|---|
| `Rhythms` | `Pop`, `Four on the floor`, `Trap` |
| `Chords` | Quatre suites d'accords : I V vi IV, vi IV I V, I vi IV V, ii V I I |
| Basse | Une ligne de basse qui suit les accords |
| `Melody` | `A melody idea`. Chaque clic en propose une autre |

Ces générateurs sont de simples règles, pas de l'IA. Ils donnent une base à retravailler.

## MIDI

- **Importer** : un fichier `.mid`. Ses pistes s'ajoutent au morceau. Formats 0 et 1. Seuls les notes et le tempo sont lus. Contrôleurs, paroles et le reste sont ignorés.
- **Exporter** : `MIDI (.mid)` écrit le morceau.

## Exporter

`Export…` : WAV ou MP3. Le rendu est calculé par les synthés intégrés.

Le morceau exporté se pose ensuite dans une [session audio](son-audio.md), un [montage](montage.md) ou une [composition Motion](motion.md#composition).

## Limites

- Sept instruments, sans réglage de leur son (pas de filtre, d'enveloppe ni d'effets par piste).
- Pas d'enregistrement depuis un clavier MIDI.
- Pas d'échantillons : tout est synthétisé. Pas de greffons.
- L'écoute demande ffplay (installé avec ffmpeg).
- Le fichier porte un nombre de temps par mesure, mais aucun réglage n'a été trouvé dans l'interface pour le changer. En pratique : des mesures à 4 temps.
