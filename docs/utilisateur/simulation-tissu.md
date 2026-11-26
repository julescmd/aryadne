# Simulation de tissu

Le tissu se simule dans un graphe de [géométrie en nœuds](noeuds-geometrie.md) (`.arygeo`), avec le nœud `Solver` placé dans une zone de simulation.

## Le plus court chemin

1. Palette de commandes : « New geometry or simulation (nodes) ».
2. Le graphe de départ est déjà un tissu qui tombe sur une boule.
3. Souris sur l'aperçu, `Espace` : la simulation se lit.

Pour l'essayer sur la carte graphique, ouvre l'exemple « Cloth on the graphics card ».

## Ce qu'il faut dans le graphe

| Élément | Nœud | Rôle |
|---|---|---|
| Un maillage de points reliés | `Grid`, avec `Edges › With diagonals` | Le tissu. Les arêtes sont les fils |
| Des points fixes | `Set an attribute`, nom `pin`, type nombre | Ce qui tient le tissu. Sans points fixes, il tombe en entier |
| Une zone de simulation | `Simulation zone` | Fait passer l'état d'une image à la suivante |
| Des forces | `Forces` | La gravité, le vent |
| Le solveur | `Solver` | Garde les fils à leur longueur et gère les obstacles |

Chaîne dans la zone : début → `Forces` → `Solver` → fin. Branche la sortie `Time step` du début sur l'entrée `Time step` de `Forces` et de `Solver`.

La longueur au repos de chaque fil est mesurée au premier pas : le tissu garde la forme qu'il avait en entrant dans la zone.

## Deux solveurs

Le réglage `Solver` du nœud propose deux méthodes.

| | `Cloth and threads, XPBD (CPU)` | `Cloth (graphics card)` |
|---|---|---|
| Méthode | XPBD : contraintes de distance le long des arêtes | VBD (Vertex Block Descent) : raideur et flexion réglables |
| Calculé sur | Le processeur | La carte graphique. Sur le processeur s'il n'y en a pas |
| Réglage de la tenue | `Compliance` | `Stiffness`, `Shear`, `Bending`, `Substeps` |
| Flexion | Non réglable | Réglable |
| Auto-collision | Non | Oui, en option |
| Convient à | Petits tissus, fils, apprendre | Tissus fins, beaucoup de points |

Les entrées qu'un solveur n'utilise pas sont montrées en retrait sur le nœud.

Dans l'aperçu, un bouton ouvre les réglages du tissu. Son libellé dit où se fait le calcul : « Cloth: graphics card solver (settings) » ou « Cloth computed on the CPU (settings, switch to the graphics card) ». Ce menu reste ouvert pendant que tu règles.

## Réglages communs

| Réglage | Défaut | Effet |
|---|---|---|
| `Iterations` | 16 | Passes de calcul par image. Plus il y en a, moins le tissu paraît élastique. De 1 à 2000 |
| `Damping` | 0,02 | Part de vitesse perdue à chaque image, comme un frein de l'air. Plus haut : le tissu tombe et se balance moins vite |
| `Time step` | 1/24 | La durée d'une image. À relier à la sortie `Time step` de la zone |
| `Floor` | oui | Active le sol |
| `Floor height` | 0 | Hauteur du sol |
| `Ball center`, `Ball radius` | rayon 0 | Une boule obstacle. Rayon 0 : pas de boule |
| `Box center`, `Box size` | taille 0 | Une boîte obstacle. Taille 0 : pas de boîte |
| `Friction` | 0,4 | Frottement au contact des obstacles. 0 : glisse. Plus haut : accroche. Voir plus bas pour le tissu sur carte graphique |

Les obstacles sont résolus en même temps que les fils. Ils n'étirent donc pas le tissu après coup.

## Réglages du XPBD (processeur)

| Réglage | Défaut | Effet |
|---|---|---|
| `Compliance` | 0 | Souplesse des fils. 0 : le tissu ne s'étire pas. Plus grand : il devient élastique |

Le pas de temps est coupé en petits pas, chacun avec peu de passes. À nombre de passes égal, le tissu s'étire bien moins qu'avec un seul grand pas.

## Réglages du tissu sur carte graphique (VBD)

| Réglage | Défaut | Repères | Effet |
|---|---|---|---|
| `Stiffness` | 1000 | 50 : jersey. 1000 : coton. 5000 : jean | Résistance à l'étirement, en N/m |
| `Shear` | 0,1 | 0,03 : très souple en biais. 0,1 : coton. 1 : papier | Résistance à la déformation en biais, en part de `Stiffness` |
| `Bending` | 0,1 | 0 : soie. 0,1 : coton. 1 : toile épaisse. 10 : carton | Résistance au pli |
| `Fold damping` | 0,3 | 0 : aucun. 0,3 : coton. 1 : très amorti | Les plis cessent de frémir. Ne ralentit ni la chute ni le balancement |
| `Friction` | 0,4 | 0 : glisse. 0,4 : coton sur du bois. 1 : accroche fort | Coefficient de frottement sur les obstacles |
| `Substeps` | 8 | De 1 à 64 | L'image est coupée en sous-pas |
| `Self-collision` | non | | Le tissu ne se traverse plus lui-même quand il se replie |
| `Thickness` | 0 | 0 : choisie d'après la maille | Distance gardée entre deux couches du tissu, en mètres |

**Pourquoi le cisaillement.** Un tissu tissé se déforme bien plus facilement en biais que dans le sens des fils. C'est ce qui lui permet d'épouser une boule ou une épaule. Avec `Shear` à 1, il ne peut que se plier comme une feuille de papier. Ce réglage agit sur les diagonales de la grille : il faut donc `Edges › With diagonals`.

**Le frottement.** Le tissu glisse d'une pente raide et tient sur une pente douce. À 0,4, il tient jusqu'à une pente d'environ 20 degrés. Il ne colle plus là où il touche : il peut glisser le long d'un obstacle pour se détendre.

**Deux amortissements.** `Damping` freine tout mouvement, comme l'air. `Fold damping` ne freine que les mouvements des plis entre eux : le tissu se pose et se calme, sans tomber plus lentement.

**Itérations et sous-pas.** Les itérations sont réparties entre les sous-pas. Avec 32 itérations et 8 sous-pas, chaque sous-pas fait 4 passes. Beaucoup de sous-pas avec peu d'itérations chacun donnent le tissu le plus ferme, et les contacts les plus propres.

**Une correction d'ensemble à chaque sous-pas.** Avant ses passes, chaque sous-pas corrige l'étirement du tissu entier à l'aide de versions plus grossières du même tissu. C'est ce qui garde un tissu fin aussi ferme qu'un tissu grossier, sans des centaines d'itérations. Elle coûte à peu près deux passes, qui sont retirées des itérations demandées (il en reste toujours au moins une par sous-pas).

### Auto-collision

Active `Self-collision` dans les réglages du tissu (le bouton de l'aperçu), ou sur le nœud.

- Le tissu est fait de points : ce sont les points qui se repoussent. `Thickness` est la distance qu'ils gardent entre eux.
- À 0, l'épaisseur vaut 0,9 fois la taille d'une maille. **Ne descends pas en dessous de 0,7 maille** : une couche passerait entre les points d'une autre.
- Un tissu plus fin (plus de points) donne donc des couches plus rapprochées.
- Deux points voisins dans le tissu ne se repoussent jamais : il se froisse librement.
- Si le tissu se traverse quand même, monte `Substeps`. Un pli qui se referme vite peut franchir l'épaisseur en un seul sous-pas.
- C'est plus lent : le [banc de mesure](banc-gpu.md) donne le surcoût sur ta carte.

**Masse.** Sans attribut `mass`, chaque point reçoit une masse d'après la surface qu'il couvre, à 0,2 kg/m² (un coton léger). Un attribut `mass` par point la remplace.

### Ce que dit l'aperçu

Sous les réglages, une ligne résume le dernier pas :

- le nom de la carte ;
- le nombre de `points` ;
- le nombre de `colors` ;
- le nombre de `passes` ;
- le temps du pas en millisecondes.

Sans carte utilisable : « No usable graphics card: cloth computed on the CPU ». Avant la première lecture : « Press play to see where the computation runs ».

**Les couleurs.** Le solveur traite un point à la fois, ses voisins étant figés. Deux points sans fil entre eux peuvent être traités en même temps. Les points sont donc rangés par « couleurs » : tous les points d'une couleur sont calculés ensemble. Moins il y a de couleurs, plus le calcul est rapide. Une grille avec diagonales en demande quelques-unes.

## Régler un tissu, pas à pas

1. **Le tissu s'étire comme du caoutchouc.** Monte `Iterations`. En VBD, monte aussi `Substeps`, ou `Stiffness`.
2. **Le tissu explose ou tremble.** Baisse `Stiffness`, ou monte `Substeps`. Vérifie que `Time step` est bien relié à la zone.
3. **Le tissu traverse la boule.** Monte `Substeps`. Un tissu qui tombe vite peut franchir un obstacle en un seul pas.
4. **Le tissu glisse trop, ou pas assez.** Règle `Friction`.
5. **Les plis sont trop mous ou trop raides.** En VBD, règle `Bending`. En XPBD, la flexion ne se règle pas : seule la finesse de la grille compte.
6. **Le tissu ressemble à du papier.** Baisse `Shear` (0,1 ou moins), et `Bending`. Vérifie que la grille a ses diagonales.
7. **Le tissu ne se calme pas.** Monte `Fold damping`. Si tout le tissu oscille, monte `Damping`, ou `Air friction` dans le nœud `Forces`.
8. **Le tissu passe à travers lui-même.** Active `Self-collision`.
9. **C'est trop lent.** Réduis le nombre de points de la grille, ou passe au solveur sur carte graphique.

## Lecture et cache

- La barre de cache teinte les images déjà calculées. Voir [Géométrie et simulation](noeuds-geometrie.md#cache).
- Revenir en arrière relit le cache. Aller à une image lointaine recalcule depuis le dernier état connu.
- `Bake to disk`, sur le nœud `Simulation end`, écrit toutes les images dans le coffre. Le tissu se relit alors sans calcul.

Changer un réglage du solveur vide le cache de la zone : les images déjà calculées ne valent plus.

## Sans carte graphique

Le solveur « graphics card » marche quand même. Le même calcul est fait sur le processeur, plus lentement. C'est aussi cette version qui sert de référence dans les tests du code.

Le repli se fait tout seul dans trois cas : pas de carte, carte trop limitée pour ce tissu, ou erreur du pilote pendant le calcul.

Pour connaître ce que ta carte sait faire, lance le [banc de mesure](banc-gpu.md).

## Limites connues

- **Auto-collision entre points.** Pas entre faces : l'épaisseur ne peut pas être plus fine que la maille, et il n'y a pas de frottement entre les couches.
- **Obstacles simples.** Un sol, une boule, une boîte. Pas de collision avec un personnage ou un modèle quelconque.
- **Pas de couture ni de patron.** On ne coud pas deux pièces de tissu.
- **Affichage en points et en arêtes.** Le tissu n'a pas de faces : il ne se rend pas comme une surface dans la scène 3D. Voir les limites de la [géométrie en nœuds](noeuds-geometrie.md#limites).
- **Taille.** Un tissu trop gros pour les tampons de la carte est calculé sur le processeur. La limite dépend de la carte.
- **Avec très peu d'itérations par sous-pas** (moins de 4), l'accélération du calcul est coupée pour éviter que le tissu explose. C'est voulu.

Pour le fonctionnement interne, voir [Simulation sur carte graphique](../technique/simulation-gpu.md).
