# Simulation sur carte graphique

Neuf modules.

| Module | Rôle |
|---|---|
| `gfx` | Le moteur d'affichage de l'interface (wgpu ou OpenGL), et la carte qu'il partage avec les simulations |
| `gpusim` | Le socle : tout ce que les solveurs partagent, et rien d'autre |
| `gpucloth` | Le tissu en Vertex Block Descent, avec sa version processeur |
| `gpudraw` | Le dessin d'un maillage directement depuis les tampons d'un solveur |
| `volume` | Les données d'un volume (grille dense, demi-précision), son cache, son rendu sur le processeur |
| `smoke` | La fumée et le feu : réglages, opérations, version processeur de référence |
| `gpusmoke` | La fumée et le feu sur la carte |
| `gpuvolume` | Le dessin d'un volume directement par la carte (ray marching) |
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

### La carte de l'interface

L'interface d'Aryadne dessine avec wgpu (c'est le moteur par défaut d'eframe 0.37). `gpu()` prend **le même appareil** qu'elle : `gfx::ui()` rend son `RenderState`, et `Gpu::shared` vaut vrai. C'est ce qui permet à une vue de dessiner les tampons d'un solveur sans les relire.

Pour cela, `gfx::wgpu_setup()` fait demander à l'interface toutes les limites de la carte, pas celles par défaut de WebGPU.

`gpu()` ouvre son propre appareil dans trois cas : l'interface est en mode compatible OpenGL (`glow`), il n'y a pas d'interface (`--banc-gpu`, tests), ou l'appareil de l'interface est trop limité. En mode compatible, wgpu n'a alors droit qu'à Vulkan, DirectX 12 et Metal : le contexte OpenGL de la fenêtre appartient à l'interface. Sur une machine qui n'a qu'OpenGL, les simulations passent donc sur le processeur dans ce mode.

Conséquence du partage : les commandes de calcul et celles de l'affichage passent par **la même file**. Un pas de simulation de 20 ms retarde de 20 ms l'image d'interface qui le suit. C'est voulu (l'image montre ce pas), mais il ne faut jamais empiler les pas : voir « Un seul pas en route » plus bas.

`Gpu::msaa4` dit si la carte sait lisser un dessin à quatre échantillons par pixel (garanti par WebGPU, pas par tous les pilotes OpenGL).

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
| `block()` | Attend qu'elles soient arrivées, sans les copier |
| `peek(f)` | Regarde les octets arrivés sans les copier ni rendre la relecture. Ne bloque pas |
| `wait()` | Attend la carte et rend les octets |
| `take::<T>()` | Les données, dans leur type |

Une colonne porte aussi l'usage « tampon de sommets » : une vue la dessine telle quelle.

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
| 3 bis. Fumée des nodes | Le vrai solveur (`smoke::Sim`) en 128³ et 256³ : temps par pas à 2, 4 et 8 itérations de MGPCG, divergence avant et après, coût du transfert pour le cache, mémoire sur la carte (`Fumee`) |
| 4. Tissu | Sur 250 000 et 1 million de sommets : temps et lancements par passe, une image au budget de 16 et de 32 passes sans et avec niveaux grossiers, le surcoût de l'auto-collision, contre le processeur |
| 5. Comparaison | XPBD, VBD, VBD avec niveaux, à budget égal : raideur obtenue et chute sur une boule |
| Contrôles | Écart entre la carte et le processeur, pour la pression et pour un petit tissu |

`Mesure::debit()` rend le débit en Go/s : octets déplacés par passe, divisés par le temps.

`gpubench::start()` lance le banc dans un fil d'exécution. `gpubench::ui()` dessine la fenêtre. Une erreur de la carte ne laisse pas la fenêtre attendre pour rien.

La fumée de l'étape 3 est un **essai de charge**, gardée pour comparer : même taille, 40 itérations de Jacobi. Celle de l'étape 3 bis est le solveur des nœuds.

Pour l'usage, voir [Banc de mesure](../utilisateur/banc-gpu.md).

## La fumée et le feu

### Les données (`volume`)

`Volume` : une grille dense. `res` (voxels par axe), `origin` (coin bas), `voxel` (taille), et deux tableaux de quatre nombres **f16** par voxel (`Grids`) :

| Tableau | Contenu |
|---|---|
| `sca` | densité, température, carburant, flamme |
| `vel` | vitesse sur les trois faces basses du voxel (grille décalée), et l'expansion des gaz qui brûlent |

Soit 16 octets par voxel, en mémoire comme sur la carte. `to_half` et `from_half` convertissent (arrondi au plus proche, valeurs bornées à ±65504, table de 65 536 entrées pour la lecture).

Le volume voyage dans `Geo::volume`. Il entre dans `Geo::stamp`, `Geo::bytes`, `Geo::ready`, et dans le cache disque par un bloc `VOLU` (voir [Formats](formats.md#cache-de-géométrie-arygeocache)).

Trois formes de contenu (`Base`) : vide (rien n'est alloué), des voxels, ou **l'état d'un solveur** (`smoke::Shot`), peut-être encore sur la carte.

**Opérations en attente.** `Source de fumée` et `Combustion` ne touchent pas aux voxels : ils ajoutent une `Op` au volume (`with_op`). Le solveur les fait dans son pas, sur la carte. Sans solveur derrière, `Volume::grids` les fait sur le processeur à la première lecture (`smoke::apply_ops`). C'est le même principe que les forces notées sur une colonne de tissu.

`Volume::live()` rend de quoi dessiner depuis la carte si les tampons du solveur montrent encore exactement ce volume.

### Un pas (`smoke`, `gpusmoke`)

1. **Advection** de la fumée, puis de la vitesse. MacCormack : un aller semi-lagrangien, un retour, correction de la moitié de l'écart, bornée par les huit voisins du point de départ. Chaque composante de la vitesse est advectée sur ses propres faces.
2. **Sources** (`Emit`), une par lancement : sphère et boîte calculées dans le noyau, points d'une géométrie par un masque envoyé à la carte quand il change. L'irrégularité est un hasard entier par blocs de 4 voxels, identique des deux côtés.
3. **Réaction** : dissipation, refroidissement, combustion (`Burn`). Le carburant plus chaud que l'allumage brûle ; le refroidissement par rayonnement suit la solution exacte de dT/dt = −k·T⁴.
4. **Forces** sur les faces : flottabilité, confinement de vorticité (deux noyaux : rotationnel, puis force), vent, frein. Une face contre un solide est mise à zéro.
5. **Pression**, puis **projection**.

Types de voxels : fluide, solide, ouvert (pression nulle). La couche du bord n'est jamais fluide. Les types ne sont recalculés que si les obstacles ou les bords changent.

### La pression : MGPCG sans matrice

Le système `A·p = b` n'est jamais stocké : la diagonale d'un voxel est son nombre de voisins non solides, et chaque voisin fluide compte pour −1. `b` est l'expansion voulue moins le flux sortant.

Gradient conjugué, préconditionné par **un cycle en V** de multigrille géométrique :

| Élément | Choix |
|---|---|
| Lissage | Gauss-Seidel rouge-noir, 2 balayages avant et après (`SWEEPS`), rouge puis noir à la descente, noir puis rouge à la remontée : le cycle est symétrique, comme l'exige le gradient conjugué |
| Restriction | Le résidu des huit enfants, réuni (fusionné avec le calcul du résidu : pas de tampon de plus) |
| Prolongement | La correction du parent, ajoutée telle quelle |
| Niveau grossier | Mêmes règles sur une grille deux fois plus petite. Un voxel grossier est ouvert si un enfant l'est, sinon fluide si un enfant l'est |
| Fond | 8 balayages dans chaque sens (`COARSE_SWEEPS`) sur le dernier niveau (côtés pairs, au moins 8 voxels pour descendre encore) |
| Départ | La pression du pas d'avant (projection amortie) |

Mesuré sur une bouffée chaude en 32³ : la plus grande divergence est divisée par 5 environ à chaque itération (2,3 → 1,0e-1 → 2,4e-3 → 4,1e-6 pour 0, 2, 4, 8 itérations).

**Rien n'attend le processeur.** Les produits scalaires sont réduits sur la carte : un premier étage par paquets de 64 voxels, des étages par 64, et un dernier fil qui range le résultat dans un petit tampon de scalaires. Les noyaux `update_pr` et `update_s` y lisent α et β. Le nombre d'itérations est fixe. La divergence avant et après est relue avec un pas de retard, pour l'afficher (`Sim::stats`).

### Les noyaux WGSL

Trois jeux, 8 tampons de stockage au plus chacun. Chaque lancement choisit sa case de réglages (128 octets, `Bind::Slot`) ; les cases identiques sont partagées, et au-delà de 512 cases le travail part en plusieurs envois.

| Jeu | Noyaux |
|---|---|
| `fumée-champs` | `types`, `advect_sca_fwd`, `advect_sca_corr`, `advect_vel_fwd`, `advect_vel_corr`, `emit`, `react`, `curl`, `confine`, `forces`, `residual`, `project` |
| `fumée-pression` | `dots`, `reduce_ab`, `reduce_ba`, `last_a`, `last_b`, `update_pr`, `update_s` |
| `fumée-multigrille` | `relax`, `gather`, `prolong`, `coarsen` |

Les champs en f16 se lisent par `unpack2x16float` et s'écrivent par `pack2x16float` : aucune fonctionnalité optionnelle de la carte n'est demandée.

Mémoire sur la carte : environ 54 octets par voxel (quatre tampons de champs à 8 octets, types, pression, résidu, direction, niveaux). 256³ : 0,9 Go. 512³ : 7,3 Go.

### `Sim`, `Shot`, `Live`

`smoke::Sim` est l'interface : `new` (carte si la grille y tient, sinon processeur), `load`, `step` (rend la main dès l'envoi), `busy`, `wait`, `shot`, `stats`.

`Shot` est l'état à la fin d'un pas. Sur la carte, la relecture des deux tampons de champs est **demandée, pas attendue** : comme ils sont déjà en f16, c'est une simple copie. `Shot::grids` attend et copie, à la première demande seulement.

`Live` porte le tampon de la fumée et un compteur : `current()` est faux dès que le solveur a avancé.

### Le lien avec les nœuds (`geo`)

| Nœud | Opération |
|---|---|
| `vol_box` | Crée un `Volume` vide. Voxels par multiples de 8 |
| `vol_source` | Ajoute une `Op::Emit` |
| `vol_burn` | Ajoute une `Op::Burn` |
| `sim_smoke` | `smoke_step` : trouve la session de cette grille (`SMOKES`, deux au plus), charge l'état s'il ne vient pas d'elle, avance, rend un volume paresseux |
| `vol_look` | Change `Volume::look` (opacité, couleur, lueur, ombre) |

`smoke_step` suit les règles du tissu : même pas redemandé, même résultat (`memo`) ; carte occupée et calcul par tranches, le moteur rend la main (`SIM_WAIT`) ; carte perdue, le calcul continue sur le processeur (`Sim::fall_back`). `geo::smoke_status()` donne de quoi remplir la ligne de l'aperçu.

### Le dessin (`gpuvolume`)

Un triangle couvre l'image. Pour chaque pixel, le fragment lance un rayon (inverse de la matrice de vue), traverse la boîte par pas de 0,9 voxel (128 pas au plus), et lit le tampon de la fumée par huit lectures (trilinéaire à la main).

| Élément | Calcul |
|---|---|
| Fumée | Opacité `1 − exp(−densité × opacité × pas)` |
| Ombre | Six pas de deux voxels vers la lumière principale |
| Flamme | Champ `flamme` × lueur, couleur d'après la température (`volume::flame_color`, la même formule dans le noyau) |
| Profondeur | Celle d'une image calculée ailleurs (`gpudraw::Soft`) arrête le rayon, comme celle d'un maillage dessiné par la carte (`Cam::front`, l'image des identifiants de `gpudraw`) |
| Ombre portée | Là où le rayon finit (une surface de l'image calculée ailleurs, sinon le plan du sol `Shadow::floor`), 24 pas vers la lumière additionnent la densité. L'ombre est posée sous la fumée, en noir avec cette opacité (`Cam::shadow`) |

`Source::Live` lit le tampon du solveur tel quel. `Source::Data` envoie les voxels d'un volume repris du cache, quand sa version change.

L'image est à demi-résolution (au quart sur un rendu logiciel) et étirée : la fumée est douce.

| Vue | Usage |
|---|---|
| Aperçu de l'onglet Simulation (`geoview::paint_volume`) | Par-dessus les points et les surfaces. Sans carte partagée : un petit rendu sur le processeur |
| Vue 3D d'une scène (`Viewer::volumes_paint`) | Par-dessus l'image du rasteriseur, avec sa profondeur |

Dans une scène, un volume seul donne un objet de deux triangles minuscules aux coins de sa boîte (`geoscene::to_model`) : il a ainsi sa place et sa taille.

### Le rendu final

`volume::composite` pose les volumes sur une image déjà rendue, sur le processeur, avec la même formule (`Sampler::march`). `model3d::volumes_over` lui donne les rayons de la caméra et la distance de la première surface.

| Rendu | Où |
|---|---|
| Export vidéo (`encode_animation`) | Après chaque image, avec la profondeur du rendu rapide |
| Rendu final (`pathtrace::Job::start_with`) | Chaque image du rendu passe par une retouche ; la distance des surfaces vient d'un rayon par pixel |

### Les tests

| Test | Ce qu'il vérifie |
|---|---|
| `smoke::la_pression_converge_vite` | La divergence baisse à chaque doublement des itérations, de 4 ordres de grandeur en 8 |
| `smoke::la_pression_avec_un_obstacle_et_des_bords_fermes` | Convergence avec boule et boîte, rien ne traverse un solide |
| `gpusmoke::carte_contre_processeur` | Quatre images avec sources, combustion, obstacles, vent : écart sous 2 %, même divergence de départ |
| `gpusmoke::sans_maccormack_et_par_un_masque` | Advection simple, source par masque, grille non cubique |
| `gpuvolume::la_carte_dessine_comme_le_processeur` | Même image des deux côtés, et occultation par la profondeur |
| `gpuvolume::ombre_de_la_fumee_au_sol` | Une boule de fumée pose son ombre au sol du côté opposé à la lumière, rien sans ombres |
| `geo::fumee::fumee_par_les_nodes` | Les trois exemples, le cache en mémoire et sur le disque |
| `geoscene::a_scene_renders_its_smoke` | Une fumée dans une scène et dans le rendu |

### Limites

- Grille dense. Pas de tuiles creuses, pas de montée en résolution.
- Obstacles analytiques seulement (une boule, une boîte), sans vitesse propre.
- La relecture de chaque image pour le cache est toujours demandée : 16 octets par voxel. En 256³, c'est 270 Mo par image. Le banc mesure ce que ça coûte.
- Le dessin ne lit que la lumière principale. Dans la vue, la fumée pose une ombre douce sur le sol et sur l'image du rasteriseur ; pas sur un maillage dessiné par la carte, et pas dans le rendu final. Elle n'éclaire rien.
- Le rendu final de la fumée se fait sur le processeur.

## Écrire un nouveau solveur sur la carte

1. **La version processeur d'abord.** Une fonction pure, sur des colonnes. Elle servira de référence.
2. **Le noyau WGSL.** Colle `gpusim::PRELUDE` en tête. Un attribut par tampon. Vérifie les bornes du numéro d'élément.
3. **Compile** avec `Gpu::kernels`, une fois, dans un `OnceLock`.
4. **Les colonnes** avec `Gpu::column`. Regarde `page_len` : si ton noyau lit ses voisins n'importe où, exige une seule page, et replie-toi sur le processeur sinon.
5. **Le test** qui compare carte et processeur sur un petit cas. Il doit passer aussi sans carte (il ne vérifie alors rien).
6. **Le repli.** Si `gpu()` rend `None` ou si `broken()` passe à vrai, refais le pas sur le processeur.
7. **Le trait `Solver`** dans `geo.rs`, et l'inscription dans `solvers()` et `SOLVER_NAMES`. Voir [Ajouter un solveur](ajouter.md#ajouter-un-solveur).
8. **Une mesure dans le banc.**

## Afficher sans relire

Avant, chaque image de simulation revenait vers le processeur pour être dessinée. Sur une RTX 4070, un tissu d'un million de sommets coûtait 20 ms de calcul et 14 ms de relecture. Trois mécanismes font disparaître cette relecture du chemin de l'affichage.

### 1. Les clichés (`gpucloth::Shot`)

`Cloth::shot()` rend l'état du tissu à la fin d'un pas, sous la forme d'un **cliché**.

| Méthode | Sens |
|---|---|
| `live` | De quoi dessiner depuis la carte : le tampon de positions, le centre, l'ordre de rangement (`Live`) |
| `Live::current()` | Les tampons montrent-ils encore cet état ? Faux dès que le tissu a avancé |
| `ready()` | `state()` rendrait-il la main sans attendre ? Ne bloque pas |
| `wait()` | Attend la fin du calcul, sans rien copier |
| `state()` | Positions et vitesses dans l'ordre de la géométrie. La première demande copie les données |
| `bounds_now(pas)` | Boîte englobante approchée, lue dans les octets arrivés, sans les copier |

Sur la carte, le cliché **demande** la relecture (noyau `pack`, copie vers un tampon lisible) mais ne l'attend pas. Tant que personne n'appelle `state()`, rien n'est copié vers le processeur.

Ce transfert demandé à chaque pas sert le cache des images : revenir en arrière dans la barre de temps relit le cliché, sans resimuler. Il occupe la carte, pas le processeur. Ce qu'il coûte se lit dans le banc : l'écart entre « sans relecture » et « avec le transfert différé pour le cache ».

`Cloth::live()` donne de quoi dessiner sans demander aucune relecture (le banc s'en sert).

**Si la carte échoue.** Le tissu garde le dernier cliché arrivé entier (`base`) et les pas envoyés depuis (`ahead`). `recover()` repart de ce cliché et refait ces pas sur le processeur. Un cliché dont la relecture échoue rend le dernier état connu, pas un tissu en miettes.

### 2. Les colonnes paresseuses (`geo::Column`)

Une colonne de géométrie peut ne pas avoir ses données du côté du processeur.

| Forme | Contenu |
|---|---|
| `Lazy::Shot` | Positions ou vitesses d'un cliché, avec éventuellement des forces uniformes posées dessus |
| `Lazy::Concat` | Plusieurs colonnes l'une après l'autre |

`Column::data()` lit les données, une seule fois. Sa taille, sa version et `ready()` se connaissent sans lire. `Column::parts()` rend les morceaux d'une colonne : une vue y cherche ce qu'elle peut dessiner sans rien lire.

Trois nodes en profitent :

| Node | Ce qu'il fait d'une colonne encore sur la carte |
|---|---|
| Solveur (tissu sur carte) | Rend `P` et `v` paresseuses |
| Forces | Si le vent est le même partout, il **note** les forces sur la colonne (`Force`) au lieu de la lire. Le solveur les applique sur la carte, par le noyau `tissu-impulsion`, avec les mêmes arrondis que le calcul du processeur |
| Réunir | Met les colonnes bout à bout sans les copier |

Tout autre node qui lit les positions (transformer, supprimer des points, un champ qui dépend de `P`) fait revenir la colonne. C'est correct, juste plus lent : la relecture n'a lieu que quand un node en a besoin.

Le solveur garde aussi son dernier pas en mémoire (`ClothSession::memo`). Redemandé avec les mêmes entrées (miniature d'un node de la zone, tableau des attributs), il rend le même cliché sans recalculer.

### 3. Un seul pas en route

L'interface et le solveur partagent la file de la carte. Avant d'envoyer un pas, le solveur regarde si le précédent est fini (`Cloth::busy()`).

| Contexte | Si la carte calcule encore |
|---|---|
| Aperçu, vue de la scène (calcul par tranches) | Le moteur rend la main. L'image sera redemandée à la prochaine image de l'interface |
| Rendu final, cache figé, tests | `Cloth::wait()` attend la fin du calcul, sans rien copier |

### Le dessin (`gpudraw`)

`paint(canvas, taille, caméra, morceaux)` dessine des maillages dans une texture (`Canvas`), que l'interface affiche comme une image.

| Élément | Sens |
|---|---|
| `Source::Live` | Le tampon de positions du solveur sert de tampon de sommets. Aucune copie |
| `Source::Data` | Positions venues du processeur, envoyées quand leur version change. Une image reprise du cache a ainsi le même aspect |
| Normales | Un noyau de calcul somme, pour chaque sommet, les normales de ses triangles. Il relit le tampon de positions |
| Ombrage | La lumière principale (GGX), le ciel, les deux faces éclairées. `Cam::scene` (`Lighting`) donne l'éclairage de la scène : ciel vu d'en haut et d'en bas, soleil, exposition, puis la courbe de tons du rasteriseur. Sans lui, un éclairage fixe de studio |
| `Surface` | Rugosité et métal du morceau, et ses textures cuites (`Baked` : couleur sRGB, rugosité et métal, moyennes). Les textures ne servent qu'avec des UV (`Part::uv`) ; elles sont envoyées une fois par clé, avec leurs niveaux réduits |
| `Soft` | La profondeur d'une image calculée ailleurs. Un pixel du maillage caché par elle n'est pas dessiné |
| Image des identifiants | `Rg32Uint` sans lissage : les bits de la profondeur et le code du morceau (`id_code` : numéro × 4 + contour). `Canvas::pick(x, y)` en relit un pixel au clic ; `Canvas::front()` la donne à la fumée |
| Contour | Une passe plein écran lit l'image des identifiants : un pixel hors d'un morceau choisi, à moins de `Cam::outline` pixels de lui, devient orange (clair : actif) |
| Ombres | `Cam::shadow` (`Shadow`). Deux cartes de profondeur vues de la lumière, orthographiques sur une boule du monde (`light_matrix`) : celle des maillages dessinés ici, refaite à chaque image, et celle des autres objets (`Occluders`), envoyés à la carte et redessinés seulement quand eux ou la lumière changent. Le maillage lit les deux (4 lectures comparées). Une passe plein écran pose ensuite l'ombre des maillages sur l'image calculée ailleurs : le point du monde vient de sa profondeur, sa normale des pixels voisins ; sans surface, le plan du sol. Elle ne pose rien là où les autres objets font déjà de l'ombre (leur ombre est dans l'image), ni sur le maillage lui-même |
| `Canvas::pixels()` | Relit l'image. Pour les tests et le banc seulement |

Par maillage, la carte garde les triangles, le voisinage des sommets (48 octets par sommet pour une grille), les normales, les couleurs et les UV : environ 100 Mo pour un million de sommets. En plus, par vue : l'image des identifiants (8 octets par pixel) et sa profondeur, deux cartes d'ombre de 2048² (1024² sur un rendu logiciel), et les textures de huit matériaux au plus.

Ordre d'une image : normales, cartes d'ombre, image des identifiants, maillages, puis ombre posée (sous) et contour (dessus).

Le dessin ne passe pas par un `PaintCallback` d'egui mais par une texture à part, enregistrée auprès d'egui (`register_native_texture`). Elle a son propre tampon de profondeur et son lissage, sans toucher à la passe de l'interface, et le même code sert sans interface.

### Où le dessin direct sert

| Vue | Ce qui est dessiné par la carte |
|---|---|
| Aperçu de l'onglet Simulation (`geoview`) | Toute géométrie qui a des faces. Points et fils restent dessinés par le processeur, sous l'image |
| Vue 3D d'une scène (`model3d`, `DirectGeo`) | Les géométries de nodes d'au moins 20 000 points (`geoscene::DIRECT_MIN`) |

Dans la scène, la vue est un rasteriseur logiciel. Elle retire ces objets de son modèle (`scene::build_ranges_without`), calcule elle-même leur géométrie par tranches (`geoscene::geo_live`), et les dessine par-dessus son image en donnant à `gpudraw` la profondeur de cette image. L'occultation est juste dans les deux sens.

Ce que la scène lui donne (`model3d`) :

| Élément | Comment |
|---|---|
| Sélection | `Viewer::direct_pick` lit un pixel de l'image des identifiants (numéro = rang de l'objet + 1), et pour une fumée un pixel de son image (assez opaque et pas noir : pas son ombre). Ensuite seulement, le rayon dans le modèle du rasteriseur |
| Contour | Celui de `gpudraw`. Le contour du rasteriseur est posé avant le dessin de la carte ; sous ses pixels, la profondeur donnée à la carte est celle de l'objet entouré (`soft_for_direct`) : un tissu derrière l'objet ne cache pas son contour, un tissu devant le cache |
| Matériau | Le graphe assigné est cuit à plat (`texexport::bake_flat`, carré des UV, 1024² ou 512² sur un rendu logiciel) dans un fil à part, quand son programme change (`DirectMat`). L'ancienne texture reste montrée pendant la cuisson |
| Lumière | `main_sun` : le premier soleil de la scène, sinon la lumière de la vue. `scene_lighting` : l'irradiance de l'HDRI vers le haut et vers le bas, son intensité, l'exposition |
| Ombres | Avec `Look::shadows`. Les autres objets sont le modèle préparé du rasteriseur (centré, à l'échelle du cadre), envoyés quand sa version change. Le sol (`Prepared::floor`) reçoit l'ombre quand l'arrière-plan est caché, comme dans le rasteriseur. La force de l'ombre posée sur l'image (`shadow_strength`) est celle que le rasteriseur donnerait à une surface claire tournée vers le haut privée de ce soleil, avec la même courbe de tons : les deux ombres se raccordent |

Limites assumées : une seule lumière fait de l'ombre ; un matériau qui dépend de la position est vu comme sur un carreau plat ; la peinture posée sur l'objet n'est pas montrée ; la fumée n'a pas de contour ; les autres objets ne reçoivent l'ombre de la fumée que par l'image du rasteriseur (un tissu dessiné par la carte ne la reçoit pas). En dessous de 20 000 points, l'objet passe par le rendu logiciel. Le rendu final et les exports relisent la géométrie et la traitent comme tout autre objet.

Tests (`gpudraw`) : `choisi_au_clic_et_entoure`, `ombre_recue_sur_la_carte` (par un autre morceau et par `Occluders`), `ombre_portee_sur_l_image_d_ailleurs` (sol et surface), `materiau_en_textures`, `matrice_de_la_lumiere`, `codes_et_textures` ; `texexport::cuisson_a_plat_pour_la_carte`.

`ARY_DIRECT=toujours` ou `jamais` force le choix. `ARY_DBG_RELECTURE=1` écrit dans le terminal la pile d'appels de chaque relecture : de quoi trouver quel node fait revenir une colonne.

## Ce qui manque

- **Auto-collision par triangles.** Elle se fait entre sommets, avec une épaisseur de l'ordre de la maille. Des faces, puis des bornes de déplacement par sommet (OGC), permettraient une épaisseur fine.
- **Frottement entre les couches** du tissu.
- **Collision avec un maillage** quelconque.
- **Fumée : grille creuse, montée en résolution, obstacles en maillage.** Voir les limites de la fumée plus haut.
- **Fluides, corps rigides, cheveux, destruction** : rien n'est branché.
- **Affichage direct des particules et des fils.** Seules les surfaces (géométries avec faces) sont dessinées depuis la carte. Points et arêtes passent encore par le processeur.
- **Dessin direct : plusieurs lumières qui font de l'ombre, matériau évalué en 3D**, et l'ombre de la fumée sur un maillage dessiné par la carte. Voir les limites plus haut.
- **Sauter le transfert quand le cache ne sert pas.** Chaque pas demande encore sa relecture (jamais attendue). Le banc mesure ce qu'elle coûte : ligne « avec le transfert différé pour le cache ».
- **Cache disque sur la carte.** Le cache figé passe par les colonnes du processeur.

La suite est résumée dans la [feuille de route du moteur](feuille-de-route-moteur.md).
