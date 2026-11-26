# Géométrie et simulation en nœuds

Fichier : `.arygeo`. Dans l'esprit des Geometry Nodes de Blender et de Houdini, en plus simple.

Un graphe de géométrie fabrique des **points**, leur donne des **attributs** (une couleur, une vitesse), les relie par des **arêtes**, et peut les faire bouger dans le temps avec une **zone de simulation**.

Ces fichiers s'ouvrent dans l'outil `Simulation` de l'étape 3D, à côté de `Scene` et `Materials`.

Pour les gestes de l'éditeur, voir [L'éditeur de nœuds](noeuds-editeur.md). Pour le tissu en détail, voir [Simulation de tissu](simulation-tissu.md).

## Créer

Palette de commandes : « New geometry or simulation (nodes) ». Le fichier s'ouvre sur un exemple complet : un tissu accroché par le haut, qui tombe sur une boule.

Six exemples sont fournis :

| Exemple | Ce qu'il montre |
|---|---|
| Cloth on a ball | Une source, un champ (ce qui est épinglé), une zone de simulation, un solveur |
| Cloth on the graphics card | Le même, plus fin, calculé sur la carte graphique, posé sur une boule et une boîte |
| Cloth folding onto itself | Un tissu debout qui s'affaisse et se replie sans se traverser (auto-collision) |
| Flag in the wind | Un drapeau tenu par son bord, dans un vent à rafales. Fait pour être posé dans une scène 3D |
| Particle fountain | Des particules qui jaillissent, retombent, rebondissent et changent de couleur en vieillissant |
| Animated waves | Un champ qui lit le temps : une hauteur par point |

## L'écran

| Zone | Contenu |
|---|---|
| Le graphe | Les nœuds |
| L'aperçu 3D | Les points et les arêtes de la sortie, ou du nœud choisi. La surface ombrée se voit dans une [scène 3D](#dans-une-scène-3d) |
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
| `Grid` | `Width`, `Depth`, `Points across`, `Points deep` | Une grille de points. `Plane` : `Floor (XZ)` ou `Wall (XY)`. `Edges` : oui, `With diagonals`, ou non. `Faces` : oui ou non |
| `Points on a sphere` | `Points`, `Radius` | Des points répartis sur une sphère |
| `Circle` | `Points`, `Radius` | Un cercle |
| `Line` | `Points`, `Start`, `End` | Une ligne, avec ses arêtes |
| `Point cloud` | `Points`, `Scale`, `Seed` | Des points au hasard dans une boîte |

Pour un tissu, choisis `Edges › With diagonals` : les diagonales empêchent la grille de se cisailler.

### Faces et UV

`Grid` crée aussi des **faces** (un quad par case) et des **UV** (l'attribut `uv`, de 0 à 1 d'un bord à l'autre). Les solveurs ne s'en servent pas. Elles servent à la scène 3D : c'est grâce à elles qu'un tissu y est ombré, reçoit un matériau et sort au rendu.

- Les faces suivent les points à travers tous les nœuds : `Move, rotate`, `Set point positions`, `Set an attribute`, la zone de simulation et son cache les laissent telles quelles. `Merge` les réunit. `Copy onto points` les recopie sur chaque copie. `Delete points` retire les faces qui perdent un sommet.
- Les autres sources (`Points on a sphere`, `Circle`, `Line`, `Point cloud`) ne créent pas de faces.
- Un graphe enregistré avant l'arrivée des faces s'ouvre comme avant, avec les faces en plus. `Faces › no` rend l'ancienne grille, points et arêtes seulement.

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
| `Cd` | Couleur | L'aperçu, la scène 3D | La couleur du point. C'est le nom proposé par défaut. Dans une scène, c'est la couleur de l'objet tant qu'il n'a pas de matériau |
| `uv` | Deux nombres | La scène 3D | Les coordonnées de texture, créées par `Grid` |
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
| `Scene object` | Réglage `Object` : le nom d'un objet de la scène. `Center outside a scene`, `Size outside a scene`, `Margin` | Donne la boîte de cet objet : `Center`, `Radius`, `Size`. Voir [Dans une scène 3D](#dans-une-scène-3d) |

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

`Geometry output` : ce qui y arrive est ce que montre l'aperçu, et ce que la scène 3D affiche.

## Dans une scène 3D

Un `.arygeo` se pose dans une scène comme un objet.

| Geste | Effet |
|---|---|
| Menu `Add › Node geometry › New simulated flag` | Crée un graphe de drapeau à côté de la scène et le pose |
| Menu `Add › Node geometry`, liste `FROM THE VAULT` | Pose une géométrie du coffre |
| Glisser un `.arygeo` du panneau des fichiers sur la vue | Pareil |

Dans la liste de la scène, son icône est une vague. Ce qu'il sait faire :

- **Transformation.** Position, rotation, taille, clés, parent : comme les autres objets.
- **Timeline.** L'image de la scène pilote l'image de la simulation. À la première image de la scène, la simulation est à sa première image. Si la scène et le graphe n'ont pas la même cadence, c'est le même instant qui est montré.
- **Départ.** Onglet `Object`, section `Simulation`, réglage `Start (frames)`. 10 : la simulation part à la dixième image de la scène. -10 : elle a déjà tourné dix images quand la scène commence. `Simulation frame` indique l'image montrée et la durée de la simulation.
- **Cache.** Si la zone de simulation est figée sur le disque, la scène lit ce cache. Sinon elle calcule, et garde en mémoire ce qu'elle a calculé : lire la timeline dans l'ordre ne coûte qu'un pas par image.
- **Lien vivant.** La scène garde le chemin du graphe. Enregistre le graphe : la scène le relit. L'icône du graphe, dans la section `Simulation`, l'ouvre dans l'onglet **Simulation**.
- **Surface.** Les faces sont ombrées. La couleur `Cd` sert de couleur tant qu'il n'y a pas de matériau.
- **Matériau.** Onglet `Material`, comme pour les autres objets. Le matériau lit les UV de `Grid` : dans son graphe, relie la sortie `uv` de `Texture coordinates` aux `Coordinates` d'un motif pour qu'il suive le tissu.
- **Rendu.** L'objet sort dans l'aperçu, dans le tracé de chemins, dans le rendu final (`F12`) et dans `Capture › Animation as video (MP4)…`, image par image.
- **Export.** L'icône de téléchargement de la section `Simulation` écrit une suite d'OBJ : un fichier par image de la scène, dans un dossier `<scène> - <objet> (OBJ)` à côté de la scène. Les points sont dans le repère de la scène, les quads restent des quads, les UV sont écrites.

Une géométrie sans face (particules, fils) est montrée par un petit tétraèdre à chaque point.

### Un objet de la scène comme obstacle

Le nœud `Scene object` fait entrer un objet de la scène dans le graphe.

1. Dans le réglage `Object`, écris le nom de l'objet, tel qu'il est dans la liste de la scène.
2. Pour une boule : relie `Center` à `Ball center` et `Radius` à `Ball radius` du `Solver`.
3. Pour une boîte : relie `Center` à `Box center` et `Size` à `Box size`.

À chaque image, le nœud donne la boîte qui entoure l'objet, dans le repère de la géométrie. Si l'objet est animé, la simulation le voit bouger.

`Margin` grossit un peu l'obstacle (2 cm au départ). Sans marge, le tissu affleure l'objet et l'objet transparaît entre ses points. Un tissu grossier demande plus de marge.

Hors d'une scène (dans l'onglet **Simulation**), ou si aucun objet ne porte ce nom, le nœud donne ses deux entrées, `Center outside a scene` et `Size outside a scene`. Tu peux donc régler le tissu sans la scène.

Ce que ce nœud ne fait pas :

- Il donne une boîte, pas la forme. Une sphère est exacte. Un cube droit est exact. Un cube tourné devient la boîte, plus grande, qui l'entoure. Un personnage devient sa boîte ou la boule qui l'entoure.
- La boîte est celle du modèle d'origine : les modificateurs et le squelette de l'objet ne comptent pas.
- Le `Solver` n'a qu'une boule et qu'une boîte. Deux nœuds `Scene object` peuvent en remplir une chacun, pas plus.
- Changer l'objet ou son animation recalcule la simulation depuis le début.
- Une géométrie de nodes ne peut pas en regarder une autre.

## Limites

- **L'aperçu montre des points et des arêtes.** Les faces existent, mais l'aperçu de l'onglet **Simulation** ne les dessine pas. Pour voir la surface, pose la géométrie dans une scène.
- **Faces.** Seul `Grid` en crée. Pas de nœud pour en créer à la main, trianguler ou extruder.
- **UV.** Une valeur par point : pas de couture. C'est assez pour une grille.
- **Dans la scène.** L'objet ne se modifie pas en mode édition : il se modifie dans son graphe. Un modificateur posé sur lui marche, mais lui fait perdre ses UV. Chaque changement d'image reconstruit la scène : un tissu très fin ralentit la lecture.
- **Export.** Suite d'OBJ seulement. Pas d'Alembic ni d'USD : ce sont des conteneurs binaires qui demandent une bibliothèque entière, alors qu'une suite d'OBJ se relit partout (Blender, Houdini, moteurs de jeu). Le cache `.arygeocache` reste un format propre à Aryadne.
- **Solveurs.** Tissu et fils seulement. Pas de fluides, de fumée, de corps rigides ni de cheveux. Voir la [feuille de route du moteur](../technique/feuille-de-route-moteur.md).
- **Collisions.** Un sol, une boule, une boîte, réglés à la main ou donnés par un objet de la scène. Pas de collision avec la forme exacte d'un modèle.
- Une zone de simulation ne peut pas se trouver dans une zone de répétition.
