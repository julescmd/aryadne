# Coffre et fichiers

## Le coffre

Le coffre est un dossier ordinaire. Tu peux l'ouvrir dans l'explorateur de ton système, le copier, le synchroniser, le mettre sous git. Aryadne n'y ajoute qu'un dossier caché, `.app/`, pour ses propres données.

Dossiers créés au départ :

| Dossier | Rôle |
|---|---|
| `notes/` | Les notes Markdown |
| `refs/` | Les images de référence importées |
| `boards/` | Les boards |
| `.app/` | Corbeille, historique, caches, liens (voir [Formats](../technique/formats.md#le-dossier-app)) |

D'autres dossiers apparaissent quand tu t'en sers : `modèles/` (modèles de notes), `textures/` (cuisson et peinture 3D), `envois/` (images passées d'un outil à l'autre), `reçus/` (transfert), `scripts/`, `palettes/`, `matériaux intelligents/`.

Changer de coffre : `File › Vault › Change vault…`. Les coffres récents sont dans le même menu.

## Le panneau des fichiers

`Ctrl+B` l'affiche ou le cache. Il contient, de haut en bas :

- un champ **Search for a file** ;
- des **filtres par type** : notes, images, 3D, code, documents, videos, boards, nodes, other ;
- **Bookmarks** (signets) : fichiers et recherches enregistrés ;
- **Recent** ;
- l'arbre du coffre, replié par défaut.

### Gestes

| Geste | Effet |
|---|---|
| Double-clic ou `Entrée` | Ouvrir |
| `F2` | Renommer ou déplacer (tape un chemin) |
| `Suppr` | Mettre à la corbeille du coffre |
| `Ctrl+clic`, `Maj+clic` | Choisir plusieurs fichiers |
| `Ctrl+A` | Tout choisir |
| `Ctrl+Z` | Annuler la dernière opération sur les fichiers |
| Glisser sur un dossier | Déplacer |
| Clic droit | Menu |

### Menu du clic droit

`Open`, `New file here`, `New folder here`, `Rename / move`, `Copy into the vault`, `Open with the default app`, `Open with…`, `Export as zip…`, `Export with its links as zip…`, `Send to another computer…`, `Show in folder`, `Copy path`, `Add to bookmarks`, `Convert, compress…`, `Delete`.

Sur une photo : **PBR material from photo**. Aryadne en tire une couleur sans éclairage (albedo), une normale, une hauteur et une rugosité, posées à côté de la photo. Deux variantes : `Relief: light is raised` ou `Relief: light is recessed`.

### Options (bouton `…`)

- `Sort` : `By name`, `Recently modified`, `By type`.
- `Show extensions`.
- `"Recent" section`.
- `Collapse all`, `Expand all`, `Show the open file`.
- `Show as a list` ou `Show as a grid, with thumbnails`. La grille montre des miniatures pour les images, les modèles 3D, les vidéos et les `.blend`.

## Importer

Glisse des fichiers dans la fenêtre, ou `File › Import files…`. Une question s'affiche :

| Choix | Effet |
|---|---|
| `Link (keep in place)` | Recommandé. Le fichier reste dans son dossier. Aryadne l'affiche sans le copier. |
| `Move into the vault` | Le fichier quitte son dossier d'origine. Aucun doublon. |
| `Copy into the vault` | L'original reste. Le fichier prend deux fois la place. |

`Remember this choice` fixe la réponse. Elle se change ensuite dans `Settings › Files`.

Par défaut, un fichier importé va dans `refs/`, ou dans le dossier choisi dans l'explorateur. Un fichier déjà dans le coffre n'est jamais recopié.

### Dossiers et fichiers liés

Un lien montre un dossier de ton disque dans le coffre, sans rien copier. Utile pour un projet Blender, un dossier de cours, une banque de textures.

- `File › Import › Link a folder…`, ou `Settings › Files › + Link a folder…`.
- Le dossier lié apparaît dans l'explorateur comme un dossier normal. Survole-le pour voir son emplacement réel.
- Clic droit : `Move into the vault` ou `Copy into the vault` si tu veux le rapatrier.
- Retirer le lien ne supprime jamais l'original.

La liste des liens est dans `.app/links.json`.

### Image depuis une adresse web

`File › Import › Image from a URL…`. L'image arrive dans `refs/`. Coller une adresse (`Ctrl+V`) dans la page References fait la même chose. L'adresse d'origine est gardée dans `.app/sources.json`.

## Supprimer et récupérer

`Suppr` envoie dans `.app/trash/`. Rien n'est effacé du disque. `Ctrl+Z` dans l'explorateur remet le fichier. Une confirmation s'affiche si `Settings › Files › Confirm before deleting` est coché.

## Historique des versions

Chaque fichier texte garde ses anciennes versions.

- Une version est gardée à chaque enregistrement, au plus une toutes les 2 minutes. Les 40 dernières sont conservées.
- `File › Version history` ouvre la liste.
- `Name this version` : une version nommée reste tant que tu ne la supprimes pas.
- `Compare with` : une autre version, ou `Now`. Les lignes ajoutées et retirées sont comptées. `See the differences` les montre côte à côte.
- `Restore this version` : l'état actuel est d'abord gardé sous le nom « before restoring ».

Pour un dessin vectoriel, la comparaison compte les formes ajoutées, retirées et modifiées, et montre les différences en rose.

### Versions publiées

Dans la barre d'un fichier, **Publish version** écrit une copie figée dans un dossier `versions` à côté du fichier : `v001`, `v002`… Depuis la liste, `Open this version` ou `Go back to this version` (l'état actuel est publié avant).

Différence avec l'historique : l'historique est automatique et limité à 40. Une version publiée est un vrai fichier, voulu, qui reste.

## Sauvegarde

- `Settings › Files › Back up the vault as zip once a day (7 kept)`.
- `File › Vault › Back up now (zip)`.
- Les fichiers de plus de 50 Mo et la corbeille ne sont pas inclus.

## Exporter et transférer

- **Export as zip…** : un fichier ou un dossier.
- **Export with its links as zip…** : ajoute les notes liées, les images citées et le contenu d'un board.
- **Transfer between computers** : sur le même réseau, sans cloud ni limite de taille.
  1. Sur l'ordinateur qui reçoit : onglet `Receive`, puis `Start receiving`. Un code à 4 chiffres s'affiche.
  2. Sur l'autre : onglet `Send`, ajoute les fichiers, choisis l'ordinateur dans la liste (ou tape son adresse), donne le code.
  3. Les fichiers arrivent dans `reçus/`.
- **Online vault** : synchronisation avec ton propre stockage compatible S3 (Cloudflare R2, Backblaze B2, AWS S3, MinIO). Tout se règle dans `Settings › Online vault`. Les identifiants restent dans le dossier de configuration de l'ordinateur, jamais dans le coffre.

## Convertir et compresser

Clic droit sur un fichier ou un dossier, `Convert, compress…` :

| Type | Formats de sortie |
|---|---|
| Images | JPG, PNG, WebP, TGA, TIFF, BMP, avec qualité et taille |
| Vidéos | MP4, WebM, MOV ProRes, GIF |
| Sons | MP3, Ogg, AAC, FLAC, WAV |

Un aperçu et le poids estimé s'affichent avant de lancer. Les vidéos et les sons passent par ffmpeg. S'il manque, Aryadne propose de l'installer dans ton dossier utilisateur, sans droits administrateur.

## Formats lus

| Famille | Extensions |
|---|---|
| Images | PNG, JPG, WebP, GIF, BMP, TIFF, TGA, EXR, HDR |
| 3D | OBJ, FBX binaire, glTF, GLB, STL, 3DS, PLY, Maya ASCII (`.ma`, maillages) |
| Blender | `.blend` : miniature seulement |
| Photoshop | `.psd` : ouvert comme une image à calques |
| Vidéo | MP4, MOV, WebM, MKV, AVI |
| Bureautique | `.pptx` (affiché et modifiable), `.xlsx`, `.docx`, `.odp`, `.ods`, `.odt` (aperçu) |
| Obsidian | `.canvas` : import en board, export d'un board |
| Scénario | `.fountain` |
| Son | WAV, MP3 et ce que lit ffmpeg ; `.mid` pour le séquenceur |

Les extensions `.mb` et `.max` sont reconnues par l'explorateur 3D, mais le code ne contient un lecteur que pour Maya ASCII. **Incertain :** un `.mb` ou un `.max` ne s'ouvrira probablement pas.

## Statistiques et temps passé

- `File › Vault › Statistics` : taille par type, fichiers les plus lourds, nombre de mots.
- `File › Help › Time spent per project` : le temps actif devant chaque fichier, par jour. « Actif » veut dire que la fenêtre a le focus et qu'il y a eu une action dans les 2 dernières minutes. `Copy as CSV` donne projet, fichier et heures, pour une facture.

## Anciennes extensions

Les fichiers créés par d'anciennes versions (`.arya3`, `.aryav`, `.mat.json`, `.comp.json`, `.board.json`, `.arya2`) s'ouvrent toujours. La palette de commandes propose **Vault: switch to the new extensions** : les fichiers sont renommés et ceux qui les citent sont corrigés.
