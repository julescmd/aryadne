# Rendu 3D

Aryadne a deux moteurs pour la vue, et un rendu final.

| Moteur | Libellé | À quoi il sert |
|---|---|---|
| Aperçu | `Preview (fast)` | Travailler. Instantané, ombres et reflets approchés. Comparable à Workbench ou Eevee |
| Tracé de chemins | `Path tracing (realistic)` | Juger l'image. La lumière rebondit, l'image s'affine quand la vue ne bouge plus. Comparable à Cycles |

Les deux lisent les mêmes matériaux en nœuds, les mêmes lumières et la même peinture.

## Choisir le moteur

Menu `Render ▾` de la barre de la vue, section **Viewport engine**. Le bouton du menu prend la couleur d'accent quand le tracé de chemins est actif.

## L'aperçu rapide

Réglé dans l'onglet `World` (ou les menus `Lighting` et `Overlays`). Voir [Scène 3D](3d-scene.md#affichage-et-éclairage-de-la-vue).

Ce qu'il fait bien : rester fluide, montrer la forme, l'éclairage général, les matériaux.

Ce qu'il approche : les ombres (rayons lancés depuis chaque pixel, avec une pénombre réglable), les reflets (lus dans l'environnement, pas dans les autres objets).

Il est calculé sur le processeur. Voir [Moteur d'affichage 3D](../technique/moteur-3d.md).

## Le tracé de chemins

Chaque passe ajoute un échantillon par pixel. L'image part bruitée et s'affine. Elle gère : rebonds de lumière, ombres douces, reflets brillants ou mats, métaux, émission, transmission (verre), ciel HDRI, lumières de la scène, profondeur de champ.

### Réglages

| Réglage | Plage | Effet |
|---|---|---|
| `Render lighting` | `View`, `Studio`, `Outdoor` | Sert quand la scène n'a pas ses propres lumières. `View` : la lumière de la vue. `Studio` : clé, contre-jour et débouchage sur un fond gris, idéal pour présenter un modèle. `Outdoor` : un soleil franc et le ciel de l'environnement |
| `Studio backdrop (the floor fades into the distance)` | | Le sol se fond dans le fond : plus de ligne d'horizon |
| `Backdrop brightness` | | Clarté du fond gris du studio |
| `Contrast` | | 0 : rendu de base. 1 : image plus tranchée |
| `Saturation` | | 1 : inchangée |
| `Outlines (line art)` | pixels | Trace des contours autour des formes. 0 : aucun |
| `Outline color` | | Couleur des contours |
| `Bounces` | 1 à 16 | Nombre de rebonds de la lumière. Plus haut : intérieurs mieux éclairés, plus lent |
| `Samples` | 16 à 4096 | Le rendu de la vue s'arrête à ce nombre de passes |
| `Floor (shadows and bounces)` | | Un sol infini sous le modèle. Il reçoit les ombres et renvoie la lumière |
| `Visible floor` | | Décoché : le sol ne montre que les ombres, sur le fond de la vue |
| `Firefly clamp` | 1 à 100 | Plafond d'un échantillon. Évite les points blancs isolés. Plus bas : image plus propre, reflets vifs un peu éteints |
| `Denoise` | | Débruite l'image affichée |
| `Path tracing while moving` | | Décoché : l'aperçu rapide prend le relais quand on tourne autour de la scène |
| `Depth of field` | 0 à 0,2 | Rayon de l'ouverture. 0 : tout net |
| `Focus distance` | 0,05 à 10 | Distance de mise au point. Par défaut : le pivot de la vue |
| `Colors` | `AgX` ou `Filmic` | La conversion des couleurs pour l'affichage. AgX est celle de Blender |
| `Graphics card` | | Calcule sur la carte graphique, si elle est là. Le nom de la carte est affiché |

### Carte graphique

Avec une carte utilisable, le tracé de chemins passe par elle : même rendu, bien plus vite. Sans carte, le message « No usable graphics card: rendering on the CPU » s'affiche et le calcul se fait sur le processeur.

### Débruitage

Deux débruiteurs :

- le débruiteur maison, toujours disponible ;
- **Intel Open Image Denoise**, le débruiteur de Cycles, Arnold et V-Ray. Bouton `Install the AI denoiser (~65 MB)`. Il est téléchargé une fois dans le dossier de l'utilisateur, sans droits administrateur. Une fois installé, il prend le relais.

## Rendu final

`F12`, ou `Render ▾ › Render image…`.

1. Choisis la taille : `720p`, `1080p`, `4K`, portrait (1080 × 1350), carré (2048 × 2048), ou des valeurs libres.
2. Règle `Samples` (8 à 4096).
3. Coche `Denoise` si voulu.
4. `Render`.

Le point de vue est celui de la vue 3D. Pour rendre depuis une caméra, regarde d'abord par elle (`0`). Rebonds et sol sont ceux du menu `Render`.

Pendant le calcul : `Stop` arrête. À la fin :

| Bouton | Effet |
|---|---|
| `Save…` | Enregistre en PNG |
| `Multilayer EXR (passes)` | Enregistre les passes dans un EXR multicouche. Il faut attendre la fin du rendu |
| Envoyer | Envoie l'image vers un autre outil (retouche, board, montage) |
| `Close` | Ferme |

## Rendu d'une animation

`Capture › Animation as video (MP4)…` : chaque image de la scène, du début à la fin, vue par sa caméra. S'il n'y a pas de caméra, c'est la vue actuelle. ffmpeg assemble la vidéo.

Une géométrie de nodes posée dans la scène (un tissu simulé) est rendue à chaque image, à l'image de sa simulation, avec son matériau. Elle sort aussi dans le rendu final (`F12`).

**Incertain.** Le code de capture d'animation passe par le moteur de la vue. La qualité (aperçu ou tracé de chemins) et le temps par image n'ont pas été vérifiés à l'écran.

## Matériaux prêts à l'emploi

Sans ouvrir les nœuds, `Nodes ▾ › New material…` propose des préréglages (néon, verre, toon 3 bandes, métal peint usé, pierre aux creux sombres…). Voir [Matériaux en nœuds](noeuds-materiaux.md).

## Conseils

- Pour présenter un modèle : éclairage `Studio`, `Studio backdrop` coché, `Visible floor` coché, 256 échantillons, débruitage.
- Image trop bruitée : monte `Samples`, ou active le débruiteur IA.
- Points blancs isolés : baisse `Firefly clamp`.
- Intérieur trop sombre : monte `Bounces`.

## Limites

- Pas de volumes (brouillard, fumée) dans le rendu.
- Pas de flou de mouvement.
- Pas de rendu par tuiles ni de file d'attente : un rendu à la fois.
- Les caustiques (lumière concentrée par le verre) convergent très lentement, comme dans tout tracé de chemins simple.
- La feuille de route prévoit un moteur temps réel sur carte graphique pour la vue. Il n'existe pas encore : l'aperçu reste sur le processeur.
