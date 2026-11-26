# Montage vidéo

Étape **Video**, outil `Edit`. Fichier : `.aryseq`.

Un montage assemble des vidéos, des images, des sons et des titres sur une timeline. Les médias restent à leur place : le fichier ne garde que leur chemin et ce que tu en fais.

Tout le calcul d'image passe par **ffmpeg**, avec un seul graphe de filtres pour l'aperçu, la lecture et l'export. Ce que tu vois est ce que tu exportes. Si ffmpeg manque, Aryadne propose de l'installer. `Install sound (ffplay)` ajoute la lecture du son.

## L'écran

| Panneau | Contenu |
|---|---|
| `Media (edit)` | Onglets `Media` et `Source` |
| `Monitor (edit)` | L'image à la tête de lecture |
| `Inspector (edit)` | Onglets `Clip`, `Effects`, `Advanced` |
| `Edit timeline` | Les pistes : image (V1, V2…) au-dessus, son (A1, A2…) en dessous |

## Importer des médias

- Glisse des fichiers dans le panneau Médias ou dans la timeline.
- Types acceptés : vidéos, images, sons, images à calques (`.aryimg`), compositions Motion, autres montages.
- Filtres : `All`, `Not used yet`, `Videos`, `Images`, `Sounds`. Un champ cherche par mots (« mist, drone, wide shot »).

### Choisir une partie d'un média

Survole une carte pour voir le plan. Glisse sur sa barre du bas pour choisir la partie voulue.

| Geste | Effet |
|---|---|
| `E` | `Add at the end` |
| `Maj+E` | `Insert at the playhead` |
| Clic droit | `Whole media`, `Open in the source`, `Remove from project` |

### Le moniteur Source

Double-clic sur un média l'ouvre dans l'onglet `Source`.

| Touche | Effet |
|---|---|
| `I`, `O` | Point d'entrée, point de sortie |
| `←` `→` | Image par image |
| `,` | Insérer dans la timeline (la suite recule) |
| `.` | Recouvrir (écrase ce qu'il y a dessous) |

Ou glisse l'image dans la timeline pour y poser la partie choisie.

## La timeline

### Timeline magnétique

Bouton **Magnetic timeline**. Quand elle est active :

- la piste V1 raconte l'histoire, sans trous ;
- retirer un plan resserre la suite ;
- titres, calques et sons restent accrochés à leur plan.

Désactivée, la timeline est libre, comme dans Premiere : les clips restent où tu les poses.

### Outils

| Outil | Touche | Usage |
|---|---|---|
| Sélection | `V` | Déplacer, raccourcir par les bords |
| Sélection de piste en avant | `A` | Tout ce qui suit. `Maj` : une seule piste |
| Propagation | `B` | Raccourcir sans laisser de trou |
| Déroulement | `N` | Déplacer une coupe entre deux clips |
| Étirement temporel | `R` | La longueur change la vitesse |
| Cutter | `C` | Couper à l'endroit cliqué. `Maj` : toutes les pistes |
| Déplacement | `Y` | Changer la partie du média utilisée, sans bouger le clip |
| Main | `H` | Faire défiler la timeline |
| Zoom | `Z` | Clic pour zoomer, `Alt+clic` pour dézoomer |
| Texte | `T` | Poser un titre |

### Lecture et déplacement

| Touche | Effet |
|---|---|
| `Espace` | Lire, arrêter |
| `J`, `K`, `L` | Arrière, arrêt, lecture |
| `←` `→` | Une image |
| `Maj+←`, `Maj+→`, `↑`, `↓` | Coupe précédente, suivante |
| `Origine`, `Fin` | Début, fin |
| `=`, `−` | Zoomer, dézoomer |
| `\` | Tout voir |
| `Ctrl+molette`, `Alt+molette` | Zoom de la timeline |

### Monter

| Touche | Effet |
|---|---|
| `I`, `O` | Entrée et sortie de la timeline |
| `Alt+X` | Retirer l'entrée et la sortie |
| `Ctrl+K` | Couper à la tête de lecture |
| `Q`, `W` | Rogner par propagation jusqu'à la tête de lecture : le début, la fin |
| `Suppr` | Supprimer |
| `Maj+Suppr` | Supprimer et refermer le trou |
| `;` | Supprimer (Lift) |
| `'` | Extraire : retire le passage entre l'entrée et la sortie, la suite se rapproche |
| `Ctrl+D` | Fondu enchaîné sur le clip choisi |
| `Ctrl+Maj+D` | Fondu audio |
| `Ctrl+L` | Lier ou délier l'image et le son |
| `Ctrl+A` | Tout choisir |
| `Ctrl+C`, `Ctrl+V` | Copier, coller. `Ctrl+Maj+V` : coller en insérant |
| `M` | Poser un repère |
| `S` | Aimanter aux coupes et à la tête de lecture |
| `Ctrl+Z`, `Ctrl+Maj+Z` | Annuler, rétablir |
| `Ctrl+M` | Exporter |

**Incertain.** Les touches `;` et `'` suivent les habitudes de Premiere (prélever, extraire). Leur effet exact sur un clavier français n'a pas été vérifié.

### Pistes, repères, chapitres

- `Add a track`, `Delete track (empty)`.
- `M` pose un repère.
- **Chapter** : commence un chapitre à la tête de lecture. Clic droit sur sa bande : renommer, supprimer.

## L'inspecteur

### Clip

| Section | Réglages |
|---|---|
| `Frame` | `Position X`, `Position Y`, `Scale`, `Opacity`, `Recenter` |
| `Speed` | Vitesse du clip |
| `Freeze frame` | Fige l'image de la tête de lecture pendant 2 secondes. La suite recule d'autant |
| `Split at scene cuts` | Détecte les coupes d'une vidéo déjà montée et sépare chaque plan |
| `Sound` | `Volume`, `Fade in`, `Fade out` |
| `To the next shot` | `Cut` ou `Dissolve`, avec sa durée |
| Titre | Texte, `Scale`, `Color`, contour, police |

**Animation.** `Turn on animation` pose une clé à la tête de lecture. Ensuite, chaque réglage modifié pose sa clé. Un bouton ajoute ou retire une clé. L'éditeur de courbes montre les clés du clip (position, taille, opacité).

### Étalonnage

| Réglage | Effet |
|---|---|
| `Look` | `Natural`, `Soft`, `Punchy`, `Black and white` |
| `Light`, `Contrast`, `Saturation` | Réglages d'ensemble |
| Roues `Shadows`, `Midtones`, `Highlights` | Glisser : teinter. Double-clic : remettre à zéro |
| Zone | `Whole image`, ou une forme : `Ellipse`, `Rectangle`, forme libre. `Invert` corrige autour de la forme |
| `Copy grade`, `Paste` | Sur tous les plans choisis |
| LUT | `Apply a .cube LUT` : un look, ou le profil Log d'une caméra |
| `Grade in nodes` | Un graphe de [compositing](noeuds-compositing.md) appliqué aux couleurs du clip |

**Scopes** sur le moniteur : forme d'onde, parade RVB, vectorscope, histogramme.

### Découpe

`Cutout` : ne garde qu'une partie de l'image. `Whole clip`, ou une forme. `Invert` cache la forme plutôt que de la garder. Réglages : `Center X`, `Center Y`, `Width`, hauteur, `Soften`.

### Suivi et stabilisation

| Outil | Effet |
|---|---|
| `Stabilization` | L'image tremble moins. Un léger `Zoom` cache les bords |
| `Motion tracking` | Ce clip (un titre, une image) suit un point de la vidéo de la piste du dessous. Clique ensuite le point dans le moniteur |
| Suivi de masque | `Track` : le masque suit ce qui bouge sous lui, sur toute la durée du clip. Place-le d'abord à la tête de lecture. Demande un clip vidéo |

### Effets

Panneau `Effects`, avec recherche. Glisse un effet sur un clip, ou double-clic pour l'appliquer au clip choisi.

| Catégorie | Effets |
|---|---|
| Transitions vidéo | Fondu enchaîné, fondu au noir |
| Effets vidéo | Couleur Lumetri, étalonnage en nœuds, stabilisation, suivi de mouvement |
| Transitions audio | Fondu audio |

Une transition se pose du côté du clip où on la lâche.

## Multicam

1. Choisis plusieurs prises de la même scène.
2. `Sync by sound` : elles se calent sur la première d'après leur son (clap, voix).
3. `Make a multicam` : chaque prise devient un angle.
4. Touches `1` à `9` : couper vers un angle à la tête de lecture, même pendant la lecture.

## Outils automatiques

| Outil | Effet |
|---|---|
| `Cut silences` | Retire les passages sans voix du clip choisi et de son son lié. La suite se resserre |
| `Cut to the beat` | Choisis la musique : les plans de V1 sont mis bout à bout et chaque coupe tombe sur un temps fort |
| `9:16 version` | Crée une copie verticale du montage pour Reels, TikTok et Shorts. Chaque plan remplit le cadre et le cadre suit son sujet |
| `Nest into a sub-sequence` | Les plans choisis partent dans un nouveau montage, qui revient ici comme un seul plan |
| Proxies | La lecture se sert de copies légères (540p), fabriquées en arrière-plan. L'export garde toujours les originaux |

## Sous-titres

Menu `Subtitles` :

| Commande | Effet |
|---|---|
| `Transcribe speech` | Écoute le mixage et pose une réplique par titre, en bas de l'image |
| `Import an SRT file…` | Pose des sous-titres existants |
| `Word by word (vertical formats)` | De gros sous-titres de trois mots, le mot dit en couleur vive |
| `Text-based editing…` | La transcription en lignes. Retirer une ligne coupe ce passage du montage |
| `Export as SRT` | À côté du montage |
| `Remove subtitles` | Les retire |

La transcription utilise whisper.cpp s'il est installé sur l'ordinateur. Sinon, l'API de transcription d'OpenAI, si une clé est enregistrée dans les réglages de l'IA. Sans l'un ni l'autre, elle ne marche pas.

## Modèles Motion

Une composition [Motion](motion.md#modèles) posée dans le montage montre ses réglages exposés dans l'inspecteur (`Template`). Les changer crée une variante : le modèle d'origine reste tel quel.

## Réglages de la séquence

`Sequence settings` : `Video format` (taille, cadence).

## Exporter

`Export as…` (`Ctrl+M`). Entre les points I et O s'ils sont posés.

| Préréglage | Usage |
|---|---|
| MP4 H.264 (YouTube, social) | Bonne qualité, lisible partout |
| Light MP4 (quick share) | Fichier plus petit, pour une revue ou un message |
| ProRes 422 HQ (.mov) | Pour l'étalonnage ou un autre logiciel de montage |
| WebM VP9 | Pour le web |
| Animated GIF | 15 images par seconde, 480 px de large |
| PNG images | Une image par trame : `nom_00001.png`… |
| Audio only (WAV) | Le mixage, sans l'image |

Plusieurs exports se suivent dans une file.

Autres sorties :

- image de la tête de lecture ;
- `Color script (thumbnails and colors)` : une vignette par plan avec ses couleurs dominantes, en PNG à côté du montage.

## Scripts

Un montage est du JSON lisible. Le module Python `aryadne` sait lister les clips et renommer les plans. Voir [Code et scripts](code.md#lapi-python-aryadne).

## Limites

- Tout passe par ffmpeg : sans lui, pas d'image.
- Sept effets dans le panneau. Pas de greffons.
- L'aperçu recalcule l'image à la tête de lecture. Sur de gros fichiers, active les proxies.
- La détection du sujet pour la version 9:16 repose sur ce qui bouge et ce qui ressort. Elle peut se tromper : vérifie chaque plan.
- Le son se mixe ici simplement. Pour un vrai travail du son, passe par l'[éditeur audio](son-audio.md).
