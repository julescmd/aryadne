# Géométrie et simulation en nœuds

Fichier : `.arygeo`. Dans l'esprit des Geometry Nodes de Blender et de Houdini, en plus simple.

Un graphe de géométrie fabrique des **points**, leur donne des **attributs** (une couleur, une vitesse), les relie par des **arêtes**, et peut les faire bouger dans le temps avec une **zone de simulation**.

Pour les gestes de l'éditeur, voir [L'éditeur de nœuds](noeuds-editeur.md). Pour le tissu en détail, voir [Simulation de tissu](simulation-tissu.md).

## Créer

Palette de commandes : « New geometry or simulation (nodes) ». Le fichier s'ouvre sur un exemple complet : un tissu accroché par le haut, qui tombe sur une boule.

Quatre exemples sont fournis :

| Exemple | Ce qu'il montre |
|---|---|
| Cloth on a ball | Une source, un champ (ce qui est épinglé), une zone de simulation, un solveur |
| Cloth on the graphics card | Le même, plus fin, calculé sur la carte graphique, posé sur une boule et une boîte |
| Particle fountain | Des particules qui jaillissent, retombent, rebondissent et changent de couleur en vieillissant |
| Animated waves | Un champ qui lit le temps : une hauteur par point |

## L'écran

| Zone | Contenu |
|---|---|
| Le graphe | Les nœuds |
| L'aperçu 3D | Les points et les arêtes de la sortie, ou du nœud choisi |
| La barre de lecture | Lecture, image courante, barre de cache |
| **Attributes** | Le tableau des attributs : une ligne par point, une colonne par attribut |

### L'aperçu

| Geste | Effet |
|---|---|
| Glisser | Tourner |
| Clic droit glissé | Déplacer |
| Molette | Zoom |
| Double-clic | Recadrer |
| `Espace` (souris sur l'aperçu) | Lire ou mettre en pause |

Sous l'aperçu, une ligne indique le nombre de points et d'arêtes, et ce que le dernier calcul a coûté : combien de nœuds ont été `recomputed` et combien `from cache`.

### La lecture

| Bouton | Effet |
|---|---|
| `Back to the first frame` | Revient à la première image |
| `Play or pause (Space)` | Lecture |
| `Length, frame rate and cache` | Ouvre les réglages : `First frame`, `Last frame`, `Frames per second`, la taille du `Cache in memory`, et `Clear the cache` |
| `Frame the preview on the geometry` | Recadre |

La **barre de cache** teinte chaque image de simulation déjà calculée. Clique ou glisse dessus pour te déplacer dans le temps.

Pendant un calcul long : « Simulating… ». L'interface reste utilisable. Le calcul d'une image s'arrête après un court délai et reprend à l'image suivante de l'écran.

## Les trois idées à retenir

1. **Une géométrie est un tableau.** Chaque point est une ligne. Chaque attribut est une colonne : `P` (la position), `v` (la vitesse), `Cd` (la couleur)…
2. **Rien ne se calcule pour rien.** Changer un réglage ne refait que ce qui en dépend. Le reste vient du cache.
3. **Une simulation est un état qui passe d'une image à la suivante.** Demander l'image 80 repart du dernier état connu avant elle.

## Sources

| Nœud | Entrées | Ce qu'il crée |
|---|---|---|
| `Grid` | `Width`, `Depth`, `Points across`, `Points deep` | Une grille de points. `Plane` : `Floor (XZ)` ou `Wall (XY)`. `Edges` : oui, `With diagonals`, ou non |
| `Points on a sphere` | `Points`, `Radius` | Des points répartis sur une sphère |
| `Circle` | `Points`, `Radius` | Un cercle |
| `Line` | `Points`, `Start`, `End` | Une ligne, avec ses arêtes |
| `Point cloud` | `Points`, `Scale`, `Seed` | Des points au hasard dans une boîte |

Pour un tissu, choisis `Edges › With diagonals` : les diagonales empêchent la grille de se cisailler.

## Opérations

| Nœud | Entrées | Effet |
|---|---|---|
| `Move, rotate` | `Translation`, `Rotation`, `Scale` | Déplace, tourne et redimensionne toute la géométrie |
| `Set point positions` | `Select`, `Location`, `Offset` | Place les points choisis, ou les décale |
| `Set an attribute` | `Value`, `Select`. Réglages : `Name`, `Type` (couleur, nombre, vecteur) | Écrit un attribut |
| `Merge` | `Geometry A`, `Geometry B` | Réunit deux géométries |
| `Delete points` | `Select` | Supprime les points choisis |
| `Copy onto points` | `Points`, `Model`, `Scale` | Pose une copie du modèle sur chaque point |

`Copy onto points` refuse de dépasser 8 millions de points.

## Lire la géométrie

| Nœud | Sortie | Rôle |
|---|---|---|
| `Location` | Un vecteur | La position de chaque point |
| `Point index` | Un nombre | Le numéro de chaque point |
| `Read an attribute` | Valeur, vecteur | Un attribut par son `Name` |

## Champs

Dans un graphe de géométrie, une entrée de nombre, de vecteur ou de couleur accepte une valeur **par point**. On appelle cela un champ.

Tous les nœuds de calcul des matériaux servent à construire des champs : `Math`, `Vector math`, `Separate XYZ`, `Compare`, `Noise`, `Voronoi`, `Random`, `Expression`, `Time`…

### Exemple : épingler le bord d'un tissu

Dans l'exemple de départ, les points du fond sont épinglés ainsi :

1. `Location` donne la position de chaque point.
2. `Separate XYZ` en tire Z.
3. `Compare`, mode « inférieur », avec -0,98 : vrai pour les points dont z est tout au fond.
4. Ce résultat entre dans `Select` d'un nœud `Set an attribute`, nom `pin`, type nombre.

Seuls les points choisis reçoivent `pin = 1`. Le solveur les laisse fixes.

### Exemple : des vagues

1. `Grid`, sans arêtes.
2. `Location` vers `Separate XYZ`.
3. `Math` (sinus) sur `X + Time.Seconds`.
4. Le résultat entre dans la composante Y de l'`Offset` d'un `Set point positions`.

Chaque point monte et descend selon sa place et le temps.

## Attributs connus

Tu peux nommer un attribut comme tu veux. Certains noms sont lus par les nœuds.

| Nom | Type | Lu par | Sens |
|---|---|---|---|
| `P` | Vecteur | Tous | La position. Toujours un vecteur |
| `v` | Vecteur | Simulation | La vitesse |
| `Cd` | Couleur | L'aperçu | La couleur du point. C'est le nom proposé par défaut |
| `N` | Vecteur | L'aperçu | La normale |
| `pin` | Nombre | Les solveurs | Point fixe si supérieur à 0,5 |
| `mass` | Nombre | Le tissu sur carte graphique | La masse du point. Sans elle : 0,2 kg/m² (un coton léger) |
| `age` | Nombre | `Emit particles` | L'âge de la particule, en secondes |
| `id` | Nombre | `Emit particles` | Un numéro unique par particule |
| `rest` | Nombre, sur les arêtes | Les solveurs | La longueur au repos de chaque arête, mesurée au premier pas |

## Zone de simulation

`Add › Simulation › Simulation zone` pose deux nœuds : le début et `Simulation end`.

- Ce qui est **entre les deux** s'exécute une fois par image.
- Le début donne l'état de l'image d'avant. La fin reçoit le nouvel état.
- À la première image, l'état vient de ce qui est branché à l'entrée du début.
- Le début a aussi une sortie `Time step` : la durée d'une image, à brancher sur les nœuds de simulation.

**Simulation state** : ce qui passe d'une image à la suivante. Par défaut, une géométrie. `Add a socket` ajoute d'autres valeurs à transporter.

### Nœuds de simulation

Ils vont dans une zone de simulation.

| Nœud | Entrées | Effet |
|---|---|---|
| `Forces` | `Gravity` (0, -9,8, 0), `Wind`, `Air drag`, `Time step` | Change la vitesse des points |
| `Move forward` | `Time step` | Déplace les points selon leur vitesse |
| `Collision` | `Floor height`, `Bounce`, `Grip`, `Ball center`, `Ball radius` | Fait rebondir les points sur le sol et sur une boule |
| `Emit particles` | `State`, `Source`, `Per frame`, `Speed`, `Scatter`, `Lifetime (s)`, `Time step` | Fait naître des particules à chaque image, les fait vieillir, retire celles qui ont fait leur temps |
| `Solver` | Voir [Simulation de tissu](simulation-tissu.md) | Tissu et fils |

`Emit particles` refuse de dépasser 4 millions de particules.

### Chaînes types

**Particules.** Début de zone → `Emit particles` → `Forces` → `Move forward` → `Collision` → fin de zone.

**Tissu.** Début de zone → `Forces` → `Solver` → fin de zone. Le solveur déplace lui-même les points : pas de `Move forward`.

### Voir un nœud de l'intérieur de la zone

Choisir un nœud du contenu de la zone le montre tel qu'il agit à l'image courante : avec, en entrée, l'état de l'image d'avant.

## Cache

### En mémoire

Chaque image calculée est gardée tant que rien ne change en amont. Le cache a un budget (512 Mo par défaut). Quand il est plein, ce qui coûte le moins cher à refaire s'en va en premier.

Changer un réglage **en amont** de la zone vide ses images : elles ne sont plus justes. Changer un nœud **en aval** (une couleur posée après la simulation) ne recalcule pas la simulation.

### Sur le disque

Sur le nœud `Simulation end` :

| Bouton | Effet |
|---|---|
| `Bake to disk` | Calcule toutes les images et les écrit dans le coffre. La zone se relit ensuite sans calcul, même si l'amont change |
| `Release the cache` | La zone se recalcule de nouveau d'après ses réglages. Les fichiers sont supprimés |

État affiché : « Baked: frames come from disk », ou « In memory: each computed frame is kept as long as nothing changes upstream ».

Les fichiers vont dans `.cache/<nom du graphe>/zone-<numéro>/`, à la racine du coffre : un fichier `.arygeocache` par image. Voir [Formats](../technique/formats.md#cache-de-géométrie-arygeocache).

## La sortie

`Geometry output` : ce qui y arrive est ce que montre l'aperçu.

## Limites

- **Des points et des arêtes, pas de faces.** La géométrie en nœuds ne porte pas de triangles. Un tissu est montré par ses points et ses arêtes.
- **Pas de lien avec la scène 3D.** Dans le code actuel, un `.arygeo` se regarde dans son propre aperçu. Il ne s'importe pas dans une `.aryscene` et ne se rend pas avec le tracé de chemins. Le cache `.arygeocache` est un format propre à Aryadne, sans export Alembic ou USD.
- **Solveurs.** Tissu et fils seulement. Pas de fluides, de fumée, de corps rigides ni de cheveux. Voir la [feuille de route du moteur](../technique/feuille-de-route-moteur.md).
- **Collisions.** Un sol, une boule, une boîte. Pas de collision avec un modèle quelconque, pas d'auto-collision du tissu.
- Une zone de simulation ne peut pas se trouver dans une zone de répétition.
