# Système de nœuds

Quatre modules.

| Module | Rôle |
|---|---|
| `nodes` | Les définitions des nœuds, le graphe, l'évaluation des matériaux et des images, l'éditeur |
| `nodeflow` | Ce qui fait d'un graphe plus qu'une liste de nœuds : prises sur mesure, groupes, zones, renvois, nœuds muets. Et la **mise à plat** |
| `geo` | Le moteur de géométrie et de simulation |
| `atelier` | La pile de calques, ramenée à un graphe |

Trois genres de graphes (`GraphKind`) : `Material`, `Compositor`, `Geometry`. Un seul éditeur les dessine tous.

## Le modèle de données

Voir la structure sur disque dans [Formats](formats.md#graphes-de-nœuds-arymat-arycomp-arygeo).

- `Graph` : `nodes`, `links`, `frames`, `groups`, `stack`, `time`.
- `Node` : `id`, `op`, `pos`, `values` (entrées non reliées), `params` (réglages), `ins` et `outs` (prises sur mesure).
- `Link` : `from`, `out`, `to`, `inp`.

### Une seule valeur : quatre nombres

Dans un matériau ou une image, **toutes les valeurs voyagent en quatre nombres** (`type V = [f32; 4]`). Un nombre seul est répété quatre fois. Un vecteur occupe trois cases.

Le type d'une prise (`Sock`) sert à l'artiste : couleur de la prise, réglage proposé, refus des liens absurdes. Il ne change pas le calcul.

| `Sock` | Sens |
|---|---|
| `Float`, `Int`, `Bool` | Un nombre |
| `Vector` | Trois nombres |
| `Color` | Quatre nombres |
| `Geo` | Une géométrie. Ne se branche que sur une géométrie |
| `Any` | Prise libre (renvoi, prise à créer) : prend le type de ce qu'on y branche |

### La table des définitions

`nodes::DEFS` est un tableau de `Def`. Un nœud y est décrit en une ligne :

```rust
Def {
    op: "blur",                 // identifiant, écrit dans les fichiers
    label: "Flou",              // nom affiché (français, traduit par i18n)
    cat: "filtre",              // catégorie du menu
    inputs: &[("image", Sock::Color, GRAY)],
    outputs: &[("image", Sock::Color)],
    params: &[ParamDef::Float("rayon", 0.0, 100.0, 6.0)],
    mat: false, comp: true,     // où il a sa place
}
```

`ParamDef` : `Choice` (liste), `Float` (curseur), `Text`, `File`, `Ramp` (dégradé), `Curve` (courbes), `Line` (une ligne de texte).

`Def::fits(kind)` dit si un nœud a sa place dans un genre de graphe. Un graphe de géométrie prend ses propres nœuds, plus tous ceux qui calculent une valeur en un point sans lire d'image : ce sont les **champs**.

## La mise à plat (`nodeflow::lower`)

Groupes, zones de répétition, renvois et nœuds muets sont des commodités d'écriture. Avant tout calcul, `lower` ramène le graphe à sa forme la plus simple. Les moteurs ne voient que des nœuds ordinaires.

`lower(g, pixel)` rend un `Lowered` : le graphe mis à plat, et une table `origin` qui dit de quel nœud du graphe de départ vient chaque nœud. Une erreur trouvée dans le graphe mis à plat est ainsi **rapportée sur le bon nœud** à l'écran.

Si le graphe ne contient rien de spécial, il est rendu tel quel, sans copie inutile.

Sinon, dans l'ordre :

1. **Groupes.** Chaque nœud `group` est remplacé par le contenu de son groupe, jusqu'à ce qu'il n'en reste plus. Les groupes dans les groupes sont dépliés à leur tour. Garde-fou : plus de 4 000 dépliages ou 20 000 nœuds donne l'erreur « trop de groupes imbriqués ». Une chaîne de noms détecte un groupe qui se contient lui-même.
2. **Nœuds muets et renvois.** Ils laissent passer leur première entrée : leurs liens de sortie sont rebranchés sur ce qui les nourrissait, puis ils sont retirés. Un nœud de sortie ou un bout de zone ne peut pas être muet.
3. **Zones.** La zone la plus intérieure d'abord.
   - **Répétition** : le contenu est **déroulé**, copié autant de fois qu'il y a d'itérations, chaque copie branchée sur la précédente. Le nombre d'itérations doit donc être fixe. Un graphe trop gros est refusé.
   - **Simulation** : avec `pixel = true` (matériau, image), elle est refusée : elle n'a pas de sens point par point. Avec `pixel = false` (géométrie), elle est **gardée**. Le moteur de géométrie l'exécute image après image.

Règles vérifiées : une zone de simulation ne peut pas être dans une répétition, les zones ne se croisent pas, un début sans fin ne fait rien.

## Évaluation d'un matériau ou d'une image

`Program::compile(g, root, max_img)` compile le graphe mis à plat :

- les nœuds sont rangés **dans l'ordre d'exécution** ;
- chaque entrée est résolue : une constante, ou la sortie d'un nœud précédent ;
- les ressources sont préparées une fois : images chargées, dégradés lus, courbes changées en tables de 256 valeurs, formules compilées ;
- deux drapeaux notent ce dont le graphe a besoin : la courbure (le rendu ne la calcule que si un nœud la lit) et le relief.

### Matériau

Le programme est exécuté **par point de surface**. Le contexte `Ctx` donne la position, la normale, les UV, la courbure et les masques peints. Le résultat est une `Surf` : couleur, rugosité, métal, émission, transmission, vernis, bandes toon, reflets, occlusion, relief, velours, sous-surface.

La vue 3D et le tracé de chemins appellent ce même code. C'est ce qui garantit qu'un matériau a le même aspect dans les deux.

### Image (compositing)

Le programme produit des images entières. Chaque nœud porte une **empreinte** : son type, ses réglages, et l'empreinte de ce qui y entre. Deux nœuds de même empreinte donnent la même image. Le compositing reprend donc du cache tout ce qui n'a pas changé : modifier le dernier nœud d'une chaîne ne recalcule pas les flous en amont.

### Le langage des formules

`Shader::compile` (nœud `Custom shader`) et `Shader::compile_vars` (nœud `Expression`) compilent un petit langage en un arbre `Expr` : nombres, variables, opérations, appels de fonctions. L'arbre est ensuite évalué pour chaque point. Les variables et les fonctions sont listées dans le [guide](../utilisateur/noeuds-editeur.md#expressions).

## L'Atelier : pile = graphe

Un matériau à calques est un `Graph` dont le champ `stack` est rempli. `Graph::effective()` rend alors le graphe **tiré de la pile** par `atelier::flatten`.

`flatten(stack, root)` construit un graphe de matériau :

1. Un nœud `mat_out`, avec ses valeurs par défaut comme point de départ de chaque canal.
2. Pour chaque calque visible, du bas vers le haut :
   - le graphe du maître est chargé et copié, ses numéros de nœuds décalés ;
   - chaque nœud `param` reçoit la valeur que le calque lui donne (le nom du paramètre est le `label` du nœud) ;
   - un **générateur de masque** est ajouté selon `mask.kind` (`edgewear`, `cavdirt`, `dust`, `drips`, `scratches`, `grunge`, `noise`, `cracks`, `voronoi`, `paintmask`), puis la cassure éventuelle ;
   - pour chaque canal de la sortie, un nœud de mélange combine ce qu'il y avait dessous et ce que donne le calque, avec le masque et l'opacité comme facteur.
3. Le dernier mélange de chaque canal est branché sur `mat_out`.

Tout le reste (vue 3D, rendu, éditeur de nœuds) ne voit qu'un graphe ordinaire. Il n'y a pas de second moteur pour les calques.

#### Calques « graphe », calques liés, masques en nœuds

Un calque (`atelier::Layer`) tire son graphe de l'un de deux endroits :

- `graph` : son propre graphe, rangé dans le fichier du matériau (calque « graphe ») ;
- `master` : le chemin d'un `.arymat` du coffre (calque lié), relu quand le fichier change.

`Layer::fill` rend l'un ou l'autre. `flatten` le copie par `splice`, qui déplie d'abord les groupes (`nodeflow::lower`) : un calque peut donc contenir des groupes.

Un masque de genre `graphe` porte lui aussi un graphe (`Mask::graph`). `flatten` y lit ce qui entre dans la **couleur** de sa sortie, et s'en sert comme facteur. `atelier::mask_graph` écrit ce graphe à partir d'un masque simple, quand on l'ouvre en nœuds pour la première fois.

#### Un matériau s'édite toujours en pile

- `atelier::to_stack` : à l'ouverture dans l'éditeur, un matériau sans pile (un ancien fichier) devient une pile d'un seul calque « graphe » qui porte tout son graphe.
- `atelier::to_file` : à l'écriture (`Graph::save`), une pile d'un seul calque « graphe » nu redevient un simple graphe. Un ancien fichier se relit et se réécrit sans changer de forme.
- `Graph::load` ne change rien : la vue 3D et le rendu lisent les deux formes, par `Graph::effective`.

#### Entrer dans un calque (`Editor::inside`)

Dans l'éditeur, `inside` dit si l'on édite le graphe d'un calque. Pendant ce temps `Editor::graph` **est** ce graphe (un graphe ordinaire, sans pile), et le matériau entier attend dans `Inside::outer` avec son historique d'annulation. Tout le code du graphe marche donc sans cas particulier.

- `enter_layer(calque, Part::Fill | Part::Mask)` échange les deux graphes. Pour un calque lié, le graphe est relu du fichier du maître.
- `leave_layer` remet le graphe dans son calque (`put_inner`), ou l'écrit dans le fichier du maître, puis pose un seul point d'annulation dans la pile.
- `save_if_due` écrit toujours le matériau entier, même depuis l'intérieur d'un calque.

## Le moteur de géométrie (`geo`)

Trois idées tiennent tout le reste. Elles sont écrites en tête de `src/geo.rs`.

### 1. Données en colonnes

Une géométrie (`Geo`) est une table.

| Champ | Contenu |
|---|---|
| `n` | Nombre de points |
| `cols` | Attributs des points : nom vers `Arc<Column>` |
| `edges` | Arêtes : paires de numéros de points |
| `ecols` | Attributs des arêtes (longueur au repos…) |

Une `Column` contient **un tableau par composante** (`data: Vec<Vec<f32>>`) : tous les x, tous les y, tous les z. Et un numéro de `version`, qui change à chaque écriture.

Pourquoi des colonnes :

- un nœud qui ne touche que la position ne copie ni ne relit les autres colonnes. Elles sont **partagées** (`Arc`) entre l'entrée et la sortie ;
- c'est la disposition qu'attend un solveur sur carte graphique : un tampon par attribut ;
- comparer deux colonnes se fait par leur numéro de version, sans relire les données.

`Geo::stamp()` est l'empreinte d'une géométrie : elle change dès qu'une colonne, le nombre de points ou les arêtes changent.

### 2. Calcul à la demande, incrémental

Rien ne se calcule tant que la sortie ne le demande pas. `Engine::eval` part du nœud de sortie et remonte.

Chaque résultat est rangé dans `Engine::memo` sous une **clé** : le nœud, ses réglages, et l'empreinte de ce qui y entre (les géométries par leur empreinte, les champs par la leur).

Deux finesses :

- **L'image de l'animation n'entre dans la clé que si le nœud en dépend.** Une grille fixe n'est calculée qu'une fois pour toute l'animation.
- **Coupure anticipée.** Quand un nœud est refait et donne les mêmes colonnes qu'avant, elles **gardent leur numéro de version** (`Geo::adopt`). L'aval reste donc valide et n'est pas refait. `Stats::cutoffs` compte ces cas.

`Engine::stats` rend compte de chaque calcul : nœuds `computed`, résultats `reused`, `cutoffs`, pas de simulation (`steps`). L'aperçu l'affiche à l'artiste.

#### Le cache et son budget

`Engine::budget` : 512 Mo par défaut. Quand le cache est plein, la règle **GreedyDual-Size** choisit qui part : chaque entrée a une priorité, son coût de calcul divisé par sa taille. Ce qui est le moins cher à refaire par octet s'en va en premier.

### 3. Simulation = état qui passe d'une image à l'autre

Une zone de simulation garde un état par image.

`sim_state(end)` calcule l'état de la zone à l'image courante :

1. **Cache figé.** Si la zone est figée sur le disque (`Engine::baked`), le fichier est relu tel quel, sans regarder l'amont.
2. **État de départ** : ce qui entre dans la zone à la première image.
3. **Clé de la zone** : tout ce qui est en amont de sa fin (nœuds, réglages, liens) et l'état de départ. Toucher à quoi que ce soit là-dedans invalide ses images. Toucher à l'aval ne les invalide pas.
4. **Dernier état connu** avant l'image demandée. Le calcul repart de là, image par image.

Pendant l'exécution du contenu de la zone, `Run::env` porte l'état de l'image d'avant : le nœud de début le rend à ses sorties.

Le pas de temps d'une zone est le même à chaque image. Il est écrit en dur dans le graphe avant l'évaluation : les champs le lisent comme une constante.

#### Rester fluide

`Engine::deadline` est une heure limite. Passé ce moment, une simulation rend l'image qu'elle a atteinte et `Engine::incomplete` passe à vrai. L'interface redemande à l'image suivante. L'aperçu donne 30 ms au calcul principal et 4 ms aux miniatures.

Une miniature ne lance jamais une simulation : elle prend ce qui est déjà en cache.

#### Figer sur le disque

`Engine::bake` calcule toutes les images, de la première à la dernière, et écrit un fichier par image et par prise. Format : voir [Formats](formats.md#cache-de-géométrie-arygeocache).

### Les champs

Une entrée de nombre, de vecteur ou de couleur d'un nœud de géométrie est un champ : `Field::Const(valeur)` ou `Field::Per(une valeur par point)`.

`Run::field` compile la chaîne de nœuds en amont de l'entrée (`Program::compile_field`), puis l'exécute **point par point**. Les attributs de la géométrie y sont lisibles par leur nom. Un champ est donc évalué par le même code que les matériaux.

### Le contrat `Solver`

```rust
pub trait Solver: Send + Sync {
    /// Nom montré dans le node « Solveur »
    fn name(&self) -> &'static str;
    fn step(&self, state: &mut Geo, dt: f32, p: &SolverParams);
}
```

Un solveur fait avancer un état d'un pas de temps. Il reçoit l'état **en colonnes** et le modifie sur place. C'est le même contrat pour un solveur sur le processeur et sur la carte graphique, où les colonnes deviennent des tampons.

`SolverParams` : `iterations`, `compliance`, `damping`, `floor`, `ball`, `cube`, `grip` (le frottement), et pour le tissu sur carte graphique `stiffness`, `bending`, `substeps`, `shear`, `calm`, `thickness`.

Deux solveurs sont inscrits dans `geo::solvers()` :

| Solveur | Nom affiché | Fichier |
|---|---|---|
| `XpbdCloth` | tissu et fils, XPBD (processeur) | `geo.rs` |
| `VbdCloth` | tissu (carte graphique) | `geo.rs`, qui appelle `gpucloth.rs` |

`XpbdCloth` est volontairement petit et lisible. Il sert de référence et d'exemple. Il lit `P`, `v`, `pin`, et l'attribut d'arête `rest`. Il coupe le pas en sous-pas, et traite les obstacles comme des contraintes dans la même boucle.

`VbdCloth` garde quelques tissus prêts (topologie rangée et coloriée, tampons sur la carte). Le plus ancien cède sa place. Si les colonnes `P` et `v` reviennent telles qu'il les a rendues au dernier pas, il ne renvoie rien à la carte. Voir [Simulation sur carte graphique](simulation-gpu.md).

`solver_uses(solveur, entrée)` dit quelles entrées du nœud un solveur lit. Les autres sont montrées en retrait.

## L'éditeur (`nodes::Editor`)

Il occupe la seconde moitié de `nodes.rs`. Il gère le dessin du graphe, la sélection, les liens, les menus, l'aperçu, l'annuler, le presse-papiers. L'aperçu d'un graphe de géométrie est délégué à `geoview`.

Le presse-papiers passe par celui du système, en texte : des nœuds se collent d'un graphe à un autre, et d'une fenêtre d'Aryadne à une autre. Les groupes dont ils dépendent voyagent avec eux.

### La bibliothèque de nœuds

Le panneau de gauche (`dock::area("node-lib", …)`) liste ce que rend `Editor::picks` : les mêmes choix que le menu d'ajout. Une recherche passe par `search_picks`. Les catégories repliées sont gardées dans `LIB_CLOSED`, commun à tous les éditeurs.

Glisser une rangée remplit `Editor::lib_drag`. Tant qu'il est rempli, `show` dessine le fantôme sous la souris et cherche le lien survolé :

- `link_at` : le lien qui passe sous un point du graphe (mêmes courbes que le dessin, voir `port_pos` et `link_curve`) ;
- `link_fit` : l'entrée et la sortie du nœud qui vont avec les types du lien, ou rien ;
- `insert_on_link` : coupe le lien et le fait passer par le nœud.

Au relâchement, `add_pick` pose le nœud, puis `insert_on_link` l'insère si un lien était visé.

## L'assistant et les graphes (`aigraph`)

Devant un graphe, l'assistant reçoit de quoi le comprendre et un moyen d'agir. Tout est dans `src/aigraph.rs`, sans interface : des fonctions de `Graph` vers du texte, et de texte vers `Graph`.

### Ce qui part au modèle

`Editor::ai_context(rel)` appelle `aigraph::context`, ajouté au message système par `chat::build_messages` :

1. `notice(kind, stack, rel)` : le format des blocs, puis ce qu'il faut savoir du genre de graphe (zone de simulation, solveur, calques). Elle contient un exemple complet, le drapeau, écrit avec le chemin du document ouvert.
2. `describe(g, rel, level)` : le niveau affiché, un nœud par ligne (`n<id> type @x,y : ce qui s'écarte des valeurs par défaut`), les liens par nom de prise, les groupes, les cadres. Une pile de calques est décrite calque par calque.
3. `catalog(kind)` : une ligne par nœud de `DEFS` qui convient à ce genre (`Def::fits`), avec les prises typées, les valeurs par défaut et les choix permis.

Le catalogue est tiré des tables de définitions : un nœud ajouté à `DEFS` y apparaît sans rien faire d'autre. Un test borne sa taille (environ 16 000 caractères pour la géométrie). Pour Ollama, `chat::ollama_context` demande une fenêtre de contexte assez grande, sans quoi le début de la demande serait coupé.

### Le format : un bloc de texte, pas des outils

Le modèle écrit un bloc ` ```graphe ` contenant du JSON : `{"fichier": …, "remplacer": false, "ops": [ … ]}`. Les opérations : `ajouter`, `relier`, `delier`, `regler`, `supprimer`, `zone`, `grouper`, `degrouper`, `cadre`, `lecture`, et pour la pile de l'Atelier `ajouter_calque`, `regler_calque`, `supprimer_calque`, `deplacer_calque`.

Pourquoi pas les outils (« tool use », « function calling ») des API :

- `ai.rs` parle à trois familles d'API : Ollama en natif, Claude en natif, et le format OpenAI pour les autres (y compris une adresse personnalisée). Chacune a son format d'outils et son format de flux. Beaucoup de modèles locaux, et certains serveurs compatibles, ne gèrent pas les outils : Ollama répond alors par une erreur.
- Le bloc de texte marche partout, y compris en flux, et suit la convention déjà en place pour les fichiers (` ```fichier:chemin `).
- L'application demande un clic : il n'y a pas de résultat d'outil à renvoyer au modèle dans le même tour. La boucle de correction passe par un message ordinaire (`Ask for a fix`).

La lecture est tolérante : accents et majuscules indifférents dans les clés, virgule en trop, commentaires `//`, liste nue, bloc ` ```json ` qui a la forme d'un changement. Un bloc non refermé (réponse en cours) est ignoré et caché.

### Vérifier et appliquer

`aigraph::apply(&mut Graph, &Edit, root)` travaille sur une copie. Chaque opération est vérifiée contre le catalogue : type connu et permis dans ce genre de graphe, prise et réglage existants, valeur du bon type, choix dans la liste, types de prises compatibles (`nodes::socks_fit`), pas de boucle. Les erreurs sont toutes relevées, avec le rang de l'opération. S'il y en a une, la copie est jetée : rien n'est appliqué à moitié.

Les références : `n<id>` pour un nœud existant, l'`id` donné par le bloc pour un nœud qu'il pose, `sortie` pour le nœud de sortie, ou un type présent une seule fois. Une prise s'écrit `noeud.prise`, par son nom.

`Work::layout` range ensuite les nœuds posés sans position : chacun à droite de ce qui l'alimente, glissé vers le bas tant qu'il recouvre un autre nœud (mêmes tailles que le dessin, `Editor::node_size`).

Côté éditeur :

- `Editor::ai_check` essaie le bloc sur une copie et rend le résumé ou les erreurs. Si le graphe se calculait, il doit encore se calculer après (`nodeflow::lower`, ou `Program::compile_at` en petite taille). La carte garde ce résultat tant que `ai_version()` ne change pas.
- `Editor::ai_apply` remplace `graph` puis appelle `changed()` : un seul point d'annulation.

Le bloc s'applique au niveau en cours d'édition (`Editor::graph`) : la racine, l'intérieur d'un groupe, le graphe d'un calque, ou la pile. Dans la pile, les opérations de nœuds visent un calque (`"calque": "c2"`, `"partie": "masque"`).

### L'interface

`App::assistant_ui` (`main.rs`) envoie le contexte, cache le corps des blocs dans la réponse et dessine `extras::graph_card` sous le message. `App::apply_ai_graph` applique au graphe ouvert. L'option d'application automatique est `ai::Settings::auto_graph` (`ai.json`).

### Essayer sans fournisseur

`ARY_AI_FAKE=/chemin/reponse.txt` fait « répondre » le contenu du fichier. `ARY_AI_DUMP=/chemin/messages.json` écrit ce qui serait envoyé. Les tests de `aigraph.rs` jouent la réponse « drapeau » de la notice : ils vérifient le graphe obtenu (grille, épinglage, zone, solveur sur carte graphique, vent, sortie), puis le simulent avec le solveur de référence.

### Ajouter une opération

Dans `Work::op` pour les nœuds, `layer_op` pour les calques : lire les champs avec `get` (qui accepte plusieurs noms), rendre une erreur claire, pousser une ligne dans `done`. Décrire l'opération dans `notice` et ajouter un cas au test.

## Tests

Les tests de `geo.rs` vérifient notamment que seul le nécessaire est refait. Exemple lu dans le code : après un déplacement refait à l'identique, `(computed, cutoffs) == (1, 1)`, c'est-à-dire que la couleur posée en aval vient du cache.

Pour ajouter un nœud, voir [Ajouter un nœud](ajouter.md#ajouter-un-nœud).
