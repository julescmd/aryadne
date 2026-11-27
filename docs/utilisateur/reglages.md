# Réglages

`Ctrl+,` ou `File › Settings`. La fenêtre a sept onglets.

Les réglages sont écrits dans le dossier de configuration de l'utilisateur, pas dans le coffre : `%APPDATA%\aryadne` sous Windows, `~/.config/aryadne` sous Linux. Voir [Formats](../technique/formats.md#réglages-de-lutilisateur).

## Appearance (apparence)

| Réglage | Ce qu'il fait |
|---|---|
| Langue | `Français` ou `English` |
| `Theme` | Huit thèmes : Rose poudré, Sauge, Lagon, Argile, Graphite, Monochrome (sombres), Monochrome clair, Clair. Chaque vignette montre le fond, le panneau, la couleur principale et les couleurs des étapes |
| `Accent color` | Orange, rouge, rose, violet, bleu, cyan, vert, encre, ou `Custom color`. Chaque étape garde quand même sa propre couleur |
| `Fonts` | `Interface`, `Numbers and code`, et la police des notes. Choix parmi les polices installées, ou un fichier `.ttf` / `.otf` |
| `Units (3D)` | m, cm, mm, km, in, ft. Seul l'affichage change : les fichiers restent en mètres |
| `3D navigation` | `Blender` ou `Maya` (voir plus bas) |
| `Interface size` | De 75 % à 200 %. Aussi `Ctrl +`, `Ctrl −`, `Ctrl 0` |
| `High contrast` | Textes secondaires, bordures et champs plus marqués, contour sur chaque commande |
| `Reduce motion` | Pas de fondus ni de défilement animé |

### Navigation 3D

| Geste | Blender | Maya |
|---|---|---|
| Tourner | Molette enfoncée, ou `Alt` + clic gauche | `Alt` + clic gauche |
| Déplacer | `Maj` + molette enfoncée | `Alt` + clic milieu, ou clic milieu seul |
| Zoom | Molette, ou `Ctrl` + molette enfoncée | Molette, ou `Alt` + clic droit |

## Editor (éditeur)

| Réglage | Ce qu'il fait |
|---|---|
| `Text size` | Taille du texte dans les notes et le code |
| `Line numbers (code files)` | Numéros de ligne |
| `Minimap on the right (code files)` | Mini-carte du fichier |
| `Wrap lines in code` | Retour à la ligne automatique |
| `Spell checker for notes (French and English)` | Souligne les mots inconnus |
| `Automatic backup` | Délai avant l'enregistrement automatique |
| `New notes folder` | Dossier de `Ctrl+N` quand rien n'est choisi, et des liens `[[ ]]` vers des notes qui n'existent pas encore |

## Files (fichiers)

| Réglage | Ce qu'il fait |
|---|---|
| `Files dropped in the window` | Lier, `Move into the vault`, `Copy into the vault`, ou demander à chaque fois |
| `Back up the vault as zip once a day (7 kept)` | Sauvegarde quotidienne. `Change…` choisit le dossier. `Back up now` la lance |
| `Linked folders` | La liste des dossiers liés. `+ Link a folder…` en ajoute. Retirer un lien ne supprime pas le dossier |
| `Vault` | `Open the folder`, `Change vault…` |
| `Confirm before deleting` | Demande avant d'envoyer à la corbeille |

## AI (IA)

| Réglage | Ce qu'il fait |
|---|---|
| `AI providers` | Colle une clé API pour brancher un service. Ses modèles apparaissent dans le menu du chat. Les clés restent sur l'ordinateur |
| Ollama | `Local, free, no key`. Installation et modèles : voir [Assistant IA](assistant-ia.md) |
| `Active model` | Le modèle utilisé |
| `Address` | Toute API compatible OpenAI. À changer pour un serveur personnalisé |
| `Reload the model list` | Relit les modèles du fournisseur |

## Online vault (coffre en ligne)

Branche ton propre stockage compatible S3 (Cloudflare R2, Backblaze B2, AWS S3, MinIO) : adresse, bucket, clés. Deux usages :

- le transfert par internet (les fichiers sont effacés du bucket dès qu'ils sont reçus) ;
- la synchronisation complète du coffre, avec gestion des conflits et un garde-fou contre les suppressions massives.

Aucune adresse ni clé n'est écrite dans le code ni dans le coffre.

## Updates (mises à jour)

| Réglage | Ce qu'il fait |
|---|---|
| `Check at startup (every 6 h)` | Vérifie les nouvelles versions |
| `Check now` | Vérifie tout de suite |
| `System › Display` | Le moteur d'affichage : `automatic`, `graphics card` ou `compatibility (OpenGL)`. Voir plus bas. Prend effet au prochain démarrage |
| `System › Native Wayland` | Linux seulement. Plus net si l'écran est mis à l'échelle, mais le glisser-déposer de fichiers ne marche pas. Prend effet au prochain démarrage |

Sous Wayland, Aryadne passe par défaut par XWayland pour que le glisser-déposer de fichiers fonctionne.

### Affichage : carte graphique ou mode compatible

Aryadne dessine son interface avec la carte graphique (Vulkan, DirectX 12 ou Metal selon la machine, OpenGL s'il n'y a rien d'autre). Les simulations calculées sur la carte s'affichent alors **sans quitter la carte** : c'est ce qui rend un gros tissu fluide à l'écran.

| Choix | Ce qu'il fait |
|---|---|
| `automatic` | La carte graphique. Si elle ne démarre pas, le mode compatible |
| `graphics card` | Force la carte. À choisir pour réessayer après un échec |
| `compatibility (OpenGL)` | L'ancien moteur. Les simulations restent calculées sur la carte, mais chaque image repasse par le processeur pour être affichée : c'est plus lent sur un gros tissu |

Sous les choix, une ligne dit ce qui est en service, avec le nom de la carte.

**Si l'affichage sur carte échoue.** Deux filets, sans rien avoir à faire :

- la carte refuse de démarrer : Aryadne ouvre tout de suite en mode compatible ;
- Aryadne se ferme avant d'avoir affiché sa première image (pilote qui plante) : le démarrage suivant se fait en mode compatible.

Dans les deux cas un message le dit à l'ouverture, et Aryadne reste en mode compatible jusqu'à la prochaine mise à jour, ou jusqu'à ce que tu choisisses `graphics card`.

**En dernier recours**, si la fenêtre ne s'ouvre plus du tout : lance Aryadne avec la variable d'environnement `ARY_RENDU=glow` (ou `ARY_RENDU=wgpu` pour forcer la carte). Elle passe avant le réglage.

| Système | Commande |
|---|---|
| Windows (PowerShell) | `$env:ARY_RENDU="glow"; .\aryadne.exe` |
| Linux | `ARY_RENDU=glow aryadne` |

## Shortcuts (raccourcis)

- **Global shortcuts** : la liste des actions globales. Clique un raccourci pour le changer. Les conflits sont signalés. Chaque action peut revenir à sa valeur par défaut.
- **Other shortcuts** : un rappel des raccourcis fixes.

Seules les actions globales se changent. Les éditeurs (3D, montage, nœuds…) gardent leurs touches. La liste complète est dans [Raccourcis](raccourcis.md).

## Réglages qui ne sont pas dans cette fenêtre

| Réglage | Où |
|---|---|
| Disposition des panneaux | `Edit › View › Layout`, voir [Fenêtres et dispositions](fenetres-et-dispositions.md) |
| Rendu 3D, éclairage, affichage | Les menus de la vue 3D, voir [Rendu](3d-rendu.md) |
| Format, cadence et durée d'un montage ou d'une composition | Dans le fichier lui-même (`Sequence settings`, onglet `Composition`) |
| Entrée dans le menu des applications | `Edit › Window › Add to the applications menu` ou `Add to the Start menu` |
