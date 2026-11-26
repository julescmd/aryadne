# Audio

Étape **Sound**, outil `Audio`. Fichier : `.aryaudio`.

Un éditeur audio multipiste : voix, musiques et ambiances sur plusieurs pistes. Couper, nettoyer, mixer, exporter.

**Rien n'est détruit.** Un clip renvoie à un son du coffre. Il garde son début, la partie du son utilisée, son gain, ses fondus et ses effets. Le son d'origine ne change jamais. Le mixage se fait en stéréo à 48 kHz. Les effets sont rendus par ffmpeg.

## L'écran

| Panneau | Contenu |
|---|---|
| `Sounds (audio)` | Les sons du coffre, et les repères |
| `Tracks (audio)` | Les pistes et leurs clips |
| `Transport (audio)` | Lecture, enregistrement, outils, métronome |
| `Inspector (audio)` | Onglets `Effects`, `Clip`, `Spectrum` |
| `Status (audio)` | Sélection, durée, fréquence du projet |

## Poser des sons

- Clique un son dans la liste : il est posé sur la piste active, à la tête de lecture.
- `Import sounds…` (le `+`) en ajoute au coffre.
- Glisse un clip pour le déplacer. Glisse ses bords pour le raccourcir.

## Transport

| Bouton | Touche | Effet |
|---|---|---|
| `Back to start` | `Origine` | Début |
| `Play` | `Espace` | Lire |
| `Stop` | | Arrêter |
| `Loop the range` | `L` | Boucle sur la plage choisie |
| Enregistrer | `R` | Enregistre au micro sur la piste choisie. `R` de nouveau arrête la prise |
| `Select` | `V` | Outil de sélection |
| Rasoir | `B` | Couper un clip au clic |
| | `S` | Couper à la tête de lecture |
| | `M` | Poser un repère |
| | `Ctrl+D` | Dupliquer le clip |
| | `Suppr` | Supprimer et resserrer la piste |
| | `Ctrl+Z`, `Ctrl+Y` | Annuler, rétablir |

Glisser sur une piste, hors d'un clip, choisit une **plage** de temps.

## Tempo et grille

| Réglage | Effet |
|---|---|
| Métronome | Un clic à chaque temps pendant l'écoute. Jamais dans l'export |
| `Tempo` | Battements par minute |
| `Bar grid` | Une règle en mesures et en temps sur la session |
| `Snap clips to beats` | Aimante les clips aux temps |
| `Tempo changes` | `Change at the playhead` : le nouveau tempo commence une mesure à cet instant |

## Pistes

| Élément | Effet |
|---|---|
| `Mute` | Coupe la piste |
| `Solo` | N'écoute que cette piste |
| `Volume` | Volume de la piste |
| Automation du volume | Clic sur la ligne pour un point. Glisser pour le régler. Double-clic pour le retirer |
| `Duck under` | La piste baisse toute seule quand une autre joue : la musique sous la voix. Réglages : de combien (`By`), temps de remontée (`Release`) |
| `Delete track` | Supprime la piste |

## Effets

Onglet `Effects`. Ils s'appliquent `On the clip` ou `On the range`.

### Volume

| Effet | Ce qu'il fait |
|---|---|
| `Normalize` | Monte le clip jusqu'à une crête à −1 dB |
| Normaliser la sonie | Chaque clip choisi atteint une cible de sonie, crête sous −1 dB. Cibles : −14 LUFS (YouTube, Spotify), −16 (Apple Music, podcasts), −23 (télé, EBU R128), −19 (podcast en mono) |
| `Measure: true peak, correlation` | Crête vraie (dBTP, à garder sous −1 pour les plateformes) et corrélation stéréo (proche de 1 : passe bien en mono) |
| `Fade in and out` | Une demi-seconde de chaque côté |

Si la crête empêche d'atteindre la cible de sonie : « limited by the peak ».

### Clean up (nettoyer)

| Effet | Ce qu'il fait |
|---|---|
| `Clear voice` | Coupe-bas, bruit, présence, compression : tout en un |
| `Noise reduction` | Souffle et bourdonnement |
| `Low cut` | Retire les graves sous 100 Hz |
| `Cut silence` | Retire les blancs et resserre la piste |

### Tone (couleur du son)

| Effet | Ce qu'il fait |
|---|---|
| `Equalizer` | Plus de clarté : +3 dB d'aigus, −2 dB de graves |
| `Warm equalizer` | +3 dB de graves, −1 dB d'aigus |
| `Compressor` | Voix parlée, niveau plus égal |
| `De-esser` | Adoucit les « s » qui sifflent |
| `Limiter` | Rien ne dépasse −1 dB |
| `Gate` | Coupe le souffle entre les phrases (sous −45 dB) |

### Space and voice

| Effet | Ce qu'il fait |
|---|---|
| `Reverb` | Une pièce. Réglable dans l'onglet Clip |
| `Deeper voice`, `Higher voice` | Change la hauteur en `Semitones`, sans changer la durée |

## Onglet Clip

`Gain`, `Fade in`, `Fade out`, `Start`, `Duration`, puis **Clip effects** : la liste des effets posés, chacun réglable (`Pitch`, `Limiter`, `Gate`, `Reverb`, `Noise reduction`…) et retirable.

### Chaînes d'effets

Règle les effets d'un clip, donne un nom, enregistre : c'est une **chaîne** réutilisable. Appliquer une chaîne remplace les effets des clips choisis.

## Onglet Spectrum et vue spectrale

L'onglet `Spectrum` montre le spectre `Live` pendant la lecture, avec la crête (`Peak`).

**Spectral view** ouvre le spectrogramme d'un clip : le temps en largeur, les fréquences en hauteur. On y voit un clic, une sonnerie, un sifflement comme une tache. On la peint pour la traiter.

| Outil | Touche | Usage |
|---|---|---|
| Pinceau | `B` | Peins la tache à traiter |
| Rectangle | `M` | Glisse pour prendre une zone de temps et de fréquences |
| Taille du pinceau | `[` `]` | |
| Réparer | | La tache est ramenée au niveau du son juste avant et juste après. Pour un clic, un bruit bref |
| Atténuer | | La tache est baissée du nombre de dB choisi. Pour une sonnerie, un sifflement. 120 dB : effacé |
| Traiter | `Entrée` | Traite ce qui est peint |
| Oublier | `Suppr` | Oublie ce qui est peint |
| Annuler | `Ctrl+Z` | Annule le dernier traitement |
| Sensibilité | | Le niveau le plus bas affiché |
| `Keep` | | Écrit le son retouché dans un **nouveau fichier** et le met dans le clip |
| Fermer | `Échap` | |

L'original reste intact.

## Repères et régions

`M` pose un repère à la tête de lecture. Chaque région va d'un repère au suivant.

`Export each region` : un WAV par région, nommé comme le repère. Pratique pour une banque de bruitages.

## Exporter

`Export…` :

| Sortie | Notes |
|---|---|
| WAV, MP3 | Le mixage complet |
| `Separate tracks (stems)` | Un WAV par piste, tous de la durée de la session. Ils se recalent tels quels dans un autre logiciel |

`Project rate` : la fréquence d'échantillonnage du projet.

## Limites

- Les effets sont des préréglages avec peu de réglages. Ce n'est pas un égaliseur paramétrique complet.
- Pas de greffons VST.
- L'écoute demande ffplay (installé avec ffmpeg).
- Les effets sont rendus par ffmpeg : un changement relance un calcul, il n'y a pas d'écoute en temps réel pendant qu'on tourne un bouton.
- Mixage en stéréo. Pas de 5.1.
