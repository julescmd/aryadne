# Ajouter un nœud, un solveur, un format, un mode, une icône

Des recettes, fichier par fichier. Elles décrivent le code tel qu'il est organisé aujourd'hui.

Règles communes :

- les textes visibles sont en **français** dans le code, et passent par `i18n` (voir [Traductions](traductions.md)) ;
- les commentaires sont en français ;
- un **test** accompagne la logique ;
- `cargo build` puis `cargo test` doivent passer avant un commit.

## Ajouter un nœud

### 1. La définition

Dans `src/nodes.rs`, ajoute une ligne à `DEFS` :

```rust
Def {
    op: "monflou",                                  // identifiant : écrit dans les fichiers, ne plus le changer
    label: "Mon flou",                              // nom affiché, en français
    cat: "filtre",                                  // catégorie du menu
    inputs: &[("image", Sock::Color, GRAY), ("force", Sock::Float, f(1.0))],
    outputs: &[("image", Sock::Color)],
    params: &[ParamDef::Float("rayon", 0.0, 100.0, 6.0)],
    mat: false, comp: true,                         // matériau ? compositing ?
},
```

Catégories existantes : `entrée`, `motif`, `usure`, `couleur`, `math`, `filtre`, `shader`, `géométrie`, `simulation`, `structure`, `sortie`, `interne`.

Le nœud apparaît tout seul dans le menu d'ajout et dans la bibliothèque de nœuds (la liste à gauche du graphe), avec ses prises et ses réglages. L'éditeur lit tout dans `DEFS`.

### 2. Le calcul

Selon le genre de nœud, il se place à un endroit différent.

| Genre | Où | Repère dans le code |
|---|---|---|
| Valeur calculée en un point (motif, couleur, math) | L'évaluation par point de `nodes.rs` | Le grand `match` sur `op`, près de `"noise" => {` |
| Filtre d'image (compositing) | L'évaluation d'images de `nodes.rs` | Le `match` près de `"blur" => vec![blur(…)]` |
| Géométrie ou simulation | `Run` dans `geo.rs` | Le `match` près de `"geo_grid" => {` |

Un nœud calculé en un point est utilisable dans un matériau, un compositing et comme **champ** dans un graphe de géométrie, si `mat` et `comp` sont vrais tous les deux.

Lis les réglages avec les fonctions d'aide déjà là : `pnum(&s.params, "rayon", 6.0)` pour un nombre, `pstr(&s.params, "mode")` pour un choix.

**Les choix se comparent au texte français** : `match pstr(n, "plan") { "mur (XY)" => …, _ => … }`. C'est ce texte qui est écrit dans les fichiers.

### 3. Un nœud qui dépend du temps

Si le résultat change d'une image à l'autre, il doit le déclarer, sinon le cache le croira constant. Regarde comment le nœud `time` est traité (`uses_time` dans `nodes.rs`) et comment la clé de cache de `geo.rs` inclut l'image.

Même règle pour un nœud qui lit la scène 3D : `geo_scene_obj` met la boîte de l'objet dans l'empreinte du champ (`Run::source_hash`) et toute sa trajectoire dans la clé de la zone de simulation. Voir [Dans une scène 3D](noeuds.md#dans-une-scène-3d-geoscene).

### 4. Les traductions

Ajoute à `assets/i18n/en.json` : le libellé, la catégorie si elle est nouvelle, chaque nom de prise, chaque nom de réglage, chaque choix.

### 5. Le test

Un test qui construit un petit graphe et vérifie la sortie :

```rust
#[test]
fn mon_flou() {
    let mut g = Graph::empty(GraphKind::Compositor);
    let n = g.add("monflou", [0.0, 0.0]);
    // … relier, évaluer, vérifier une valeur
}
```

`Graph::add` remplit les valeurs et les réglages par défaut d'après la définition. Il panique si l'`op` n'existe pas : c'est déjà un premier test.

### Cas particuliers

- **Un nœud à prises variables** (groupe, zone) : voir `nodeflow.rs`, `is_dynamic` et `node_def`.
- **Un nœud réservé à un genre de graphe** : `Def::fits` décide. Un nœud de catégorie `géométrie` ou `simulation` va dans un graphe de géométrie.
- **Un générateur de masque pour l'Atelier** : ajoute aussi une entrée à `atelier::MASKS` (identifiant, nom affiché, `op` du nœud).

## Ajouter un solveur

Un solveur fait avancer un état d'un pas de temps.

1. Dans `src/geo.rs`, écris une structure vide et implémente le trait :
   ```rust
   struct MonSolveur;
   impl Solver for MonSolveur {
       fn name(&self) -> &'static str { SOLVER_NAMES[2] }
       fn step(&self, g: &mut Geo, dt: f32, p: &SolverParams) {
           // lit g.vec3("P"), g.vec3("v"), g.col("pin")…
           // écrit g.set("P", Column::new(…)), g.set("v", …)
       }
   }
   ```
2. Inscris-le dans `solvers()` et ajoute son nom à `SOLVER_NAMES`. Les deux listes doivent rester dans le même ordre.
3. S'il lit des réglages que les autres n'ont pas : ajoute-les à `SolverParams`, comme entrées du nœud `sim_solver` dans `DEFS`, et dans la lecture des entrées (le cas `"sim_solver"` de `geo.rs`).
4. Mets à jour `solver_uses` : il dit quelles entrées du nœud ton solveur lit. Les autres sont montrées en retrait.
5. Ajoute ses réglages au menu de l'aperçu (`geoview.rs`).
6. Traduis le nom et les réglages.
7. Écris un test : un petit état, quelques pas, une propriété vérifiée (rien n'explose, l'énergie baisse, un point fixe ne bouge pas).

Pour un solveur sur carte graphique, suis d'abord [Écrire un nouveau solveur sur la carte](simulation-gpu.md#écrire-un-nouveau-solveur-sur-la-carte). Le modèle à suivre est `VbdCloth`.

**Attention aux attributs.** Un solveur ne doit pas supposer que `v` ou `pin` existent : `g.vec3("v")` rend des zéros s'il manque, `g.col("pin")` rend `None`.

## Ajouter un format de fichier

### Un format propre à Aryadne

1. **La table.** Dans `src/formats.rs`, ajoute une ligne à `ALL` et augmente la taille du tableau :
   ```rust
   Format { id: "truc", ext: "arytruc", legacy: &[], label: "truc", help: "Ce que contient le fichier", ready: true },
   ```
   L'extension doit commencer par `ary` et ne pas contenir de point. Un test le vérifie.
   
   Avec `ready: true`, le format apparaît dans la fenêtre « nouveau fichier ». Les filtres et la conversion des anciens noms suivent.

2. **La structure.** Écris la structure de données avec `serde` :
   ```rust
   #[derive(Serialize, Deserialize, Clone, Default)]
   pub struct Truc {
       #[serde(default)]
       pub version: u32,
       #[serde(default)]
       pub items: Vec<Item>,
   }
   ```
   Mets `#[serde(default)]` sur chaque champ : un vieux fichier restera lisible quand tu ajouteras des champs. Écris avec `crate::vault::atomic_write`.

3. **L'éditeur.** Voir « Ajouter un mode » ci-dessous.

4. **La création.** Dans `src/extras.rs` : une variante de `Cmd` (`NewTruc`), une entrée dans la liste des commandes de la palette, et un cas dans l'exécution :
   ```rust
   Cmd::NewTruc => self.new_untitled(crate::formats::ext("truc"), |p| Truc::default().save(p)),
   ```
   Dans `src/main.rs` : une entrée dans le menu `File › New`.

5. **L'étape.** Dans `src/workspace.rs`, `owner(rel)` dit dans quelle étape le fichier s'ouvre. Ajoute ton format au bon endroit.

6. **L'icône du fichier.** Dans `src/files.rs`, `kind(rel)` rend l'icône, la couleur et le filtre d'un fichier.

7. **Traductions** du `label` et du `help`.

8. **Un test** d'aller-retour : enregistrer, relire, comparer.

### Un format étranger à lire (import)

Exemples à suivre : `formats3d.rs` (3DS, PLY, Maya ASCII), `midi.rs`, `psd.rs`, `canvas.rs`.

1. Écris une fonction pure : des octets vers la structure d'Aryadne. Pas d'interface dedans.
2. Fais-la appeler par le chargeur du genre. Pour un modèle 3D : ajoute l'extension à `model3d::EXT` et un cas dans le chargement.
3. Un test avec un petit fichier construit dans le test lui-même.

**Attention.** `model3d::EXT` contient `mb` et `max`, alors qu'aucun lecteur n'existe pour ces deux formats dans le code lu. N'ajoute une extension que si le lecteur est là.

### Un format étranger à écrire (export)

Exemples : `fbxwrite.rs`, `fbxscene.rs`, `gltfscene.rs`, `lottie.rs`, `mdexport.rs`. Même principe : une fonction pure qui rend des octets ou écrit un chemin, appelée depuis le menu d'export de l'éditeur.

## Ajouter un mode

Un « mode » peut vouloir dire trois choses.

### Un éditeur pour un nouveau genre de fichier

1. **Le module.** `src/truc_ui.rs`, avec une structure `Editor` :
   ```rust
   pub struct Editor { /* le document, la sélection, l'annuler */ }
   impl Editor {
       pub fn open(path: PathBuf, root: &str) -> Result<Editor, String> { … }
       pub fn show(&mut self, ui: &mut egui::Ui) { … }
       pub fn save(&mut self) -> Result<(), String> { … }
   }
   ```
   Déclare-le dans `main.rs` : `mod truc_ui;`.

2. **Le contenu.** Dans `main.rs`, ajoute une variante à `Content` : `Truc(Box<truc_ui::Editor>)`.

3. **L'ouverture.** Dans `App::open`, ajoute un cas à la suite des autres :
   ```rust
   } else if truc::is_truc(&rel) {
       match truc_ui::Editor::open(path, &self.root()) {
           Ok(e) => Content::Truc(Box::new(e)),
           Err(e) => Content::Unreadable(e),
       }
   ```

4. **Le dessin.** Le compilateur te montrera chaque `match` sur `Content` qui n'est plus complet : dessin, enregistrement, état « modifié », aide de la barre du bas. Suis ses erreurs une à une. C'est le moyen le plus sûr de ne rien oublier.

5. **Les panneaux.** Pour que les panneaux de ton éditeur soient ancrables et détachables, dessine-les à travers `dock` : `dock::panel`, `dock::area`, `dock::strip`, `dock::window`. Donne à chacun un identifiant stable et un titre du genre « calques (truc) ».

6. **Les raccourcis.** Lis-les dans ton `show`, seulement quand la souris est sur l'éditeur et qu'aucun champ de texte n'a le focus (`ctx.egui_wants_keyboard_input()`). Si un de tes raccourcis recouvre un raccourci global, regarde `keymap::pressed` : c'est là que la vue 3D prend la priorité.

7. **L'aide.** Ajoute la ligne de raccourcis de ton éditeur à la barre du bas (dans `main.rs`, à côté des autres lignes d'aide).

### Un outil dans une étape

Dans `src/workspace.rs`, `tools(kind)` liste les outils de chaque étape : identifiant, nom, icône, page ouverte. Ajoute une entrée au tableau de l'étape. Les raccourcis `Alt+1`, `Alt+2`… suivent l'ordre du tableau.

Une nouvelle **étape** demande une ligne dans `PRESETS` (nom, couleur, page, genre), un cas dans `tools`, `pages` et `owner`, et une couleur dans les thèmes (`theme.rs`).

### Un mode dans la vue 3D

Les modes de la vue 3D (objet, édition, sculpture, peinture, UV) vivent dans `model3d.rs`. Chaque mode est un état optionnel du `Viewer` : `self.mesh` pour l'édition, `self.sculpt`, `self.edit` pour les UV.

1. Ajoute ton état (`self.truc: Option<TrucMode>`).
2. Ajoute l'entrée au sélecteur de mode de la barre (près des libellés « mode objet », « mode édition »).
3. Dans la gestion de la souris et du clavier, fais passer ton mode avant les autres quand il est actif. Regarde comment la sculpture « remplace la sélection et les outils du mode édition ».
4. Ajoute sa ligne d'aide dans la barre du bas.

`model3d.rs` est le fichier le plus chargé du projet. Mets la logique de ton mode dans son propre module (comme `sculpt.rs`, `paint3d.rs`, `uvedit.rs`) et ne garde dans `model3d.rs` que le branchement.

## Ajouter une icône

Trois systèmes d'icônes coexistent.

### Une icône Lucide (le cas courant)

Les icônes Lucide sont une police (`assets/fonts/Lucide.ttf`, licence ISC). Elle est déclarée en secours de chaque famille de police : une icône s'écrit donc **dans n'importe quel texte**.

```rust
ui.button(format!("{}  {}", crate::lucide::PLUS, crate::i18n::t("ajouter")));
```

`src/lucide.rs` contient une constante par icône utilisée :

```rust
pub const FOLDER: &str = "\u{f00d7}";
```

Pour en ajouter une :

1. Trouve son nom sur le site de Lucide (par exemple `flip-horizontal-2`) et son code dans la police.
2. **Décale le code.** La police Inter occupe déjà la zone privée U+E000. Les codes Lucide sont donc déplacés vers U+F0000 : nouveau code = `0xF0000 + (code d'origine − 0xE000)`.
3. Ajoute la constante, en majuscules, les tirets remplacés par des tirets bas :
   ```rust
   pub const FLIP_HORIZONTAL_2: &str = "\u{f0xxx}";
   ```

`src/lucide_all.rs` contient déjà **toutes** les icônes Lucide, nom et caractère, pour la bibliothèque d'icônes du vectoriel. Le code décalé de n'importe quelle icône s'y lit directement : cherche son nom dans ce fichier et recopie le caractère.

Pendant le développement, un script `addicon.py` faisait ce calcul. Il n'est pas dans le dépôt et écrit dans un chemin fixe : à relire avant de s'en servir.

### Une icône dessinée

`src/icons.rs` : des icônes tracées en code (traits fins, une seule couleur, même grille). Elles servent là où un dessin sur mesure est préférable, par exemple les matériaux de la vue.

1. Ajoute une variante à l'énumération `I`.
2. Ajoute son tracé dans `icons::draw(painter, rect, icône, couleur)`.

Le montage a aussi quelques icônes propres, dessinées de la même façon dans `montage_ui.rs`.

### La règle d'interface

Icônes plutôt que boutons de texte, avec le texte au survol :

```rust
ui.button(crate::lucide::TRASH_2).on_hover_text(crate::i18n::t("supprimer"));
```

## Ajouter un réglage

1. Un champ dans `settings::Prefs` (`src/settings.rs`), avec `#[serde(default)]` ou une fonction de valeur par défaut.
2. Sa ligne dans l'onglet voulu de la fenêtre de réglages.
3. Sa traduction.

Les préférences sont lues au lancement (`Prefs::load`) et écrites dans `settings.json`. Un réglage lu très souvent (l'unité 3D, la navigation) est recopié dans une variable globale par une fonction comme `settings::set_unit`.

## Ajouter un raccourci global

Dans `src/keymap.rs`, ajoute une entrée à `ACTIONS` :

```rust
Action { id: "truc", label: "faire un truc", default: &[(C, Key::T)] },
```

Puis lis-le là où l'action se déclenche : `if keymap::pressed(ctx, "truc") { … }`.

Il apparaît tout seul dans `Settings › Shortcuts`, modifiable, avec détection des conflits. Traduis son `label`.

## Ajouter une commande à la palette

Dans `src/extras.rs` : une variante de `Cmd`, une entrée dans la liste des commandes (texte français, raccourci affiché, commande), un cas dans l'exécution. La recherche floue et la recherche en anglais suivent.

## Avant de pousser

- `cargo build` sans erreur.
- `cargo test` sans échec.
- Chaque texte visible a sa traduction.
- Un fichier créé par l'ancienne version s'ouvre toujours.
- Aucun code repris d'un logiciel sous licence GPL.
