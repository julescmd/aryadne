# Simulation sur carte graphique

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

Méthode : **Vertex Block Descent** (Chen, Macklin, Kim, Yuksel, 2024), avec une correction grossière multi-niveaux, un frottement de Coulomb sur les obstacles et une auto-collision entre sommets.

### Le principe

Le pas de temps implicite est une minimisation. Pour chaque sommet, on cherche la position qui équilibre son inertie et les forces de ses voisins.

VBD le fait **un sommet à la fois**, par un pas de Newton sur un bloc 3×3, les voisins étant figés. Deux sommets sans lien entre eux peuvent être traités en même temps. On **colorie** donc les sommets au chargement : aucun fil ne relie deux sommets de la même couleur. Chaque couleur est un lancement sur la carte.

### Les énergies

| Énergie | Forme | Réglage |
|---|---|---|
| Membrane | Un ressort par arête | `stiffness`, en N/m |
| Cisaillement | Les diagonales sont des ressorts plus souples | `shear`, en part de `stiffness` |
| Flexion | Pour chaque sommet i et chaque paire d'arêtes opposées (i au milieu de j et k) : ½·D·\|(1−t)·xj + t·xk − xi\|² | `bending`, en N·m |
| Auto-collision | Un ressort qui ne fait que repousser, entre deux sommets non voisins plus proches que l'épaisseur | `thickness`, en mètres |

La flexion est nulle quand les trois points sont alignés. Elle est quadratique, donc sa dérivée seconde est constante : c'est la moins chère des flexions pour ce solveur.

**Les diagonales.** Un tissu tissé se déforme bien plus facilement en biais que dans le sens des fils. C'est ce qui lui permet d'épouser une forme ronde. Avec des diagonales aussi raides que les arêtes, il se comporte comme du papier. `Topo::build_with` reconnaît les diagonales sans aide : dans un carré qui a ses deux diagonales, les quatre sommets sont tous liés entre eux, et des trois façons d'apparier ces six arêtes, les diagonales forment la paire la plus longue. Elles portent une marque (bit 30 de l'entrée) et leur raideur est multipliée par `shear`. Un maillage en triangles, sans diagonales croisées, n'a aucune arête marquée.

### Garder la mémoire sobre

C'est elle qui borne la vitesse. Quatre choix :

- **Rangement.** Les sommets sont rangés par couleur, puis le long d'une **courbe de Morton**. Un lancement lit une plage contiguë, et les voisins d'un sommet sont proches en mémoire.
- **Voisinage par sommet.** Chaque sommet a sa liste de voisins. Le noyau ne fait que **rassembler** : aucune écriture concurrente. Ressorts et flexion tiennent dans **une seule liste de 12 octets par entrée** : le voisin, le sommet qui prolonge l'arête, et les deux longueurs au repos sur 16 bits chacune.
- **Peu de couleurs.** Seuls les ressorts comptent pour colorier. Les deux bouts d'un terme de flexion peuvent avoir la même couleur : chacun lit alors de l'autre sa position **du début de la passe**, gardée dans deux tampons qui s'échangent à chaque passe. Une grille avec diagonales passe ainsi de 8 couleurs à 4, donc de 8 lancements par passe à 4. L'auto-collision lit ses voisins dans les mêmes tampons.
- **Positions relatives.** Les positions sont relatives au centre du tissu. En `f32`, un tissu posé à 100 m de l'origine garde la précision d'un tissu posé à l'origine.

Le rangement par grappes de 64 sommets (variable `ARY_TISSU_GRAPPES`) a été retiré : il lisait moins d'octets, mais il était deux fois plus lent sur une vraie carte (2,1 ms contre 0,85 ms par passe à 250 000 sommets sur une RTX 3060 Ti).

### La topologie (`Topo`)

Tout ce qui ne dépend que du maillage au repos. Construite une fois, par `Topo::build`. Les mêmes tableaux servent au processeur et à la carte.

| Champ | Contenu |
|---|---|
| `perm`, `inv` | Rang d'origine du sommet rangé en k, et l'inverse |
| `groups` | Plages traitées l'une après l'autre, une par couleur. Les sommets fixes sont après la dernière |
| `phases` | Nombre de couleurs |
| `off` | Par sommet : début de ses ressorts, puis leur nombre et sa couleur |
| `springs` | Une entrée par voisin : son rang (bit 31 : forme un terme de flexion avec l'entrée suivante, bit 30 : diagonale), le sommet qui prolonge l'arête, et deux longueurs au repos codées sur 16 bits |
| `rest_scale` | Échelle des longueurs codées : longueur au repos = code² × cette échelle |
| `mass` | Masse. 0 : sommet fixe |
| `spacing` | Taille moyenne d'une maille : l'épaisseur d'auto-collision par défaut en dépend |
| `rest_pos` | Positions au repos : deux sommets proches au repos ne se repoussent jamais |
| `coarse` | Les niveaux grossiers |

Sans masse fournie, elle est déduite de la taille des mailles, avec `DENSITY = 0.2` kg/m².

### Les réglages (`Params`)

| Champ | Défaut | Sens |
|---|---|---|
| `stiffness` | 1000 | Raideur des arêtes, en N/m |
| `shear` | 0,1 | Raideur des diagonales, en part de celle des arêtes |
| `bending` | 1e-4 | Raideur de flexion, en N·m |
| `damping` | 0,02 | Part de vitesse perdue par image |
| `calm` | 0,3 | Amortissement interne, de 0 à 1 |
| `iterations` | 16 | Passes sur tous les sommets, par image, réparties entre les sous-pas |
| `substeps` | 8 | Sous-pas par image (1 à 256) |
| `rho` | 0,9 | Rayon spectral supposé pour l'accélération. 0 : sans |
| `floor`, `ball`, `cube` | | Obstacles |
| `friction` | 0,4 | Coefficient de frottement sur les obstacles |
| `thickness` | aucune | Auto-collision : distance gardée entre deux sommets. 0 : choisie d'après la maille |
| `levels` | 0 | Niveaux grossiers utilisés |

`Params::split()` rend le nombre de sous-pas et de passes par sous-pas réellement faits. `Params::with_levels()` rend les mêmes réglages avec la correction grossière : c'est ce que fait le nœud.

Dans le nœud `Solver`, le réglage de flexion est en **millièmes de N·m** : 0,1 dans l'interface vaut 1e-4 ici.

### Un sous-pas

1. **Prédiction.** Chaque sommet avance de sa vitesse. Avant cela, l'amortissement interne rapproche sa vitesse de la moyenne de celles de ses voisins. Un sommet qui entre dans un obstacle en est sorti, et marqué.
2. **Auto-collision**, si elle est demandée : la grille, puis les voisins proches de chaque sommet.
3. **Correction grossière**, si le tissu a des niveaux.
4. **Les passes** : un lancement par couleur. La dernière passe écrit aussi la vitesse du sous-pas.

Il n'y a donc pas de lancement séparé pour les vitesses. Sans niveaux, un sous-pas d'une passe coûte 1 + le nombre de couleurs lancements.

**L'amortissement interne** (`calm`) ne freine pas un mouvement d'ensemble : un tissu qui tombe ou se balance garde sa vitesse. Il éteint les frémissements que laisse un calcul arrêté tôt. Sans lui, un tissu posé ne se calme jamais tout à fait.

### La correction grossière

Les passes sommet par sommet ne propagent une déformation que d'une maille à la fois. Sur un tissu fin, un étirement d'ensemble met des centaines de passes à se résorber, et le tissu paraît mou. Au début de chaque sous-pas, la force qui reste sur chaque sommet est donc confiée à un tissu quatre fois moins dense, lui-même aidé par un tissu encore quatre fois moins dense, et ainsi de suite. Le déplacement d'ensemble trouvé est rendu au niveau fin.

Les niveaux grossiers ne résolvent pas l'énergie du tissu mais une énergie plus simple qui la borne par le dessus : chaque ressort y est remplacé par un ressort de même raideur dans toutes les directions (Liu et al. 2013 ; c'est la matrice de Projective Dynamics). Leur matrice ne dépend donc que du maillage au repos, et se calcule une fois au chargement (Galerkine : Pᵀ·A·P).

**Rangement.** Tous les niveaux sont bout à bout dans les mêmes tableaux (`Coarse`) : `vert` (diagonale, début des voisins et des enfants), `ent` (voisins), `kids` (les sommets d'au-dessus qui suivent ce sommet), `par` (ses parents dans le niveau d'en dessous), `up` (les parents de chaque sommet du tissu). Un seul branchement sert à tous les niveaux.

**Le cycle.** Un cycle en V avec des passes de **Jacobi** (poids 0,8). Jacobi ne demande pas de couleurs : une passe est un lancement, quel que soit le niveau. Par niveau, quatre étapes :

| Étape | Rôle |
|---|---|
| `gather` | Réunit la force restante des enfants, et fait la première passe (à partir d'un déplacement nul) |
| `remain` | Force restante, confiée au niveau suivant |
| `prolong` | Ajoute le déplacement des parents |
| `post` | Dernière passe |

Sur le niveau le plus grossier, `cycle.bottom` passes de plus (`remain` puis `apply`).

**Les petits niveaux d'un seul lancement.** À partir du premier niveau de moins de `TAIL` (2048) sommets, tout le bas du cycle est fait par le noyau `tail` : un seul groupe de 64 fils, qui enchaîne toutes les étapes de tous ces niveaux en s'attendant entre deux (`storageBarrier`). À cette taille, ce qui coûte est le lancement, pas le calcul.

Une correction coûte donc : 1 (force restante du tissu) + 4 par grand niveau + 1 (`tail`) + 1 (remontée vers le tissu). Pour 250 000 sommets : 3 grands niveaux, 15 lancements. Avant, avec des passes par couleur sur chaque niveau : environ 170.

Une correction seulement de temps en temps a été essayée : elle rattrape d'un coup l'erreur de plusieurs sous-pas, ce qui se retrouve dans les vitesses, et le tissu rebondit.

### Obstacles et frottement

Sol, boule, boîte. Ils sont résolus **dans le bloc du sommet** : après son pas de Newton, un sommet qui est dans un obstacle en est sorti par le plus court chemin.

**Frottement de Coulomb, sur les déplacements.** L'enfoncement mesure la force d'appui. Le frottement retient un glissement d'au plus µ fois cet enfoncement depuis le début du sous-pas. En dessous, le sommet reste où il était. Au-dessus, il glisse d'autant moins. Un tissu glisse donc d'une pente raide et tient sur une pente douce.

L'ancienne adhérence multipliait le glissement par un facteur à chaque sous-pas, quelle que soit la force d'appui : le tissu collait partout où il touchait, et gardait la forme d'une tente tendue. La loi exacte du bloc (qui divise par la raideur en travers du sommet) a aussi été essayée : avec peu de passes, les forces des voisins pas encore à l'équilibre la débordent, et le tissu rampe.

**Contacts et correction grossière.** La correction ne voit pas les obstacles. Trois règles la rendent compatible avec eux :

- un sommet qui **vient d'entrer** dans un obstacle (marque 3) ne reçoit pas de correction, et ses voisins lui sont attachés comme à un point d'ancrage. C'est ce qui arrête le tissu autour du point d'impact dans le même sous-pas ;
- un sommet **déjà posé** (marque 1) reçoit la correction, mais la part de sa force qui le pousse vers l'obstacle est retirée : l'obstacle la retient déjà. Il peut ainsi glisser le long de la surface et relâcher un étirement ;
- après la correction, tout sommet déplacé est sorti des obstacles.

Marques, dans la quatrième valeur de la position : 0 libre, 1 posé sur un obstacle (ou repoussé par le tissu), 2 fixe, 3 vient d'entrer dans un obstacle.

### Auto-collision

Le tissu est fait de points reliés par des fils : il n'a pas de faces. L'auto-collision se fait donc **entre sommets** : deux sommets qui ne sont pas voisins dans le tissu ne s'approchent pas à moins d'une épaisseur.

- **L'épaisseur** doit être de l'ordre de la maille. En dessous de 0,71 maille, un sommet passe au milieu d'un carré de quatre sommets sans en toucher aucun. Par défaut : 0,9 maille (`AUTO_THICK`).
- **Qui se repousse.** Deux sommets plus proches au repos que 2,1 épaisseurs ne se repoussent jamais (`REST_APART`). Le tissu peut ainsi se froisser sans se gêner lui-même.
- **La grille.** À chaque sous-pas, après la prédiction, les sommets sont rangés dans une grille de hachage dont les cases ont la taille de la distance de recherche (1,5 épaisseur). Cinq lancements : `count` (comptage par case, par additions atomiques, et par bloc de 256 cases), `scan` (somme des blocs, par un seul groupe de fils), `spread` (début de chaque case, un fil par bloc), `fill` (rangement, par soustractions atomiques : les comptes retombent à zéro pour le sous-pas suivant), `nearby` (les voisins).
- **Les voisins.** Chaque sommet garde ses 8 plus proches voisins (`NEAR`) parmi les 27 cases autour de lui, **triés par numéro**. Le résultat ne dépend ni de l'ordre de rangement dans une case, ni de l'ordre des fils : le calcul est déterministe.
- **La répulsion.** Dans le bloc du sommet, comme une énergie de plus. Les deux sommets d'une paire font le même calcul pendant la même passe, chacun lisant l'autre au début de la passe : chacun compte la raideur deux fois, ce qui revient à faire la moitié du chemin chacun, sans dépassement.
- **Avec la correction grossière.** Près d'une autre couche, la correction ne déplace pas un sommet de plus d'un quart d'épaisseur : deux couches ne peuvent pas se croiser à cause d'elle.

L'auto-collision demande 9 tampons de stockage sur un noyau. Sur une carte qui n'en permet que 8, le tissu passe sur le processeur.

Limites : pas de frottement entre les couches ; un tissu qui se replie plus vite qu'une demi-épaisseur par sous-pas peut se traverser (monter les sous-pas ou l'épaisseur) ; au-delà de 128 sommets dans une case, le résultat dépend de l'ordre de rangement.

### Accélération de Tchebychev

Entre les passes, la position est extrapolée d'un poids `omega`, calculé d'après le rayon spectral supposé. Trois garde-fous (`Params::rho_used`) : elle est bridée avec peu de passes par sous-pas et **coupée en dessous de quatre**, coupée sur les sommets **au contact**, et coupée avec les niveaux grossiers, qui font ce travail.

### Les noyaux WGSL

Compilés une fois pour tous les tissus et gardés (`programs`, et `self_programs` pour l'auto-collision, à la première demande).

| Module | Points d'entrée | Rôle |
|---|---|---|
| `tissu-prediction` | `predict` | Début d'un sous-pas : amortissement interne, prédiction, sortie des obstacles |
| `tissu-vbd` | `solve` | Une couleur : le pas de Newton de chaque sommet, obstacles, frottement |
| `tissu-acceleration` | `accel` | Accélération de Tchebychev |
| `tissu-reste` | `rest` | Force restante du niveau fin |
| `tissu-niveaux` | `gather`, `remain`, `apply`, `prolong`, `post`, `tail` | Les niveaux grossiers |
| `tissu-remontee` | `lift` | Le déplacement du niveau 0, rendu au tissu |
| `tissu-grille` | `count`, `scan`, `spread`, `fill`, `nearby` | Auto-collision : grille et voisins |
| `tissu-relecture` | `pack` | Positions et vitesses dans l'ordre de la géométrie |

Chaque lancement choisit sa case de réglages (`Bind::Slot`) : la couleur à traiter, le niveau, le poids d'accélération.

`Cloth::launches()` rend le nombre de lancements du dernier pas d'image. Le banc l'affiche.

### Ce que coûte un sous-pas

`Topo::substep_cost(params)` le donne en passes du niveau fin, d'après les octets lus et écrits : ses passes, plus l'amortissement interne, la correction grossière et l'auto-collision s'ils sont actifs. `split_budget(topo, budget, params)` en déduit le découpage conseillé pour un budget par image : autant de sous-pas d'une passe que le budget en paie.

### L'objet `Cloth`

L'interface qu'utilise le reste du code.

| Méthode | Rôle |
|---|---|
| `Cloth::new(topo, positions, on_gpu)` | Prépare le tissu. Essaie la carte si demandé |
| `on_gpu()` | Le calcul se fait-il sur la carte ? |
| `set_positions`, `set_velocities` | Change l'état |
| `push_velocities` | Les vitesses voulues après les forces de l'image : l'écart est réparti sur les sous-pas |
| `step(dt, params)` | Avance d'une image. Sur la carte, rend la main dès que le travail est envoyé |
| `request()` | Demande l'état à la carte, sans attendre. Rend un `Pending` |
| `accept(pending)` | Range dans l'état ce que la carte a rendu |
| `sync()` | Met l'état du processeur à jour, en attendant la carte s'il le faut |
| `read()` | Positions et vitesses, dans l'ordre de la géométrie |
| `launches()` | Lancements du dernier pas d'image |

**Filet de sécurité.** `Cloth` garde les pas envoyés à la carte et pas encore relus. Si la carte rend une erreur, `accept` les refait sur le processeur, et le tissu y reste.

Un tissu ne va pas sur la carte s'il n'y tient pas : une colonne dépasserait ce qu'un noyau peut brancher (`GpuCloth::new` rend `None`).

### Le lien avec les nœuds

`geo::VbdCloth` implémente le trait `Solver` (voir [Système de nœuds](noeuds.md#le-contrat-solver)).

- Il lit `P`, `v`, `pin`, et `mass` s'il existe.
- Il garde la forme au repos dans l'état : `repos` (points) et `rest` (arêtes), mesurées au premier pas. Une reprise au milieu de la simulation retrouve ainsi le même tissu.
- Il garde quelques tissus prêts. Le plus ancien cède sa place, et sa mémoire sur la carte.
- Si `P` et `v` reviennent telles qu'il les a rendues au dernier pas, la carte a déjà cet état : rien n'est renvoyé.
- `geo::cloth_status()` rend ce qu'a fait le dernier pas (carte ou processeur, nom de la carte, points, couleurs, sous-pas, temps) pour l'afficher dans l'aperçu.

Les entrées du nœud : `frottement` (8), `cisaillement` (14), `amortissement des plis` (15), `épaisseur` (16), et le réglage `auto-collision`. Les nouvelles entrées sont à la fin : un graphe enregistré avant elles prend leurs valeurs par défaut.

### Comparer avec le XPBD

`gpucloth::compare` donne le même budget au VBD, sans et avec niveaux, et à une autre méthode, sur deux essais : un tissu pendu calculé jusqu'au bout (quelle raideur obtient-on vraiment ?), et une chute sur une boule (de combien s'étire-t-il ?). Le budget du VBD compte tout ce que coûte un sous-pas (`substep_cost`). Les diagonales y sont aussi raides que les arêtes : on mesure l'étirement, pas le cisaillement. `stretch` mesure l'allongement des arêtes par rapport au repos (le pire et la moyenne).

## Le banc (`gpubench`)

`gpubench::run(quick, progress)` rend un `Rapport`. `quick` réduit les tailles (tests, rendu logiciel).

Le banc a ses propres noyaux WGSL : copie, itération de Jacobi à 7 points, advection semi-lagrangienne, divergence, projection.

| Étape | Ce qui est mesuré |
|---|---|
| 1. Copie | Le plafond de débit de la carte |
| 2. Pression | Une itération de Jacobi sur une grille 3D |
| 3. Fumée | Un pas complet : advection, flottabilité, 40 itérations de pression |
| 4. Tissu | Sur 250 000 et 1 million de sommets : temps et lancements par passe, une image au budget de 16 et de 32 passes sans et avec niveaux grossiers, le surcoût de l'auto-collision, contre le processeur |
| 5. Comparaison | XPBD, VBD, VBD avec niveaux, à budget égal : raideur obtenue et chute sur une boule |
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

- **Auto-collision par triangles.** Elle se fait entre sommets, avec une épaisseur de l'ordre de la maille. Des faces, puis des bornes de déplacement par sommet (OGC), permettraient une épaisseur fine.
- **Frottement entre les couches** du tissu.
- **Collision avec un maillage** quelconque.
- **Fumée, fluides, corps rigides, cheveux, destruction** : rien n'est branché. Le banc contient seulement des noyaux d'essai pour la fumée.
- **Affichage sans relecture.** L'état est relu de la carte vers le processeur pour être affiché, puisque la vue est sur le processeur. Avec une vue sur carte graphique, cette relecture disparaîtrait.
- **Cache disque sur la carte.** Le cache figé passe par les colonnes du processeur.

La suite est résumée dans la [feuille de route du moteur](feuille-de-route-moteur.md).
