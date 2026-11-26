# Architecture

Aryadne est un seul exécutable écrit en Rust. L'interface est faite avec **egui** (bibliothèque d'interface en mode immédiat) et **eframe** (la fenêtre et la boucle d'affichage). Version décrite : 0.35.0 (`Cargo.toml`).

Environ 120 fichiers dans `src/`, 130 000 lignes. Pas de sous-dossiers : chaque fichier est un module, déclaré dans `src/main.rs`.

## Les principes de la maison

- **Un coffre, des fichiers.** Tout document est un fichier du coffre. Pas de base de données.
- **Rien d'installé.** Les outils externes (ffmpeg, Ollama, Open Image Denoise) sont cherchés sur la machine, sinon téléchargés dans le dossier de l'utilisateur, sans droits administrateur.
- **Le français est la langue source.** Les textes de l'interface sont écrits en français dans le code et traduits par une table. Voir [Traductions](traductions.md).
- **Chaque noyau de calcul sur carte graphique a sa version processeur**, et un test qui compare les deux.
- **Commentaires en français**, comme le reste du code.
- **Aucun code GPL.** Les idées viennent de sources publiques (articles, documentations), pas du code de Blender.

## Mode immédiat : ce que cela change

Avec egui, l'interface n'est pas un arbre d'objets qu'on modifie. À **chaque image**, le code redessine tout : il déclare les boutons, et egui répond tout de suite si un bouton vient d'être cliqué.

```rust
if ui.button(crate::i18n::t("créer")).clicked() {
    // l'action, tout de suite
}
```

Conséquences :

- L'état vit dans des structures Rust ordinaires (`App`, les `Editor`), pas dans les widgets.
- Un calcul long ne doit jamais se faire dans le dessin. Il part dans un fil d'exécution, et l'interface regarde à chaque image si le résultat est prêt.
- Rien ne se redessine sans raison : egui ne relance une image que sur un évènement, ou quand le code le demande (`request_repaint`).

## Le trajet d'une image de l'interface

`main()` (dans `src/main.rs`) lit les réglages, prépare la fenêtre et lance `eframe::run_native`. Ensuite, eframe appelle `App::ui` à chaque image. Dans l'ordre :

1. **Raccourcis.** `shortcuts()` lit les raccourcis globaux (`keymap`). Les entrées de menu qui correspondent à un raccourci le rejouent : chaque éditeur réagit comme au clavier.
2. **Tâches de fond.** Synchronisation du coffre en ligne, temps passé, sauvegarde automatique (500 ms après la dernière frappe), résultats des fils d'exécution (`poll_jobs`, `poll_backup`, `poll_url`).
3. **Fichiers glissés** depuis le système.
4. **Barres.** Barre de menus, barre du haut, barre des étapes et de leurs outils, barre du bas.
5. **Disposition.** `tiles()` dessine l'arbre de cases (`layout`, bibliothèque `egui_tiles`). Chaque case appelle le dessin de son panneau.
6. **Document.** La case centrale appelle `main_view()`, qui choisit la page (fichiers, cerveau, références, boards, tâches, chat, recherche). Pour un fichier, `editor()` appelle l'éditeur du document courant.
7. **Fenêtres détachées.** Les panneaux sortis de la fenêtre sont dessinés dans leur propre fenêtre du système (`dock::os_window`).
8. **Messages** passagers et fenêtres modales.

Les actions demandées pendant le dessin (ouvrir un fichier, renommer, supprimer) sont empilées dans `self.actions` et exécutées à la fin de l'image. Cela évite de modifier une liste pendant qu'on la parcourt.

## Du fichier à l'éditeur

Un document ouvert est une structure `Doc` : son chemin relatif, son contenu, et l'état de l'éditeur de texte.

Le contenu est une énumération, `Content`. Chaque variante porte l'éditeur du genre :

| Variante | Éditeur | Fichiers |
|---|---|---|
| `Text` | `code::Editor` (commun à tous les `Doc`) | Notes, code, texte |
| `Image` | Visionneuse d'image | PNG, JPG… |
| `Model` | `model3d::Viewer` | Modèles et `.aryscene` |
| `Nodes` | `nodes::Editor` | `.arymat`, `.arycomp`, `.arygeo` |
| `Paint` | `paint::Editor` | `.aryimg`, `.psd` |
| `Vector` | `vector_ui::Editor` | `.aryvec` |
| `Story` | `story_ui::Editor` | `.arystory` |
| `Montage` | `montage::Editor` | `.aryseq` |
| `Motion` | `motion_ui::Editor` | `.arymotion` |
| `Audio` | `audio_ui::Editor` | `.aryaudio` |
| `Music` | `music_ui::Editor` | `.arymusic` |
| `Video` | `video::Viewer` | Vidéos |
| `Office` | `office::Viewer` | Documents bureautiques |
| `NeedFfmpeg`, `Unreadable` | Messages | |

`App::open(rel)` choisit la variante d'après l'extension, par une suite de tests (`model3d::is_model`, `story::is_story`, `nodes::is_graph`…). `workspace::owner(rel)` dit dans quelle étape le fichier a sa place. Voir [Ajouter un mode](ajouter.md#ajouter-un-mode).

Le board est un cas à part : c'est une page (`View::Boards`), pas une variante de `Content`.

## Les modules, par domaine

### Cœur de l'application

| Module | Rôle |
|---|---|
| `main` | `App`, la boucle d'interface, les menus, l'ouverture des fichiers, les pages |
| `vault` | Le coffre : chemins relatifs, lecture, écriture sûre (`atomic_write`), liens, graphe des notes |
| `files` | Le panneau des fichiers |
| `formats` | La table des formats propres à Aryadne |
| `untitled` | Fichiers sans titre et récupération après plantage |
| `learn`, `learn_ui` | Le mode Apprendre : leçons embarquées, vérifications, progression, panneau. Voir [Mode Apprendre](apprendre.md) |
| `workspace` | Les étapes, leurs outils, leurs pages, leurs dispositions |
| `layout` | Disposition en cases (`egui_tiles`) |
| `dock` | Panneaux ancrables, flottants, fenêtres détachées |
| `screens` | Liste des écrans branchés (Xinerama, user32) |
| `keymap` | Raccourcis globaux modifiables |
| `settings` | La fenêtre de réglages et `Prefs` |
| `theme` | Thèmes, palettes, polices |
| `i18n` | Traductions |
| `icons`, `lucide`, `lucide_all` | Icônes |
| `extras` | Palette de commandes, tâches, note du jour, modèles, historique, sauvegarde, minuteur, terminal, exécution de scripts |
| `desktop`, `updater` | Intégration au système, mises à jour |
| `timetrack`, `sources` | Temps passé, origine des fichiers importés |

### Notes et texte

| Module | Rôle |
|---|---|
| `code` | L'éditeur de texte et de code |
| `hl` | Coloration du code |
| `lsp` | Client de serveurs de langage |
| `live` | Serveur web local pour l'aperçu en direct |
| `preview` | Rendu Markdown en lecture |
| `mdextra` | Maths LaTeX et diagrammes Mermaid |
| `mdexport` | Export HTML et PDF d'une note |
| `bases` | Blocs `base` et `tasks` |
| `brain` | Le graphe du coffre |
| `related`, `search`, `spell` | Notes proches, recherche, orthographe |
| `fountain` | Scénarios |

### IA

| Module | Rôle |
|---|---|
| `ai` | Appels à une API compatible OpenAI |
| `ollama` | Installation et gestion d'Ollama |
| `chat` | Chat avec le coffre |
| `aigraph` | L'assistant et les graphes de nœuds : contexte envoyé au modèle, blocs `graphe`, validation et application |
| `aitasks` | L'IA dans l'éditeur de texte |
| `transcribe` | Parole vers sous-titres |

### 2D

| Module | Rôle |
|---|---|
| `paint` | Retouche à calques |
| `psd` | Lecture et écriture PSD |
| `colorize` | Mise en couleur par zones |
| `vector`, `vector_ui`, `vgeom` | Dessin vectoriel : modèle, interface, géométrie sur grille |
| `board`, `boardlive`, `canvas` | Boards, éléments vivants, JSON Canvas |
| `palette` | Générateur de palettes |
| `images`, `zoomview` | Chargement d'images, zoom |
| `story`, `story_ui` | Storyboard |
| `pbrphoto` | Photo vers matériau PBR |
| `bridge`, `sendto` | Passerelles entre outils |

### 3D

| Module | Rôle |
|---|---|
| `model3d` | Visionneuse, scène, modes, rendu « pixols ». Le plus gros module |
| `scene` | Le format `.aryscene` et l'évaluation d'une image |
| `meshedit` | Maillage modifiable du mode édition |
| `modifiers` | Pile de modificateurs |
| `sculpt`, `multires`, `remesh` | Sculpture, multirésolution, remaillage voxel |
| `uvedit`, `uvtools` | IDs de matériau, coutures, UV |
| `retopo` | Retopologie |
| `paint3d` | Peinture sur le modèle |
| `atelier` | Matériaux à calques |
| `gizmo` | Poignées de transformation |
| `bvh` | Lancer de rayons sur un maillage |
| `pathtrace`, `gputrace`, `oidn` | Tracé de chemins (processeur, carte graphique), débruitage |
| `rig`, `autorig`, `mocap`, `matchmove` | Squelettes, rig bipède, BVH, suivi de caméra |
| `nla`, `animtools`, `curves`, `keys` | Montage d'animation, outils, courbes, lissages |
| `fbx`, `fbxwrite`, `fbxscene`, `gltfscene`, `formats3d`, `blend` | Formats 3D |
| `text3d` | Texte 3D |

### Nœuds et simulation

| Module | Rôle |
|---|---|
| `nodes` | Définitions des nœuds, graphe, évaluation matériau et image, éditeur |
| `nodeflow` | Groupes, zones, renvois, mise à plat |
| `geo` | Moteur de géométrie et de simulation |
| `geoview` | Aperçu 3D, lecture, tableau des attributs |
| `geoscene` | Géométrie de nodes dans une scène 3D : image de scène vers image de simulation, maillage, obstacles venus de la scène, suite d'OBJ |
| `gpusim` | Socle de simulation sur carte graphique |
| `gpucloth` | Tissu en VBD |
| `gpubench` | Banc de mesure |
| `expr` | Expressions des propriétés animées (Motion) |

### Mouvement, vidéo, son

| Module | Rôle |
|---|---|
| `motion`, `motion_ui`, `lottie` | Motion design et ses exports |
| `montage`, `montage_ui`, `montage_panels` | Montage vidéo |
| `video`, `ffmpeg`, `scopes`, `track2d` | Lecteur, ffmpeg, scopes, suivi 2D |
| `audio`, `audio_ui`, `spectral` | Éditeur audio, vue spectrale |
| `music`, `music_ui`, `midi` | Séquenceur, MIDI |
| `timenav` | Navigation commune des timelines |

### Échanges

| Module | Rôle |
|---|---|
| `transfer` | Zip et transfert sur le réseau local |
| `cloud`, `cloud_ui` | Coffre en ligne (S3) |
| `convert` | Conversion et compression |
| `office`, `slides` | Documents bureautiques, PowerPoint |

## Calcul hors de l'interface

Trois moyens reviennent partout.

| Moyen | Usage | Exemple |
|---|---|---|
| `rayon` | Paralléliser une boucle sur tous les cœurs | Tracé de la vue 3D, cuisson |
| `std::thread` + canal ou `Arc<Mutex>` | Un travail long, relu à chaque image | Chargement d'un modèle, rendu final, export |
| Calcul borné dans le temps | Faire un peu de travail par image | Simulation : `Engine::deadline` arrête le calcul et le reprend à l'image suivante |

La carte graphique des simulations est ouverte à part de celle de l'interface (qui dessine en OpenGL). Voir [Simulation sur carte graphique](simulation-gpu.md).

## Écriture des fichiers

Toute écriture passe par `vault::atomic_write` : le contenu va dans un fichier temporaire du même dossier, puis le remplace d'un coup. Une coupure de courant laisse l'ancienne version intacte.

Les chemins manipulés sont relatifs à la racine du coffre, avec des `/`, même sous Windows.

## Bibliothèques principales

| Bibliothèque | Usage |
|---|---|
| `eframe`, `egui`, `egui_extras`, `egui_tiles` | Interface, images, coloration, disposition en cases |
| `wgpu`, `pollster` | Calcul sur carte graphique |
| `rayon` | Parallélisme |
| `serde`, `serde_json` | Lecture et écriture des fichiers JSON |
| `image`, `exr`, `resvg`, `ab_glyph` | Images, EXR, rendu SVG, polices |
| `gltf` | Lecture glTF |
| `zip`, `tar`, `ruzstd`, `lzma-rs` | Archives et décompression |
| `ureq`, `hmac` | Requêtes web, signatures S3 |
| `rfd`, `arboard`, `open`, `dirs` | Dialogues de fichiers, presse-papiers, ouverture par le système, dossiers de l'utilisateur |
| `regex`, `chrono` | Expressions régulières, dates |
| `libloading` | Chargement d'Open Image Denoise à l'exécution |
| `self-replace` | Mise à jour de l'exécutable |

Pas de bibliothèque pour : FBX, PSD, MIDI, PowerPoint, la retopologie, le dépliage UV, le tracé de chemins. Ils sont écrits à la main, à partir des spécifications publiques.

## Compiler et tester

```
cargo run              # lancer en développement
cargo test             # les tests
cargo build --release  # l'exécutable final
```

Sous Linux, il faut les paquets de développement de GTK 3, xkbcommon, Wayland et OpenGL (voir le README à la racine).

Les tests sont dans chaque module (`#[cfg(test)]`). Ils couvrent surtout la logique : sérialisation, évaluation de graphe, géométrie, comparaison processeur contre carte graphique.

### Variables d'environnement de développement

| Variable | Effet |
|---|---|
| `ARY_SANS_GPU` | Force le repli des simulations sur le processeur |
| `ARYADNE_VAULT` | Donnée aux scripts lancés par `F5` : la racine du coffre |
| `ARY_AI_FAKE` | Chemin d'un fichier texte : l'assistant et le chat « répondent » son contenu, par morceaux, sans rien envoyer sur le réseau. Pour essayer l'interface sans fournisseur |
| `ARY_AI_DUMP` | Chemin où écrire les messages envoyés au modèle (JSON), pour relire le contexte |
| `ARY_DEMO`, `ARY_OUT`, `ARY_EXEMPLES`, `ARY_BIG` | Lues par des tests pour écrire des fichiers de démonstration. Sans effet en usage normal |

### Options de ligne de commande

| Option | Effet |
|---|---|
| `--banc-gpu` | Mesure la carte graphique dans le terminal, sans ouvrir l'interface |

`workspace::launch_args` lit aussi un coffre et une étape au lancement : c'est ce qui sert à ouvrir une étape dans une nouvelle fenêtre.

## Points faibles connus de l'architecture

- **`main.rs` et `model3d.rs` sont très gros** (8 400 et 11 500 lignes). L'interface et la logique y sont mêlées.
- **L'ouverture d'un fichier est une longue suite de `if`.** Ajouter un éditeur demande de toucher plusieurs endroits de `main.rs`.
- **Pas d'historique global.** Chaque éditeur a son propre annuler.
- **Le README de la racine est en retard** sur le code : il annonce la version 0.19 et ne cite pas la moitié des modules.
- **La vue 3D est sur le processeur.** Le passage à la carte graphique est le premier chantier de la [feuille de route](feuille-de-route-moteur.md).
