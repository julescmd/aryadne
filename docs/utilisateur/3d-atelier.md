# Atelier : matériaux à calques

> **En cours de refonte.** Un autre chantier change le flux des matériaux au moment où cette page est écrite. L'Atelier doit devenir l'entrée unique des matériaux, et chaque calque s'ouvrira en nœuds. Cette page décrit le code actuel. Les libellés et l'emplacement des boutons peuvent changer.

## L'idée

Un matériau à calques est une **pile**, comme dans Substance Painter.

- Chaque **calque** est une *instance* d'un **matériau maître**.
- Le maître est un graphe de nœuds (un fichier `.arymat`) qui expose quelques paramètres : une couleur, une échelle, une usure.
- Le calque règle ces paramètres par des curseurs, sans ouvrir les nœuds.
- Un **masque** dit où le calque se pose : partout, sur les arêtes usées, dans les creux, sur le dessus.

Corriger un maître met à jour tous les calques qui s'en servent.

Sous le capot, la pile entière est ramenée à un seul graphe de nœuds. C'est lui que voient la vue 3D, le rendu et l'éditeur de nœuds. Voir [Système de nœuds](../technique/noeuds.md#latelier--pile--graphe).

## Créer un matériau à calques

`File › New › Layered material (Atelier)`, ou la palette : « New layered material (Atelier) ».

Le fichier est un `.arymat` qui contient une pile. Il s'applique à un objet comme n'importe quel matériau en nœuds (onglet `Material` des propriétés de l'objet).

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
| Matériau maître du coffre | Ajouté comme calque |
| Matériau intelligent | Ajoute tous ses calques, avec leurs masques |
| Préréglage | Devient un matériau maître du coffre, puis un calque |

`New master material` crée un `.arymat` à côté, l'ajoute comme calque et l'ouvre en nœuds.

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
| Double-clic | Ouvrir le matériau maître en nœuds |
| Œil | Montrer ou cacher |
| Icônes de l'en-tête | Dupliquer, monter, descendre, supprimer |
| Disquette | Enregistrer la pile comme matériau intelligent |

## Propriétés d'un calque

### Le calque

| Réglage | Effet |
|---|---|
| `Name` | Le nom du calque |
| `Open the master's nodes` | Ouvre le graphe du maître |
| `Opacity` | De 0 à 100 % |

### Les paramètres du maître

Chaque paramètre exposé par le maître apparaît ici : une couleur ou un curseur. Une valeur changée porte une flèche **Back to the master's value**.

Si le maître n'expose rien, un message l'explique. Pour exposer un paramètre : dans les nœuds du maître, ajoute un nœud `Exposed parameter` et renomme-le (`F2`). Voir [Matériaux en nœuds](noeuds-materiaux.md#paramètre-exposé).

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

1. Nouveau matériau à calques.
2. Clique le préréglage « métal brossé rayé » : c'est le calque du bas, masque `Everywhere`.
3. Clique « plastique » : le calque de peinture. Change sa couleur. Masque `Edge wear`, coche `Invert` : la peinture couvre tout sauf les arêtes, où le métal réapparaît.
4. Clique « pierre aux creux sombres » ou crée un maître sombre. Masque `Cavity dirt`, `Amount` bas.
5. Passe l'aperçu sur `Cube` pour juger les arêtes et les creux.
6. Enregistre en matériau intelligent.

## Limites

- Les masques d'usure reposent sur la courbure et l'orientation du modèle. Sur un modèle très peu détaillé, `Edge wear` et `Cavity dirt` ne trouvent presque rien.
- La pile gère couleur, rugosité, métal, émission et les autres entrées de la sortie matériau. Il n'y a pas de mode de fusion par canal ni de dossier de calques.
- Le masque peint est limité à quatre canaux par objet.
- **Export des textures.** L'aide de l'application parle d'« export of textures ». Il se fait par la cuisson de l'onglet UV (voir [UV et UDIM](3d-uv.md#cuisson-des-textures-bake)). Un export direct depuis l'Atelier n'a pas été trouvé dans le code actuel.
