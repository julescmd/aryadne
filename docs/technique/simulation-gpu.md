# Simulation sur carte graphique

> **En cours de refonte.** Le tissu sur carte graphique est en cours d'optimisation au moment où cette page est écrite. Le socle (`gpusim`) et le contrat des solveurs sont stables. L'intérieur de `gpucloth` (rangement, noyaux, relecture) peut changer.

Trois modules.

| Module | Rôle |
|---|---|
| `gpusim` | Le socle : tout ce que les solveurs partagent, et rien d'autre |
| `gpucloth` | Le tissu en Vertex Block Descent, avec sa version processeur |
| `gpubench` | Le banc de mesure |

La bibliothèque est **wgpu** : la même interface pour Vulkan, Metal et DirectX 12. Les noyaux de calcul sont écrits en **WGSL**, dans des chaînes de caractères du code Rust.

## La règle de la maison

**Chaque noyau a sa version processeur, et un test qui compare les deux.**

La version processeur sert trois fois : de repli quand il n'y a pas de carte, de référence dans les tests, et de filet quand la carte rend une erreur en plein calcul.

## Le socle (`gpusim`)

### Une seule carte pour toutes les simulations

`gpu()` ouvre la carte à la première demande et la garde (`OnceLock`). Elle rend `Option<&'static Gpu>` : `None` veut dire « pas de carte utilisable », et chaque solveur passe alors par le processeur.

`Gpu` porte l'appareil, la file de commandes, le nom lisible de la carte, et ses **limites réelles** :

| Champ | Sens |
|---|---|
| `max_binding` | Plus gros tampon qu'un noyau peut brancher, en octets |
| `max_buffer` | Plus gros tampon tout court |
| `max_groups` | Groupes de calcul permis par dimension de lancement |
| `max_storage` | Tampons de stockage permis par noyau |
| `soft` | Rendu logiciel : le code marche, mais plus lentement que le processeur |

À l'ouverture, on demande **tout ce que la carte permet**. Les valeurs par défaut de WebGPU (128 Mo par branchement, 8 tampons par noyau) plafonneraient vers 600 000 sommets.

Une carte est refusée si elle offre moins de 8 tampons de stockage par noyau, ou des groupes de moins de 64 fils.

L'interface d'Aryadne dessine de son côté en OpenGL. Les deux ne se gênent pas.

### Erreurs : ne jamais fermer le logiciel

Une erreur de la carte (pilote, validation, carte perdue) est notée dans un drapeau `broken`. Après cela, `gpu()` rend `None` et tout repasse au processeur. L'ouverture elle-même est protégée contre une panique (`catch_unwind`).

`hardware()` dit si une vraie carte est là (pas un rendu logiciel).

`ARY_SANS_GPU` dans l'environnement force `None` : pour essayer le repli sans débrancher la carte.

### Tampons en colonnes

Un attribut = un tampon. `Column` :

| Champ | Sens |
|---|---|
| `pages` | Un tampon, ou plusieurs si la colonne dépasse ce qu'un noyau peut brancher d'un coup |
| `len` | Nombre d'éléments |
| `stride` | Octets par élément |
| `page_len` | Éléments par page |

Un noyau ne lit que les colonnes dont il a besoin. **C'est la mémoire qui borne la vitesse** : moins d'octets lus et écrits, c'est plus vite.

Les noyaux « élément par élément » se lancent page par page. Ceux qui vont lire leurs voisins n'importe où (le tissu) exigent une seule page : `Column::single()`.

### Les noyaux

`Gpu::kernels(label, wgsl, binds, entries)` compile un module WGSL et ses points d'entrée. `binds` décrit ce qui se branche à chaque place :

| `Bind` | Sens |
|---|---|
| `Uniform` | Réglages, les mêmes pour tout le lancement |
| `Slot(taille)` | Réglages rangés par cases dans un même tampon. Chaque lancement choisit sa case |
| `Read` | Colonne lue |
| `Write` | Colonne lue et écrite |

`Gpu::group` branche des tampons sur un jeu de noyaux. `Gpu::launch` lance un noyau sur un nombre d'éléments.

### Lancement en deux dimensions

Une carte refuse plus de 65 535 groupes par dimension. Avec des groupes de 64 fils (`WG`), une seule dimension plafonne vers 4 millions d'éléments.

`grid(count, max_groups)` répartit sur une dimension tant que ça tient, sur deux au-delà.

`PRELUDE` est un morceau de WGSL à coller en tête d'un noyau. Il fournit `lin(gid, nwg)`, qui rend le numéro d'élément que le lancement soit sur une ou deux dimensions. **Le noyau doit vérifier lui-même** que ce numéro est dans les bornes : le dernier groupe déborde.

### Relecture sans attendre

`Gpu::read(colonne)` rend tout de suite un `Readback`. La copie se fera quand la carte aura fini le travail déjà envoyé.

| Méthode | Usage |
|---|---|
| `ready()` | Les données sont-elles arrivées ? Ne bloque pas. À appeler à chaque image de l'interface |
| `wait()` | Attend la carte et rend les octets |
| `take::<T>()` | Les données, dans leur type |

`Gpu::finish()` attend que la carte ait fini tout ce qui lui a été envoyé.

### Le noyau d'essai

`saxpy_cpu` et `saxpy_gpu` calculent `y ← a·x + y`. C'est le plus petit noyau possible. Il sert à tester le socle : pages, lancement sur deux dimensions, relecture. Ses tests forcent de petites pages et un petit plafond de groupes pour passer par tous les chemins.

## Le tissu (`gpucloth`)

Méthode : **Vertex Block Descent** (Chen, Macklin, Kim, Yuksel, 2024).

### Le principe

Le pas de temps implicite est une minimisation. Pour chaque sommet, on cherche la position qui équilibre son inertie et les forces de ses voisins.

VBD le fait **un sommet à la fois**, par un pas de Newton sur un bloc 3×3, les voisins étant figés. Deux sommets sans lien entre eux peuvent être traités en même temps. On **colorie** donc les sommets au chargement : aucun fil ne relie deux sommets de la même couleur. Chaque couleur est une passe sur la carte.

### Les énergies

| Énergie | Forme | Réglage |
|---|---|---|
| Membrane | Un ressort par arête | `stiffness`, en N/m |
| Flexion | Pour chaque sommet i et chaque paire d'arêtes opposées (i au milieu de j et k) : ½·D·\|(1−t)·xj + t·xk − xi\|² | `bending`, en N·m |

La flexion est nulle quand les trois points sont alignés. Elle est quadratique, donc sa dérivée seconde est constante : c'est la moins chère des flexions pour ce solveur.

### Garder la mémoire sobre

C'est elle qui borne la vitesse. Trois choix :

- **Rangement.** Les sommets sont rangés par couleur, puis le long d'une **courbe de Morton**. Une passe lit une plage contiguë, et les voisins d'un sommet sont proches en mémoire.
- **Voisinage par sommet.** Chaque sommet a sa liste de voisins. Le noyau ne fait que **rassembler** : aucune écriture concurrente. Depuis le commit `064fa57`, ressorts et flexion tiennent dans **une seule liste de 12 octets par entrée** : le voisin, le sommet qui prolonge l'arête, et les deux longueurs au repos sur 16 bits chacune. Les poids de flexion sont recalculés à partir des longueurs. Le message du commit donne, pour une grille avec diagonales : 904 octets lus et écrits par sommet et par passe avant, 468 après.
- **Rangement par grappes** (option). Avec la variable d'environnement `ARY_TISSU_GRAPPES`, les sommets sont rangés par grappes de 64. Une grappe est chargée une fois dans la mémoire partagée du groupe de fils, et ses couleurs y sont résolues. Environ 300 octets par sommet et par passe d'après le même commit. Chaque rangement a sa version processeur et son test.
- **Positions relatives.** Les positions sont relatives au centre du tissu. En `f32`, un tissu posé à 100 m de l'origine garde la précision d'un tissu posé à l'origine.

### La topologie (`Topo`)

Tout ce qui ne dépend que du maillage au repos. Construite une fois, par `Topo::build`. Les mêmes tableaux servent au processeur et à la carte.

| Champ | Contenu |
|---|---|
| `layout` | Rangement : par couleurs, ou par grappes |
| `perm`, `inv` | Rang d'origine du sommet rangé en k, et l'inverse |
| `groups` | Plages traitées l'une après l'autre : début, nombre. Par couleur (les sommets fixes sont alors après la dernière), ou par couleur de grappes |
| `cluster` | Sommets traités d'un bloc : 1, ou la taille d'une grappe |
| `phases` | Nombre de couleurs de sommets |
| `off` | Par sommet : début de ses ressorts, puis leur nombre et sa couleur |
| `springs` | Une entrée par voisin : son rang, le sommet qui prolonge l'arête (pour la flexion), et deux longueurs au repos codées sur 16 bits |
| `rest_scale` | Échelle des longueurs codées : longueur au repos = code² × cette échelle |
| `mass` | Masse. 0 : sommet fixe |

Sans masse fournie, elle est déduite de la taille des mailles, avec `DENSITY = 0.2` kg/m².

### Les réglages (`Params`)

| Champ | Défaut | Sens |
|---|---|---|
| `stiffness` | 1000 | Raideur des arêtes |
| `bending` | 1e-4 | Raideur de flexion |
| `damping` | 0,02 | Part de vitesse perdue par image |
| `iterations` | 16 | Passes sur tous les sommets, par image, réparties entre les sous-pas |
| `substeps` | 8 | Sous-pas par image (1 à 256) |
| `rho` | 0,9 | Rayon spectral supposé pour l'accélération. 0 : sans |
| `floor`, `ball`, `cube` | | Obstacles |
| `grip` | 0,6 | Adhérence au contact |

`Params::split()` rend le nombre de sous-pas et d'itérations par sous-pas réellement faits : `iterations` divisé par `substeps`, arrondi vers le haut.

Dans le nœud `Solver`, le réglage de flexion est en **millièmes de N·m** : 0,1 dans l'interface vaut 1e-4 ici.

### Accélération de Tchebychev

Entre les itérations, la position est extrapolée d'un poids `omega`, calculé d'après le rayon spectral supposé. Cela fait converger plus vite.

Deux garde-fous (`Params::rho_used`) :

- avec peu d'itérations par sous-pas, l'extrapolation n'a pas le temps de se stabiliser et le tissu explose. Elle est donc bridée, et **coupée en dessous de quatre itérations** ;
- elle est coupée sur les sommets **au contact** d'un obstacle, où elle est instable.

### Obstacles

Sol, boule, boîte. Ils sont résolus comme des contraintes **dans le bloc du sommet**, pas après coup. L'adhérence freine le glissement.

**L'auto-collision n'est pas faite.**

### Les noyaux WGSL

Deux modules, compilés une fois pour tous les tissus et gardés :

| Module | Points d'entrée | Rôle |
|---|---|---|
| `tissu-vbd` | `solve` | Une passe sur une couleur : le pas de Newton de chaque sommet |
| `tissu-pas` | `predict`, et les suivants | Prédiction du pas, accélération, mise à jour des vitesses |

Chaque lancement choisit sa case de réglages (`Bind::Slot`) : la couleur à traiter, le poids d'accélération de l'itération.

### L'objet `Cloth`

L'interface qu'utilise le reste du code.

| Méthode | Rôle |
|---|---|
| `Cloth::new(topo, positions, on_gpu)` | Prépare le tissu. Essaie la carte si demandé |
| `on_gpu()` | Le calcul se fait-il sur la carte ? |
| `set_positions`, `set_velocities` | Change l'état |
| `step(dt, params)` | Avance d'une image. Sur la carte, rend la main dès que le travail est envoyé |
| `request()` | Demande l'état à la carte, sans attendre. Rend un `Pending` |
| `accept(pending)` | Range dans l'état ce que la carte a rendu |
| `sync()` | Met l'état du processeur à jour, en attendant la carte s'il le faut |
| `read()` | Positions et vitesses, dans l'ordre de la géométrie |

**Filet de sécurité.** `Cloth` garde les pas envoyés à la carte et pas encore relus. Si la carte rend une erreur, `accept` les refait sur le processeur, et le tissu y reste.

Un tissu ne va pas sur la carte s'il n'y tient pas : une colonne dépasserait ce qu'un noyau peut brancher (`GpuCloth::new` rend `None`).

### Le lien avec les nœuds

`geo::VbdCloth` implémente le trait `Solver` (voir [Système de nœuds](noeuds.md#le-contrat-solver)).

- Il lit `P`, `v`, `pin`, et `mass` s'il existe.
- Il garde la forme au repos dans l'état : `repos` (points) et `rest` (arêtes), mesurées au premier pas. Une reprise au milieu de la simulation retrouve ainsi le même tissu.
- Il garde quelques tissus prêts. Le plus ancien cède sa place, et sa mémoire sur la carte.
- Si `P` et `v` reviennent telles qu'il les a rendues au dernier pas, la carte a déjà cet état : rien n'est renvoyé.
- `geo::cloth_status()` rend ce qu'a fait le dernier pas (carte ou processeur, nom de la carte, points, couleurs, sous-pas, temps) pour l'afficher dans l'aperçu.

### Comparer avec le XPBD

`gpucloth::compare` donne le même budget de passes au VBD et à une autre méthode, sur deux essais : un tissu pendu calculé jusqu'au bout, et une chute sur une boule. `stretch` mesure l'allongement des arêtes par rapport au repos (le pire et la moyenne). `substeps_for(budget)` choisit le nombre de sous-pas pour un budget.

## Le banc (`gpubench`)

`gpubench::run(quick, progress)` rend un `Rapport`. `quick` réduit les tailles (tests, rendu logiciel).

Le banc a ses propres noyaux WGSL : copie, itération de Jacobi à 7 points, advection semi-lagrangienne, divergence, projection.

| Étape | Ce qui est mesuré |
|---|---|
| 1. Copie | Le plafond de débit de la carte |
| 2. Pression | Une itération de Jacobi sur une grille 3D |
| 3. Fumée | Un pas complet : advection, flottabilité, 40 itérations de pression |
| 4. Tissu | Temps par passe sur 250 000 et 1 million de sommets, contre le processeur |
| 5. Comparaison | VBD contre XPBD à nombre de passes égal |
| Contrôles | Écart entre la carte et le processeur, pour la pression et pour un petit tissu |

`Mesure::debit()` rend le débit en Go/s : octets déplacés par passe, divisés par le temps.

`gpubench::start()` lance le banc dans un fil d'exécution. `gpubench::ui()` dessine la fenêtre. Une erreur de la carte ne laisse pas la fenêtre attendre pour rien.

La fumée du banc est un **essai de charge**. Elle n'est branchée sur aucun nœud.

Pour l'usage, voir [Banc de mesure](../utilisateur/banc-gpu.md).

## Écrire un nouveau solveur sur la carte

1. **La version processeur d'abord.** Une fonction pure, sur des colonnes. Elle servira de référence.
2. **Le noyau WGSL.** Colle `gpusim::PRELUDE` en tête. Un attribut par tampon. Vérifie les bornes du numéro d'élément.
3. **Compile** avec `Gpu::kernels`, une fois, dans un `OnceLock`.
4. **Les colonnes** avec `Gpu::column`. Regarde `page_len` : si ton noyau lit ses voisins n'importe où, exige une seule page, et replie-toi sur le processeur sinon.
5. **Le test** qui compare carte et processeur sur un petit cas. Il doit passer aussi sans carte (il ne vérifie alors rien).
6. **Le repli.** Si `gpu()` rend `None` ou si `broken()` passe à vrai, refais le pas sur le processeur.
7. **Le trait `Solver`** dans `geo.rs`, et l'inscription dans `solvers()` et `SOLVER_NAMES`. Voir [Ajouter un solveur](ajouter.md#ajouter-un-solveur).
8. **Une mesure dans le banc.**

## Ce qui manque

- **Auto-collision** du tissu.
- **Collision avec un maillage** quelconque.
- **Fumée, fluides, corps rigides, cheveux, destruction** : rien n'est branché. Le banc contient seulement des noyaux d'essai pour la fumée.
- **Affichage sans relecture.** L'état est relu de la carte vers le processeur pour être affiché, puisque la vue est sur le processeur. Avec une vue sur carte graphique, cette relecture disparaîtrait.
- **Cache disque sur la carte.** Le cache figé passe par les colonnes du processeur.

La suite est résumée dans la [feuille de route du moteur](feuille-de-route-moteur.md).
