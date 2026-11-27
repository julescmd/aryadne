# Formats de fichiers

Presque tous les fichiers d'Aryadne sont du **JSON lisible**. Les noms de champs dans le fichier sont ceux des structures Rust. Deux exceptions sont binaires : l'image à calques et le cache de géométrie.

## Règle de nommage

Une extension propre à Aryadne commence par `ary`, suivi d'un mot court qui dit ce que contient le fichier. La table est dans `src/formats.rs` (`formats::ALL`).

| Identifiant | Extension | Anciennes extensions | Contenu | Module |
|---|---|---|---|---|
| `scene` | `.aryscene` | `.arya3` | Scène 3D | `scene` |
| `seq` | `.aryseq` | `.aryav` | Montage vidéo | `montage` |
| `motion` | `.arymotion` | | Composition animée | `motion` |
| `audio` | `.aryaudio` | | Session audio | `audio` |
| `music` | `.arymusic` | | Morceau | `music` |
| `mat` | `.arymat` | `.mat.json` | Matériau en nœuds | `nodes` |
| `comp` | `.arycomp` | `.comp.json` | Compositing en nœuds | `nodes` |
| `geo` | `.arygeo` | | Géométrie et simulation en nœuds | `nodes`, `geo` |
| `board` | `.aryboard` | `.board.json` | Board | `board` |
| `vector` | `.aryvec` | | Dessin vectoriel | `vector` |
| `story` | `.arystory` | | Storyboard | `story` |
| `image` | `.aryimg` | `.arya2`, `.psd` | Image à calques | `paint` |

Fichiers annexes, hors de cette table :

| Extension | Contenu | Module |
|---|---|---|
| `.arysmart` | Matériau intelligent (une pile de calques) | `atelier` |
| `.arygeocache` | Une image de simulation figée | `geo` |

Les anciennes extensions restent lisibles. `formats::upgrade` donne le nouveau nom. `formats::relink` corrige, après un renommage, les fichiers qui citent l'ancien nom.

## Chemins

Un fichier qui en cite un autre (une scène et ses modèles, un montage et ses médias) garde un chemin relatif. Selon le cas, il est relatif au coffre ou au dossier du fichier. `scene.rs` préfère le chemin relatif au dossier de la scène quand c'est possible.

## Scène 3D (`.aryscene`)

```json
{
  "version": 1,
  "fps": 24.0,
  "start": 0,
  "end": 120,
  "objects": [ … ],
  "layers": [ … ],
  "active_layer": "",
  "camera": "",
  "backdrop": null,
  "actions": [ … ],
  "strips": [ … ]
}
```

### Un objet

| Champ | Type | Sens |
|---|---|---|
| `name` | texte | Nom, unique dans la scène. Sert d'identifiant (parenté, contraintes) |
| `file` | texte | Chemin du modèle. Vide pour un objet sans forme. `@cube`, `@sphere`, `@plane`, `@cylinder`, `@cone`, `@torus` pour une forme de base. Un `.arygeo` : une géométrie de nodes, calculée à l'image de la scène |
| `sim_offset` | entier | Géométrie de nodes : décalage de départ de sa simulation, en images de la scène. Absent s'il vaut 0 |
| `pos`, `rot`, `scale` | 3 nombres | Position, rotation en degrés (X puis Y puis Z), taille |
| `visible` | booléen | |
| `color` | 3 octets ou `null` | Couleur d'affichage. `null` : celles du fichier |
| `role` | texte | `"light"`, `"camera"`, `"bone"`, ou vide |
| `parent` | texte | Nom du parent |
| `mods` | liste | Pile de modificateurs propre à l'objet |
| `material` | texte | Chemin d'un `.arymat` |
| `mesh_before` | texte | Le maillage qu'une retopologie appliquée a remplacé (même forme que `file`). Absent s'il n'y en a pas. `Scene::replace_mesh` le remplit, `Scene::restore_mesh` l'échange avec `file` |
| `paint` | texte | Texture de peinture (PNG), ancienne forme à un seul calque |
| `paint_layers` | liste | Calques de peinture : `name`, `channel`, `file`, `opacity`, `visible` |
| `opacity` | nombre | Pour une image de référence |
| `keys` | liste | Clés d'animation, triées par image |
| `cycle` | | Avant la première clé et après la dernière : constante, boucle, va-et-vient |
| `layer_keys` | liste | Clés par calque d'animation : `layer`, `keys` |
| `morphs` | liste | Formes : `name`, `file`, `value`, `keys` (image, valeur) |
| `aim`, `copy_loc`, `copy_rot` | texte | Contraintes : nom de la cible |
| `limit` | 2 × 3 nombres | Limite de position : minimum, maximum |

Lumière : `energy`, `point`, `range`, `shape` (0 selon `point`, 1 ponctuelle, 2 spot, 3 rectangle), `spot` (degrés), `blend`, `size`.

Caméra : `fov` (degrés sur le petit côté, 0 : celui de la vue).

Os : `length`, `rest` (pose de repos : position, rotation, taille), `ik` (nom de la cible), `ik_chain`.

Modèle lié : `skin` (nom de l'os racine), `reach`.

Le genre d'un objet se déduit (`scene::kind`) : `role` d'abord, puis fichier vide (groupe), puis extension d'image (plan de référence), sinon modèle.

### Une clé

| Champ | Sens |
|---|---|
| `frame` | Image |
| `pos`, `rot`, `scale` | La pose complète |
| `interp` | `"bezier"`, `"linear"`, `"constant"`, ou le nom d'un lissage commun. Vide : d'après `ease` |
| `ease` | Ancien réglage, lu quand `interp` est vide |
| `handle` | Type de poignées : 0 automatique, 1 alignées, 2 libres, 3 vecteur |
| `handles` | 9 poignées (position, rotation, taille en X Y Z) : gauche Δimage, gauche Δvaleur, droite Δimage, droite Δvaleur. Vide : automatiques |

### Le reste de la scène

| Champ | Sens |
|---|---|
| `layers` | Calques d'animation : `name`, `weight`, `mute` |
| `active_layer` | Le calque où vont les nouvelles clés. Vide : la base |
| `camera` | Nom de la caméra active |
| `backdrop` | Vidéo de fond : `file`, `fps`, `from`, `first`, `frames`, `size` |
| `actions` | Actions du montage d'animation : `name`, `tracks` (objet, clés) |
| `strips` | Bandes : `action`, `start`, `repeat`, et les réglages de la bande |

### Données à côté de la scène

Tout ne vit pas dans le `.aryscene` :

| Donnée | Emplacement |
|---|---|
| Maillage modifié en mode édition | Un `.obj` à côté de la scène |
| IDs de matériau, coutures, UV | `.app/meshdata/<identifiant>.json` (`model3d::meshdata_file`). Le champ `high` y garde le modèle haute définition de la cuisson ; le `material` d'un ID est le chemin complet d'un `.arymat` |
| Maillage d'une retopologie appliquée | Un `… retopo.obj` à côté de l'ancien, jamais par-dessus (`model3d::retopo_file`) |
| Textures exportées depuis l'Atelier | `textures/` à côté du matériau, par défaut (`texexport::default_dir`) |
| Pile de modificateurs d'un modèle ouvert seul, maillage après « appliquer » | `.app/modifiers/<empreinte>.json` |
| Niveaux de multirésolution | À côté du maillage (voir `multires.rs`) |
| Textures de peinture et de cuisson | `textures/` du coffre |
| Formes (blendshapes) | Des `.obj` à côté |

## Graphes de nœuds (`.arymat`, `.arycomp`, `.arygeo`)

Les trois partagent la structure `nodes::Graph`.

```json
{
  "kind": "Material",
  "next_id": 7,
  "nodes": [
    { "id": 0, "op": "mat_out", "pos": [600, 0], "values": [[0.8, 0.8, 0.8, 1], …], "params": {} },
    { "id": 1, "op": "noise", "pos": [0, 0], "values": [[4, 4, 4, 4], …], "params": {} }
  ],
  "links": [ { "from": 1, "out": 1, "to": 0, "inp": 0 } ],
  "frames": [],
  "groups": [],
  "stack": null,
  "time": null
}
```

| Champ | Sens |
|---|---|
| `kind` | `"Material"`, `"Compositor"` ou `"Geometry"` |
| `nodes` | Les nœuds |
| `links` | Les liens : sortie `out` du nœud `from` vers l'entrée `inp` du nœud `to` |
| `frames` | Cadres : `rect`, `label`, `color`, `note` |
| `groups` | Groupes réutilisables : `name`, `inputs`, `outputs`, `nodes`, `links`, `frames` |
| `stack` | Matériau à calques : la pile de l'Atelier. Le graphe effectif en est tiré |
| `time` | Géométrie : `fps`, `start`, `end` |

### Un nœud

| Champ | Sens |
|---|---|
| `id` | Numéro, unique dans le graphe |
| `op` | Le genre du nœud : `"noise"`, `"mix"`, `"geo_grid"`… Voir `nodes::DEFS` |
| `pos` | Position dans le graphe |
| `values` | Valeur de chaque entrée non reliée. Toujours quatre nombres |
| `params` | Réglages : un nombre (`{"F": 0.5}`) ou un texte (`{"S": "multiplier"}`) |
| `muted`, `collapsed`, `preview` | Muet, réduit, miniature |
| `label` | Nom donné par l'utilisateur |
| `ins`, `outs` | Prises sur mesure (groupes, zones) : `name`, `sock`, `value` |

**Attention aux noms.** Les valeurs de `params` qui sont des choix sont écrites **en français** dans le fichier (`"multiplier"`, `"avec diagonales"`). C'est le texte source. Les renommer casserait les fichiers existants.

### La pile de l'Atelier

`stack.layers`, du bas vers le haut :

| Champ | Sens |
|---|---|
| `name` | Nom du calque |
| `master` | Chemin du `.arymat` maître |
| `params` | Valeurs des paramètres exposés : nom vers quatre nombres |
| `mask` | `kind` (« plein », « arêtes », « creux », « dessus », « coulures », « rayures », « grunge », « bruit », « fissures », « taches », « peint »), `amount`, `contrast`, `scale`, `invert`, `breakup`, `breakup_amount`, `breakup_scale`, `paint` |
| `opacity`, `visible` | |

Un `.arysmart` est la même pile, seule, dans le dossier `matériaux intelligents/`.

### Cache d'une zone de simulation

Quand une zone est figée, son nœud de fin porte `params.cache` : le chemin du dossier, `.cache/<nom>/zone-<id>`.

## Cache de géométrie (`.arygeocache`)

Binaire, petit-boutiste. Un fichier par image et par prise : `<image sur 5 chiffres>_<prise>.arygeocache`.

| Octets | Contenu |
|---|---|
| 8 | `ARYGEO1\0` |
| 4 | Nombre de points (u32) |
| 4 | Nombre d'arêtes (u32) |
| | Colonnes des points, puis colonnes des arêtes |
| 8 par arête | Les deux numéros de points (u32, u32) |
| 4 | Facultatif : `FACE` |
| 4 | Nombre de faces (u32) |
| 16 par face | Quatre numéros de points (u32). Le quatrième vaut `0xFFFFFFFF` pour un triangle |

Après les faces, un bloc `VOLU` facultatif porte le volume de la géométrie (fumée, feu) : trois entiers (voxels par axe), quatre `f32` (coin de la grille, taille d'un voxel), le nombre de champs, puis pour chaque champ son nom (longueur, octets UTF-8), son nombre de composantes, et ses valeurs en **f16**, composante par composante. Champs écrits : `densité`, `température`, `carburant`, `flamme` (1 composante), `vitesse` (3). Soit 14 octets par voxel. Un champ de nom inconnu est sauté à la lecture.

Le bloc `FACE` vient à la fin et n'existe que si la géométrie a des faces. Un cache écrit avant lui se relit (sans faces). Une version plus ancienne de l'application s'arrête après les arêtes et ignore ce bloc.

Un bloc de colonnes : le nombre de colonnes (u32), puis pour chacune : longueur du nom (u32), nom en UTF-8, largeur (u32, nombre de composantes, au plus 16), puis chaque composante en entier, en `f32` bruts.

Les données sont **en colonnes** : tous les x, puis tous les y, puis tous les z. Elles se relisent telles quelles, sans conversion. Voir `Geo::to_bytes` et `Geo::from_bytes` dans `src/geo.rs`.

## Image à calques (`.aryimg`)

Binaire.

| Partie | Contenu |
|---|---|
| 8 octets | `ARYIMG1\n` |
| 4 octets | Longueur de l'en-tête (u32, petit-boutiste) |
| En-tête | JSON |
| Calques | Chaque calque en PNG, précédé de sa longueur |
| Masques de groupe | Leurs PNG suivent ceux des calques |

En-tête : `version`, `w`, `h`, `active`, `layers` (réglages de chaque calque), `groups`, `guides`, `anim`, `palette`.

Réglages d'un calque : nom, visibilité, opacité, mode de fusion, décalage, masque, fond, verrous, écrêtage, réglage (`adj`), groupe, styles (`fx`), tenue d'animation (`hold`).

Les pixels sont en RGBA 8 bits, alpha non prémultiplié.

`bridge.rs` aplatit un `.aryimg` en image ordinaire pour le montage, le board, la 3D et les nœuds. Le résultat est gardé en cache et refait seulement quand le fichier change.

## Montage (`.aryseq`)

| Champ | Sens |
|---|---|
| `version`, `width`, `height`, `fps` | Format de la séquence |
| `tracks`, `atracks` | Nombre de pistes image et son |
| `vstate`, `astate` | État de chaque piste (muette, verrouillée…) |
| `clips` | Les clips |
| `markers`, `mark_in`, `mark_out` | Repères, entrée, sortie |
| `bin` | Les médias du projet |
| `magnetic` | Timeline magnétique |
| `chapters`, `angles`, `proxies` | Chapitres, angles multicam, proxies |

Un clip : `id`, `name`, `file`, `kind`, `track`, `audio` (clip de son), `start`, `src_in`, `src_out`, `media` (durée du média), `speed`, `link` (image et son liés), `opacity`, `volume`, `fade_in`, `fade_out`, `xfade`, `x`, `y`, `scale`, `keys`, étalonnage (`brightness`, `contrast`, `saturation`, `wheels`, `grade`, `mask`), découpe (`cut`), `stab`, `fx`, titre (`text`, `text_size`, `text_color`, `font`, `outline`). Les temps sont en secondes.

## Composition animée (`.arymotion`)

| Champ | Sens |
|---|---|
| `w`, `h`, `fps`, `duration`, `bg` | Format |
| `layers` | Les calques, le premier au-dessus |
| `music` | Chemin d'un son |
| `blur` | Flou de mouvement |
| `camera` | Caméra 2,5D |
| `controls` | Contrôles lus par les expressions |
| `params`, `template` | Réglages exposés au montage |

Un calque : `id`, `name`, `kind`, `start`, `end`, `visible`, `parent`, les pistes `pos`, `scale`, `rot`, `opacity`, `trim`, `depth`, puis `style`, `reveal` (animateur de texte), `wiggle`, `cycle`, `matte`, `repeat`, `exprs` (propriété vers formule), `masks`, `fx`, `auto_orient`, `font_axes`, `ik`.

Une piste (`Track`) est soit une valeur fixe, soit une liste de clés.

## Session audio (`.aryaudio`)

`tracks`, `markers`, `tempo`. Un clip : `src` (le son), `start`, `offset` (début dans le son), `len`, `gain`, `fade_in`, `fade_out`, `fx`. Temps en secondes.

## Morceau (`.arymusic`)

`bpm`, `beats_per_bar`, `bars`, `key` (la tonique), `minor`, `tracks`. Une piste a un instrument, un volume, un panoramique et des régions. Une note : `t` (début dans la région, en temps), `len`, `p` (hauteur MIDI, 60 = do central), `v` (force).

## Board (`.aryboard`)

`items`, `pan`, `zoom`. Un élément : `kind`, `x`, `y`, `w`, `h`.

`kind` porte un champ `type` :

| `type` | Champs |
|---|---|
| `image` | `rel`, `flip`, `gray`, `crop`, `guide` |
| `text` | `text`, `size`, `look` |
| `note` | `rel` |
| `palette` | `colors` |
| `stroke` | `points` (0 à 1 dans le rectangle de l'élément), `color`, `width`, `widths` |

## Dessin vectoriel (`.aryvec`)

`boards` (plans de travail), `shapes`, `next_id`, `swatches`, `styles`, `comments`, `vars`, `modes`.

Une forme : `id`, `name`, `kind`, `x`, `y`, `rot`, `fills`, `strokes`, `opacity`, `visible`, `locked`, `parent`, `shadow`, `blur`, `blend`, `layout`, `component`, `instance_of`, `widths`, `constraint`, `style`, `link`, `on_path`, `fill_var`, `hover`, `lib`.

Un remplissage uni s'écrit `{ "type": "solid", "c": [r, g, b, a] }`.

Le document n'est pas stocké en SVG. Il est **converti** en SVG pour le rendu (resvg), l'export et les aperçus.

## Storyboard (`.arystory`)

`version`, `width`, `height`, `panels`, `next_id`. Une case : `id`, `scene`, `shot`, `camera`, `dur`, `dialogue`, `action`, `notes`, `cam` (deux cadres : centre X, centre Y, zoom).

Le dessin de chaque case est un PNG dans le dossier « <nom> - cases » à côté du fichier.

## Le dossier `.app`

Caché, à la racine du coffre. Données propres à Aryadne.

| Chemin | Contenu |
|---|---|
| `.app/trash/` | La corbeille du coffre |
| `.app/history/` | Anciennes versions des fichiers texte |
| `.app/backup/` | Copies avant modification par un script ou un enregistrement dans le fichier d'origine |
| `.app/untitled/<n>/` | Fichiers sans titre. Ce qui y reste est proposé au lancement suivant |
| `.app/cache/` | Caches (miniatures, rendus Motion…) |
| `.app/meshdata/` | IDs de matériau, coutures, UV par modèle |
| `.app/modifiers/` | Piles de modificateurs des modèles ouverts seuls |
| `.app/annotations/` | Dessins sur les vidéos |
| `.app/chats/` | Conversations du chat |
| `.app/python/aryadne.py` | Le module de l'API de scripts |
| `.app/links.json` | Dossiers et fichiers liés |
| `.app/sources.json` | Origine de chaque fichier importé |
| `.app/temps.json` | Temps actif par fichier et par jour |
| `.app/explorer.json` | État du panneau des fichiers (signets, dossiers ouverts) |
| `.app/sync.json` | État de la synchronisation en ligne |
| `.app/dictionnaire.txt` | Dictionnaire personnel du correcteur |

Hors de `.app`, à la racine du coffre : `.cache/` reçoit les simulations figées.

## Réglages de l'utilisateur

Hors du coffre : `%APPDATA%\aryadne` sous Windows, `~/.config/aryadne` sous Linux.

| Fichier | Contenu |
|---|---|
| `settings.json` | Les préférences (`settings::Prefs`) |
| `keymap.json` | Raccourcis globaux modifiés |
| `workspaces.json` | Étapes, onglets, dispositions |
| `dock.json` | Places des panneaux ancrables |
| `last_vault`, `recent_vaults` | Dernier coffre, coffres récents |
| `last_board`, `last_backup`, `last_update_check` | États divers |
| `theme`, `import_mode` | Anciens réglages simples |
| `github_token` | Jeton pour les mises à jour d'un dépôt privé |

Les clés des fournisseurs d'IA et du stockage en ligne sont aussi dans ce dossier. Elles ne vont jamais dans le coffre.

## Compatibilité

- Les champs absents d'un vieux fichier prennent une valeur par défaut (`#[serde(default)]`). Ajouter un champ ne casse donc pas les anciens fichiers.
- Beaucoup de champs à leur valeur par défaut ne sont pas écrits (`skip_serializing_if`), pour garder les fichiers courts.
- Retirer ou renommer un champ, ou une valeur de choix écrite en français, casse la relecture. À éviter.
- Seuls `.aryscene`, `.aryseq`, `.arystory` et `.aryimg` portent un numéro de `version`.
