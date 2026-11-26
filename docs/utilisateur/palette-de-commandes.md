# Palette de commandes

La palette lance n'importe quelle action au clavier, et ouvre n'importe quel fichier.

## L'ouvrir

- `Ctrl+K` ou `Ctrl+Maj+P`.
- Ou clic dans le champ **Search or run a command**, au centre de la barre du haut.

## S'en servir

1. Tape quelques lettres. La recherche est floue : « nvscn » trouve « nouvelle scène 3D ». Le texte trouvé tel quel et les débuts de mots passent devant.
2. `↑` `↓` pour choisir.
3. `Entrée` pour lancer. `Échap` pour fermer.

La liste mélange les commandes et les fichiers du coffre. La recherche marche en français et en anglais, quelle que soit la langue affichée.

## Les commandes

### Aller à

`Go to: files`, `Go to: brain`, `Go to: refs`, `Go to: tasks`, `Go to: chat`, `Go to: search`.

### Créer

| Commande | Fichier créé |
|---|---|
| `New note` | `.md` |
| `Daily note` | Note du jour dans `journal/` |
| `New note from a template` | `.md` depuis `modèles/` |
| `New board` | `.aryboard` |
| `New file… (pick a kind)` | Ouvre la fenêtre de création |
| `New 3D scene` | `.aryscene` |
| `New video edit` | `.aryseq` |
| `New animated composition (motion)` | `.arymotion` |
| `New audio session` | `.aryaudio` |
| `New song (sequencer)` | `.arymusic` |
| `New material (nodes)` | `.arymat` |
| `New layered material (Atelier)` | `.arymat` avec une pile de calques |
| `New compositing (nodes)` | `.arycomp` |
| `New geometry or simulation (nodes)` | `.arygeo` |
| `New vector drawing` | `.aryvec` |
| `New storyboard` | `.arystory` |
| `New screenplay (Fountain)` | `.fountain` |
| `New production tracker` | Dossier `Production/` |

### Importer et partager

`Import files…`, `Link a folder…`, `Import an image from a URL…`, `Transfer between computers`, `Convert / compress files…`.

### Coffre

`Back up the vault now (zip)`, `Sync with the cloud`, `Vault statistics`, `Open the vault folder`, `Change vault…`, `Reload files`, `Vault: switch to the new extensions`.

### Fenêtre

`Toggle light / dark theme`, `Pin above other windows`, `Compact mode`, `Side-by-side view`, `Add to the Start menu` (Windows) ou `Add to the applications menu` (Linux).

### Outils

`AI assistant`, `Tidy up with AI (brain)`, `Time spent per project`, `Timer: 25 min focus`, `Timer: 50 min`, `Timer: 5 min break`, `Stop the timer`, `Check for updates`.

### Réglages

`Settings: appearance`, `Settings: editor`, `Settings: files and links`, `Settings: AI`, `Settings: shortcuts`, `Settings: online vault`.

### Sur le fichier ouvert

Ces commandes n'apparaissent que si elles s'appliquent.

| Commande | Effet |
|---|---|
| `File: version history` | L'historique du fichier |
| `File: summarize with AI` | Un résumé |
| `File: compare with another file…` | Différences entre deux fichiers |
| `File: git history` | Les commits du fichier, si le coffre est sous git |
| `File: close the tab` | Ferme l'onglet |
| `File: run the script` | Lance le script (comme `F5`) |
| `File: live preview in the browser` | Pour le HTML et le SVG |

## Ouvrir un fichier : `Ctrl+P`

`Ctrl+P` est la version « fichiers seulement ». Tape un nom, `Entrée` ouvre. Si rien ne correspond, `Entrée` crée une note de ce nom.

## Limites

- La palette ne couvre pas les actions internes de chaque éditeur (par exemple « biseau » en mode édition 3D, ou « couper à la tête de lecture » au montage). La feuille de route le prévoit.
- Les raccourcis des commandes globales se changent dans `Settings › Shortcuts`. Voir [Raccourcis](raccourcis.md).
