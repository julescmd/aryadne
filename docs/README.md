# Documentation d'Aryadne

Aryadne réunit dans une seule application les notes, les références, la 2D, la 3D, les nœuds, le motion design, le montage, le son et le code. Tout vit dans un dossier ordinaire de ton disque : le coffre.

Cette documentation a deux parties.

- **[Guide d'utilisation](utilisateur/)** : pour l'artiste. Un fichier par mode et par grand outil. Aucune connaissance en programmation n'est demandée.
- **[Partie technique](technique/)** : pour comprendre et modifier le code. Architecture, formats de fichiers, moteur 3D, système de nœuds, simulation sur carte graphique.

## Comment lire ces pages

- L'interface est décrite **en anglais**, telle qu'elle s'affiche. Le français suit entre parenthèses quand il aide. Exemple : **Loop cut** (coupe en boucle).
- Dans le code, le texte source est en français. La table `assets/i18n/en.json` donne la traduction. Voir [Traductions](technique/traductions.md).
- Les raccourcis sont écrits comme sur un clavier Windows ou Linux : `Ctrl+Maj+P`. `Maj` est la touche Shift.
- Un encadré **En cours de refonte** signale une partie que le code est en train de changer.
- Un encadré **Limites** dit ce que l'outil ne fait pas, ou fait à moitié.

## Ce que valent ces pages

Tout a été écrit en lisant le code source (dossier `src/`), pas en essayant chaque bouton à l'écran. Les libellés, les raccourcis et les réglages viennent du code. Le comportement fin d'un outil (sensation d'un pinceau, vitesse réelle) n'a pas été vérifié à la main. Quand un point est incertain, la page le dit.

## Guide d'utilisation

### Pour commencer

| Page | Contenu |
|---|---|
| [Prise en main](utilisateur/prise-en-main.md) | La fenêtre, les espaces de travail, créer un fichier |
| [Apprendre](utilisateur/apprendre.md) | Les leçons pas à pas dans le logiciel : parcours, étapes vérifiées, quiz |
| [Coffre et fichiers](utilisateur/coffre-et-fichiers.md) | Le coffre, l'explorateur, les liens, l'historique, la sauvegarde |
| [Notes et liens](utilisateur/notes.md) | Markdown, liens entre notes, tâches, cerveau, recherche |
| [Fenêtres et dispositions](utilisateur/fenetres-et-dispositions.md) | Panneaux, fenêtres détachables, dispositions par étape |
| [Palette de commandes](utilisateur/palette-de-commandes.md) | Tout lancer au clavier |
| [Raccourcis](utilisateur/raccourcis.md) | La liste complète, par éditeur |
| [Réglages](utilisateur/reglages.md) | Chaque réglage de l'application |
| [Assistant IA](utilisateur/assistant-ia.md) | L'assistant, le chat, les fournisseurs |

### 2D

| Page | Contenu |
|---|---|
| [Retouche à calques](utilisateur/2d-retouche.md) | Images `.aryimg`, calques, masques, pinceaux, animation image par image |
| [Dessin vectoriel](utilisateur/2d-vectoriel.md) | Fichiers `.aryvec`, formes, plume, composants, prototypes |
| [Board et moodboard](utilisateur/2d-board.md) | Toile infinie, références, palettes |
| [Storyboard](utilisateur/storyboard.md) | Cases, animatique, envoi au montage |

### 3D

| Page | Contenu |
|---|---|
| [Scène 3D](utilisateur/3d-scene.md) | Navigation, objets, lumières, caméras, import et export |
| [Modélisation](utilisateur/3d-modelisation.md) | Mode édition et pile de modificateurs |
| [Sculpture](utilisateur/3d-sculpture.md) | Pinceaux, multirésolution, remaillage |
| [Retopologie](utilisateur/3d-retopologie.md) | Quads automatiques, guides, densité |
| [UV et UDIM](utilisateur/3d-uv.md) | IDs de matériau, coutures, dépliage, cuisson des textures |
| [Peinture 3D](utilisateur/3d-peinture.md) | Peindre sur le modèle, calques de peinture, décalques |
| [Atelier : matériaux à calques](utilisateur/3d-atelier.md) | Pile de calques, masques, matériaux intelligents |
| [Rendu](utilisateur/3d-rendu.md) | Aperçu, tracé de chemins, rendu final, passes EXR |
| [Animation](utilisateur/3d-animation.md) | Clés, courbes, outils, NLA, calques d'animation, formes |
| [Rig, mocap et suivi de caméra](utilisateur/3d-rig-mocap.md) | Os, rig bipède, BVH, matchmove |

### Nœuds et simulation

| Page | Contenu |
|---|---|
| [L'éditeur de nœuds](utilisateur/noeuds-editeur.md) | Gestes communs, groupes, zones, champs, expressions |
| [Matériaux en nœuds](utilisateur/noeuds-materiaux.md) | Fichiers `.arymat`, liste des nœuds |
| [Compositing](utilisateur/noeuds-compositing.md) | Fichiers `.arycomp`, filtres, incrustation |
| [Géométrie et simulation](utilisateur/noeuds-geometrie.md) | Fichiers `.arygeo`, points, attributs, faces et UV, zone de simulation, pose dans une scène 3D |
| [Simulation de tissu](utilisateur/simulation-tissu.md) | XPBD, VBD, carte graphique, cache |
| [Banc de mesure de la carte graphique](utilisateur/banc-gpu.md) | Ce que mesure le banc et comment le lire |

### Mouvement, vidéo, son, code

| Page | Contenu |
|---|---|
| [Motion design](utilisateur/motion.md) | Compositions `.arymotion`, calques, clés, expressions |
| [Montage vidéo](utilisateur/montage.md) | Montages `.aryseq`, outils, étalonnage, export |
| [Revue vidéo](utilisateur/revue-video.md) | Lecture image par image, dessin, retours |
| [Audio](utilisateur/son-audio.md) | Sessions `.aryaudio`, effets, vue spectrale |
| [Musique](utilisateur/son-musique.md) | Séquenceur `.arymusic`, rouleau de piano, MIDI |
| [Code et scripts Python](utilisateur/code.md) | Éditeur de code, exécution, API `aryadne.py` |

## Partie technique

| Page | Contenu |
|---|---|
| [Architecture](technique/architecture.md) | Les modules de `src/`, le trajet d'une image, egui et eframe |
| [Formats de fichiers](technique/formats.md) | Chaque format `.ary…`, le dossier `.app`, les réglages |
| [Moteur d'affichage 3D](technique/moteur-3d.md) | Pixols, buffer de visibilité, caches d'ombres, tracé de chemins |
| [Système de nœuds](technique/noeuds.md) | Mise à plat, évaluation incrémentale, colonnes, `Solver` |
| [Simulation sur carte graphique](technique/simulation-gpu.md) | `gpusim`, `gpucloth`, `gpubench` |
| [Traductions](technique/traductions.md) | `i18n`, `t` et `t_mid`, la table anglaise |
| [Mode Apprendre](technique/apprendre.md) | Leçons embarquées, vérifications des étapes, progression |
| [Ajouter un nœud, un format, un mode, une icône](technique/ajouter.md) | Les étapes, fichier par fichier |
| [Feuille de route du moteur](technique/feuille-de-route-moteur.md) | Résumé des rapports de recherche |

## Parties en cours de refonte

Au moment où ces pages sont écrites, un chantier change le code. Les pages concernées décrivent le code actuel et portent un encadré.

- **Tissu sur carte graphique.** Le solveur est en cours d'optimisation. Pages : [Simulation de tissu](utilisateur/simulation-tissu.md), [Simulation sur carte graphique](technique/simulation-gpu.md).
