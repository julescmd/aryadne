# Atelier : matériaux à calques

L'Atelier est l'écran où s'ouvre **tout matériau** (`.arymat`). On y empile des calques, comme dans Substance Painter. Les nœuds viennent ensuite : on y entre calque par calque, comme dans Substance Designer.

## L'idée

Un matériau est une **pile de calques**.

- Chaque **calque** a son **graphe de nœuds**. Ce graphe donne la couleur, la rugosité, le métal et le reste.
- Un calque **« graphe »** porte son propre graphe, enregistré dans le matériau.
- Un calque **lié** est une *instance* d'un **matériau maître** : un autre `.arymat` du coffre. Corriger le maître met à jour tous les calques qui s'en servent. Un petit maillon marque ces calques dans la pile.
- Le graphe peut **exposer des paramètres** : une couleur, une échelle, une usure. Le calque les règle par des curseurs, sans ouvrir les nœuds.
- Un **masque** dit où le calque se pose : partout, sur les arêtes usées, dans les creux, sur le dessus. Le masque peut lui aussi devenir un graphe de nœuds.

Sous le capot, la pile entière est ramenée à un seul graphe de nœuds. C'est lui que voient la vue 3D et le rendu. Voir [Système de nœuds](../technique/noeuds.md#latelier--pile--graphe).

## Créer un matériau

`File › New › Material`, la fenêtre `New file` (genre `Material`, avec un préréglage de départ), ou la palette : « New material ».

Le matériau s'ouvre dans l'Atelier, avec un premier calque. Il s'applique à un objet depuis l'onglet `Material` de ses propriétés. Voir [Matériaux en nœuds](noeuds-materiaux.md#créer-et-appliquer).

### Les matériaux en nœuds d'avant

Un `.arymat` fait avant l'Atelier ne contient qu'un graphe. Il s'ouvre comme une pile d'**un seul calque**, nommé `Graph`, qui porte tout ce graphe : nœuds, groupes, cadres et notes. Rien n'est perdu.

Tant que la pile reste ce seul calque (posé partout, opaque, sans réglage), le fichier est réécrit sous sa forme d'avant. Dès qu'on ajoute un calque, un masque ou une opacité, il passe à la forme « pile ».

## Entrer dans les nœuds d'un calque

| Geste | Effet |
|---|---|
| Double-clic sur un calque | Ouvre son graphe en nœuds |
| L'icône de nœuds, à droite du calque ou à côté de son nom | Pareil |
| Double-clic sur la vignette du masque (masque `Node graph`) | Ouvre le graphe du masque |

On passe alors dans [l'éditeur de nœuds](noeuds-editeur.md). La barre du graphe montre un **fil d'Ariane** : `← Layers › nom du calque`, puis le groupe si l'on est entré dans un groupe.

Pour revenir à la pile : le bouton `← Layers`, `Échap`, ou `Tab` à la racine du graphe.

- Dans les nœuds, l'aperçu montre **ce calque seul**. De retour sur la pile, il montre le matériau entier.
- L'annulation est propre à chaque niveau. Dans les nœuds, `Ctrl+Z` défait les gestes du graphe. Sur la pile, un seul `Ctrl+Z` défait tout ce qui a été changé dans le calque.
- Un calque **lié** ouvre le graphe de son maître. Ce qu'on y change est écrit dans le fichier du maître, donc partagé.

## L'écran

| Zone | Contenu |
|---|---|
| À gauche | **Materials** : la bibliothèque |
| Au centre | L'aperçu du matériau sur une forme |
| À droite, en haut | La pile de calques |
| À droite, en bas | **Properties** du calque choisi |

### La bibliothèque

Filtres : `All`, `Vault master materials`, `Smart materials`, `Presets`.

| Genre | Au clic |
|---|---|
| Matériau maître du coffre | Ajouté comme calque lié à ce maître |
| Matériau intelligent | Ajoute tous ses calques, avec leurs masques |
| Préréglage | Devient un calque « graphe », avec son propre graphe. Rien n'est écrit ailleurs dans le coffre |

Tout `.arymat` du coffre qui tient en un seul graphe peut servir de maître.

Préréglages fournis : bois, pierre, métal rouillé, toon, céramique, or, métal peint usé, pierre aux creux sombres, carrelage en relief, béton fissuré, métal brossé rayé, velours, cire, tissu tissé, verre, verre dépoli, eau, toon 3 bandes, peinture de voiture, plastique, néon.

### L'aperçu

- `Sphere` : aperçu sur une sphère.
- `Cube` : un cube aux arêtes adoucies. On y voit l'usure des arêtes et la saleté des creux.
- Ou dépose un modèle 3D sur l'aperçu.
- `Light ▾` : `Studio` ou `Outdoor`.

## La pile de calques

Le calque du dessus est en haut de la liste. Chaque ligne montre deux vignettes : le matériau, puis son masque.

| Geste | Effet |
|---|---|
| Clic | Choisir le calque |
| Double-clic | Ouvrir le graphe du calque en nœuds |
| Œil | Montrer ou cacher |
| `+` de l'en-tête | Nouveau calque de remplissage |
| Autres icônes de l'en-tête | Monter, descendre, dupliquer, supprimer |
| Étincelles | Enregistrer la pile comme matériau intelligent |
| `Ctrl+Z`, `Ctrl+Maj+Z` | Annuler, rétablir |

Un **calque de remplissage** expose trois paramètres : `Color`, `Roughness` et `Metal`. Ils se règlent dans les propriétés, comme un calque de remplissage de Painter. Son graphe s'ouvre en nœuds pour aller plus loin.

## Propriétés d'un calque

### Le calque

| Réglage | Effet |
|---|---|
| `Name` | Le nom du calque |
| L'icône de nœuds | Ouvre le graphe du calque |
| `Opacity` | De 0 à 100 % |
| `Master material` (calque lié) | Le maître d'où vient le graphe. Le maillon brisé **détache** le calque : il garde une copie du graphe, à lui seul |

### Les paramètres

Chaque paramètre exposé par le graphe du calque apparaît ici : une couleur ou un curseur. Une valeur changée porte une flèche **Back to the graph's value**.

Si le graphe n'expose rien, un message l'explique. Pour exposer un paramètre : dans les nœuds du calque, ajoute un nœud `Exposed parameter` et renomme-le (`F2`). Voir [Matériaux en nœuds](noeuds-materiaux.md#paramètre-exposé).

### Le masque

Les masques sont proposés en pastilles.

| Masque | Où le calque apparaît |
|---|---|
| `Everywhere` | Partout |
| `Edge wear` | Sur les arêtes saillantes |
| `Cavity dirt` | Dans les creux |
| `Dust on top` | Sur les faces tournées vers le haut |
| `Drips` | En coulures verticales |
| `Scratches` | En rayures fines |
| `Grunge` | En taches de saleté |
| `Noise` | Selon un bruit |
| `Cracks` | Le long de fissures |
| `Spots` | En taches (cellules) |
| `Hand-painted` | Là où tu as peint, dans la scène |
| `Node graph` | Là où le dit le graphe du masque |

Réglages communs :

| Réglage | Effet |
|---|---|
| `Amount` | Combien de surface le masque couvre |
| `Contrast` | Bord doux ou franc |
| `Scale` | Taille du motif (0,5 à 40) |
| `Invert` | Inverse le masque |
| `Breakup` | Un second motif qui découpe le premier, pour un résultat moins régulier. Avec sa quantité et son échelle |

### Masques intelligents

Le bouton `Smart masks` propose des réglages tout prêts :

- peinture écaillée sur les arêtes ;
- rouille dans les creux ;
- poussière et saleté ;
- bords légèrement usés ;
- coulures de pluie ;
- métal rayé ;
- sol craquelé.

### Masque en nœuds

La pastille `Node graph` transforme le masque en graphe de nœuds et l'ouvre.

- Le graphe part du masque réglé jusque-là : son générateur, sa cassure et son inversion y sont déjà.
- La **couleur** de la sortie fait le masque. Blanc : le calque se voit. Noir : il ne se voit pas.
- L'aperçu montre le masque en gris, sur le cube.
- `Open the mask in nodes` y retourne. `Invert` reste disponible sur la pile.

Choisir une autre pastille revient à un masque simple.

### Masque peint à la main

1. Choisis `Hand-painted`, puis un canal de 1 à 4.
2. Dans la scène, en mode `Paint`, ajoute à l'objet un calque de peinture du même canal (`Mask 1 (Atelier)`…).
3. Peins où le calque doit apparaître.

Voir [Peinture 3D](3d-peinture.md#les-canaux).

## Matériaux intelligents

Un matériau intelligent est une pile enregistrée, à reposer sur un autre objet.

- Enregistrer : la disquette de l'en-tête des calques. Le fichier `.arysmart` va dans le dossier `matériaux intelligents/` du coffre.
- Réutiliser : il apparaît dans la bibliothèque, filtre `Smart materials`.

## Exemple : un métal peint, usé et sale

1. Nouveau matériau.
2. Clique le préréglage « métal brossé rayé », puis supprime le calque de départ : le métal est le calque du bas, masque `Everywhere`.
3. `+` : un calque de remplissage, la peinture. Change sa couleur. Masque `Edge wear`, coche `Invert` : la peinture couvre tout sauf les arêtes, où le métal réapparaît.
4. `+` encore : un calque sombre pour la saleté. Masque `Cavity dirt`, `Amount` bas.
5. Double-clic sur la peinture pour entrer dans ses nœuds, ajoute un bruit sur la rugosité, puis `← Layers`.
6. Enregistre en matériau intelligent.

## Limites

- Les masques d'usure reposent sur la courbure et l'orientation du modèle. Sur un modèle très peu détaillé, `Edge wear` et `Cavity dirt` ne trouvent presque rien.
- La pile gère couleur, rugosité, métal, émission et les autres entrées de la sortie matériau. Il n'y a pas de mode de fusion par canal ni de dossier de calques.
- Un calque couvre tous les canaux à la fois. On ne peut pas, par exemple, ne poser que la rugosité d'un calque.
- Le masque en nœuds lit la couleur de la sortie de son graphe : les autres entrées de cette sortie ne servent pas.
- Le masque peint est limité à quatre canaux par objet.
- **Export des textures.** L'aide de l'application parle d'« export of textures ». Il se fait par la cuisson de l'onglet UV (voir [UV et UDIM](3d-uv.md#cuisson-des-textures-bake)). Un export direct depuis l'Atelier n'a pas été trouvé dans le code actuel.
