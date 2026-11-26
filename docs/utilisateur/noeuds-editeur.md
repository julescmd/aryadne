# L'éditeur de nœuds

Un graphe de nœuds est une suite de petites boîtes reliées par des fils. Chaque boîte fait une chose. Les données vont de gauche à droite, jusqu'à un nœud de sortie.

Le même éditeur sert à trois genres de fichiers :

| Fichier | Sert à | Page |
|---|---|---|
| `.arymat` | Un matériau pour la 3D : le graphe d'un calque de l'[Atelier](3d-atelier.md) | [Matériaux en nœuds](noeuds-materiaux.md) |
| `.arycomp` | Retoucher une image, étalonner un clip | [Compositing](noeuds-compositing.md) |
| `.arygeo` | Des points, des attributs, une simulation | [Géométrie et simulation](noeuds-geometrie.md) |

## L'écran

- **La bibliothèque de nœuds**, à gauche (panneau `Node library`) : la recherche, puis tous les nœuds rangés par catégories.
- **Le graphe**, au centre.
- **L'aperçu et les réglages**, sur le côté (panneau `Preview and settings (nodes)`) : le résultat en direct, puis les réglages du nœud choisi.
- **Une miniature** sous chaque nœud montre ce qui en sort.

## Naviguer

| Geste | Effet |
|---|---|
| Molette | Zoom |
| Glisser le fond avec le bouton du milieu, ou `Espace` + glisser | Déplacer la vue |
| `F` | Cadrer la sélection |
| `Origine` | Tout cadrer |

## Ajouter un nœud

- `Maj+A`, ou clic droit dans le vide puis `Add a node…`.
- Le menu est rangé par catégories. Survole une catégorie pour voir ses nœuds.
- Tape pour chercher : `Search for a node…`. `↑` `↓` choisissent, `Entrée` pose.
- Tirer un lien et le lâcher dans le vide ouvre le même menu. Le nœud choisi arrive déjà relié.
- Depuis la bibliothèque, à gauche : glisse un nœud dans le graphe, ou clique-le. Voir ci-dessous.
- Une image glissée depuis l'explorateur devient un nœud `Image texture` (ou `Image`, ou `Video texture`). Lâchée sur un nœud qui prend un fichier, elle remplace son image.

### La bibliothèque de nœuds

La liste à gauche du graphe montre tous les nœuds utilisables ici.

| Geste | Effet |
|---|---|
| Glisser un nœud dans le graphe | Il se pose là où on le lâche. Un fantôme suit la souris pendant le glisser |
| Le lâcher sur un lien | Il s'insère dans ce lien, si les types vont ensemble. Le lien s'allume et le fantôme le dit avant qu'on lâche |
| Clic sur un nœud | Il arrive au centre du graphe |
| Clic sur le nom d'une catégorie | Replie ou déplie la catégorie. Le nombre à droite compte ses nœuds |
| Taper dans `Search nodes` | Les meilleurs résultats d'abord, toutes catégories confondues. Les noms français et anglais et les opérations (« multiplier ») sont compris |
| `Échap` pendant un glisser | Annule |

Pour s'insérer, un nœud doit avoir une entrée qui accepte ce qui arrive par le lien et une sortie qui convient à sa destination. Un nœud sans entrée (des coordonnées, une texture) ne s'insère pas : il se pose à côté. Une géométrie ne passe que par un nœud de géométrie.

Les catégories repliées le restent dans tous les graphes ouverts, le temps de la session.

Le panneau se détache comme les autres, par sa poignée. Voir [Fenêtres et dispositions](fenetres-et-dispositions.md). Détaché dans sa propre fenêtre, le glisser ne traverse pas d'une fenêtre à l'autre : le clic, lui, marche toujours.

### Les catégories

| Catégorie | Contenu |
|---|---|
| `Input` | Valeurs, couleurs, coordonnées, textures, temps |
| `Pattern` | Bruits et motifs procéduraux |
| `Wear` | Masques d'usure (matériaux) |
| `Color` | Mélanges et corrections de couleur |
| `Math` | Calculs, vecteurs, comparaisons, logique |
| `Filter` | Flous et effets d'image (compositing) |
| `Shader` | Le nœud de code |
| `Geometry` | Sources et opérations sur les points |
| `Simulation` | Forces, collisions, solveur, zone de simulation |
| `Structure` | Renvoi, zone de répétition, groupes |
| `Output` | Le nœud de sortie |

Un graphe ne propose que les nœuds qui lui conviennent.

## Relier

- Glisse d'une **sortie** (à droite d'un nœud) vers une **entrée** (à gauche d'un autre).
- Une entrée ne reçoit qu'un seul lien. Une sortie peut en nourrir plusieurs.
- Une entrée sans lien se règle à la main, directement sur le nœud.
- `Ctrl+clic` ou clic droit sur un lien le coupe.
- `Ctrl` pendant un déplacement aimante les nœuds.

### Les couleurs des prises

| Type | Ce qui y passe |
|---|---|
| Nombre à virgule | Une valeur |
| Entier | Un nombre entier |
| Vrai / faux | 0 ou 1 |
| Vecteur | Trois nombres : position, direction, vitesse |
| Couleur | Rouge, vert, bleu, alpha |
| Géométrie | Des points, leurs attributs, des arêtes |

Les nombres, vecteurs et couleurs se branchent entre eux : la valeur est convertie. Une géométrie ne se branche que sur une géométrie. Sinon un message le dit.

## Travailler sur les nœuds

| Geste | Raccourci | Effet |
|---|---|---|
| Choisir | Clic, `Maj+clic`, rectangle | |
| Tout choisir | `A` | |
| Ne rien choisir | `Alt+A` | |
| Supprimer | `Suppr` ou `X` | |
| Dupliquer | `Maj+D` ou `Ctrl+D` | |
| Copier, couper, coller | `Ctrl+C`, `Ctrl+X`, `Ctrl+V` | Passe par le presse-papiers du système : on colle aussi dans un autre graphe |
| Annuler, rétablir | `Ctrl+Z`, `Ctrl+Maj+Z` ou `Ctrl+Y` | |
| Aligner | `Q` | Range les nœuds choisis |
| Renommer | `F2` | Donne un nom au nœud |
| Muet | `M` | Le nœud laisse passer sa première entrée, comme s'il n'était pas là |
| Réduire | `H` | Ne montre que l'en-tête |
| Miniature | `P` | Affiche ou cache la miniature du nœud |
| Cadre | `Ctrl+J` | Un cadre autour de la sélection |
| Grouper | `Ctrl+G` | Voir plus bas |
| Dégrouper | `Ctrl+Alt+G` | |
| Entrer, sortir d'un groupe | `Tab` | |

Clic droit sur un nœud : `Duplicate`, `Preview node` (l'aperçu montre ce nœud seul), `Back to full material`, `Group the selection`, `Ungroup`.

`View` : `Hide thumbnails` ou `Show thumbnails` pour toutes les miniatures.

## Cadres et notes

Un **cadre** (`Ctrl+J`) entoure des nœuds, avec un titre et une couleur. Glisser le titre déplace le cadre et ce qu'il contient.

Un cadre peut porter du texte libre : il sert alors de note posée sur le graphe (« Write your note here… »).

## Renvoi

Le nœud `Reroute` est un simple point de passage. Il sert à ranger les fils. Il ne calcule rien.

## Groupes

Un groupe est un morceau de graphe rangé dans un seul nœud, réutilisable autant de fois qu'on veut.

### Créer et ouvrir

1. Choisis des nœuds.
2. `Ctrl+G`. Message : « Group created. Tab to enter it, Ctrl+Alt+G to ungroup. »
3. `Tab` sur le groupe pour y entrer. `Tab` de nouveau, ou `Leave the group (Tab)`, pour sortir.

Les liens qui entraient et sortaient de la sélection deviennent les prises du groupe.

### Régler ses prises

Dans le groupe, deux nœuds spéciaux : `Group inputs` et `Group outputs`.

- `Add a socket` ajoute une prise. Ou relie un nœud à l'anneau vide « new socket ».
- Chaque prise a un nom, un type (`Decimal number`, `Integer`, `True / false`, `Vector`, `Color`, `Geometry`) et une valeur par défaut.

### Réutiliser

`Add › Structure` liste les groupes du graphe. Poser un groupe une seconde fois en crée un **exemplaire** : modifier le groupe change tous ses exemplaires. Le panneau l'indique : « Used N times: editing it changes every copy. »

Les entrées d'un exemplaire se règlent sur le nœud, sans l'ouvrir.

### Limites

- Un groupe vit dans son graphe. Copier-coller un nœud de groupe vers un autre graphe emporte le groupe avec lui.
- Deux groupes ne peuvent pas porter le même nom.
- Une zone se groupe en entier : il faut choisir ses deux bouts.

## Zones

Une zone est délimitée par deux nœuds qui vont par deux : un début et une fin. Ce qui est entre les deux s'exécute plusieurs fois.

### Zone de répétition

`Add › Structure › Repeat zone`. Le contenu s'exécute autant de fois qu'il y a d'itérations.

- Le nœud de début porte le réglage `Iterations`.
- **Repeat state** : ce qui passe d'un tour au suivant. `Add a socket` en ajoute.
- Exemple : flouter une image cinq fois de suite, ou subdiviser un motif à chaque tour.

Le nombre d'itérations doit être un nombre fixe, réglé sur le nœud, sans lien. Une répétition qui donnerait un graphe trop gros est refusée.

### Zone de simulation

`Add › Simulation › Simulation zone`. Dans un graphe de géométrie seulement. Le contenu s'exécute **une fois par image**, et l'état passe d'une image à la suivante. Voir [Géométrie et simulation](noeuds-geometrie.md#zone-de-simulation).

### Règles

- Des zones de répétition peuvent s'emboîter. Elles ne peuvent pas se croiser.
- Une zone de simulation ne peut pas se trouver dans une répétition. L'inverse est permis : mets la répétition dans la simulation.

## Champs

Dans un graphe de géométrie, une entrée de nombre, de vecteur ou de couleur est un **champ** : au lieu d'une seule valeur, elle reçoit une valeur **par point**.

Le signe : la prise porte l'indication « field: one value per point ».

**Exemple.** Le nœud `Set point positions` a une entrée `Offset`. Branche-y un nœud `Noise` : chaque point est décalé d'une valeur différente, lue dans le bruit à sa position. Voir [Géométrie et simulation](noeuds-geometrie.md#champs).

## Expressions

Deux nœuds acceptent une formule écrite.

### Nœud Expression

Catégorie `Math`. Une ligne de formule, quatre entrées `a`, `b`, `c`, `d`, une sortie.

Variables : `a`, `b`, `c`, `d`, la position `x`, `y`, `z`, les coordonnées `u`, `v`, et `pi`.

Exemples :

```
a * b
sin(x * 10) * 0.5 + 0.5
if(y > 0.5, a, b)
clamp(a + noise(x, y, z), 0, 1)
```

### Nœud Custom shader

Catégorie `Shader`. Plusieurs lignes, exécutées pour chaque pixel. Entrées : deux couleurs `A` et `B`, un facteur `F`. Sorties : une couleur, et `F`.

Variables :

| Variable | Sens |
|---|---|
| `r`, `g`, `b`, `a` | La couleur de sortie. C'est elles qu'on écrit |
| `ar`, `ag`, `ab`, `aa` | L'entrée A |
| `br`, `bg`, `bb`, `ba` | L'entrée B |
| `f` | L'entrée F |
| `x`, `y` | Coordonnées du pixel (0 à 1) |
| `nx`, `ny`, `nz` | Normale de la surface (matériaux) |
| `px`, `py`, `pz` | Position sur le modèle (matériaux) |
| `w`, `h` | Taille de l'image |
| `pi` | 3,14159… |

Tu peux créer tes propres variables : `d = x - 0.5`. Plusieurs instructions sur une ligne se séparent par `;`.

Exemple (mélange par un dégradé radial) :

```
d = length(x - 0.5, y - 0.5) * 2
r = lerp(ar, br, d)
g = lerp(ag, bg, d)
b = lerp(ab, bb, d)
```

Le bouton `AI` écrit le shader d'après une description (« Describe the effect… »). Il demande un modèle branché. Voir [Assistant IA](assistant-ia.md).

### Fonctions communes

| Fonction | Sens |
|---|---|
| `sin`, `cos`, `tan`, `atan2(y, x)` | Trigonométrie |
| `abs`, `sign`, `floor`, `ceil`, `fract`, `sqrt`, `exp`, `log` | Une valeur |
| `pow(a, b)`, `min`, `max`, `mod` | Deux valeurs |
| `clamp(x, a, b)` | Borne `x` entre `a` et `b` |
| `lerp(a, b, t)` ou `mix(a, b, t)` | Mélange de `a` vers `b` |
| `step(seuil, x)` | 0 sous le seuil, 1 au-dessus |
| `smoothstep(a, b, x)` | Passage doux de 0 à 1 entre `a` et `b` |
| `length(x, y)` ou `len(x, y)` | Longueur |
| `noise(x, y, z)` | Bruit lisse |
| `fbm(x, y, z)` | Bruit fractal (plusieurs couches) |
| `rand(x, y)` | Hasard, stable pour un même point |
| `if(condition, si vrai, si faux)` | Choix |

Opérateurs : `+`, `-`, `*`, `/`, parenthèses, et les comparaisons `>`, `<` dans une condition.

`mix` est un autre nom de `lerp`, et `len` un autre nom de `length`. Si une formule est refusée, le message d'erreur donne la ligne fautive.

## Le temps

Le nœud `Time` donne l'image courante (`Frame`) et le temps en secondes (`Seconds`). Le champ `Frame` de la barre de l'éditeur choisit l'image montrée par l'aperçu.

`Espace` (souris sur l'aperçu) lance la lecture dans un graphe de géométrie.

## Demander à l'assistant

L'assistant (`Ctrl+L`) connaît les nœuds et modifie le graphe ouvert : tu décris ce que tu veux, il propose les changements sur une carte, tu appliques d'un clic. Le tout s'annule d'un `Ctrl+Z`. Voir [Modifier un graphe de nœuds](assistant-ia.md#modifier-un-graphe-de-nœuds).

## Erreurs

Un nœud en erreur est marqué. Le message s'affiche en clair. Exemples : « The output node is missing », « This node reads a geometry attribute: it belongs in a geometry graph ».

## Enregistrement

Un graphe est un fichier JSON lisible. Voir [Formats](../technique/formats.md#graphes-de-nœuds-arymat-arycomp-arygeo).
