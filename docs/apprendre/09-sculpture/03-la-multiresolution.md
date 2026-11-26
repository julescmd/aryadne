---
titre: La multirésolution
niveau: avancé
duree: 15 min
prerequis: 09-sculpture/02-les-sept-pinceaux
objectifs: Ajouter des niveaux de détail ; Monter et descendre dans les niveaux ; Corriger une grande forme sans perdre le détail ; Connaître la limite de faces
---

## Ce que tu vas réussir

Tu vas sculpter des détails fins sur un cube, puis changer sa forme générale. Les détails suivront. C'est le tour de magie de la multirésolution.

```essayer
nouveau: aryscene
nom: multiresolution
```

## L'idée

La multirésolution garde plusieurs niveaux de détail du même maillage.

- En bas, peu de faces : tu règles les grandes formes.
- En haut, beaucoup de faces : tu sculptes les détails.

Tu montes et tu descends sans rien perdre.

## Pars du cube

Cette fois, pas de **Remesh**. Les niveaux vont fournir les sommets.

```etape
Choisis le cube. Passe en mode **Sculpt**.
verifier: scene.selection
```

Dans la barre des pinceaux, repère le compteur de niveaux. Il affiche le niveau courant et le niveau le plus haut : 0/0 pour l'instant. Le niveau 0 est la forme de base.

## Ajoute des niveaux

```etape
À droite du compteur, clique le **+**. Tout le maillage est subdivisé. Le compteur passe à 1/1.
```

```etape
Clique **+** encore trois fois. Tu es au niveau 4/4. Le cube s'est arrondi et porte assez de faces pour sculpter.
```

Chaque niveau subdivise tout le maillage, à la façon Catmull-Clark. Le nombre de faces est multiplié par quatre.

```attention
Aryadne refuse d'ajouter un niveau au-delà de 1,5 million de faces. La vue et la mémoire ne suivraient plus.
```

## Sculpte du détail en haut

```etape
Au niveau 4, prends **Draw** avec un petit pinceau. Sculpte des rides, des écailles ou des veines sur une face.
```

```etape
Prends **Pinch** et resserre quelques-uns de ces traits.
```

## Descends

```etape
Clique la flèche vers le bas, à gauche du compteur, jusqu'au niveau 1.
```

Le maillage est grossier. Tes détails ne sont plus affichés. Ils ne sont pas perdus : le détail fin est gardé.

À droite du compteur, le **+** est devenu une flèche vers le haut. Le **+** ne revient qu'au niveau le plus haut.

## Change la grande forme

```etape
Au niveau 1, prends **Grab** avec un gros pinceau. Tire un coin loin vers l'extérieur. Écrase un côté.
```

Peu de sommets bougent, donc la forme change vite et proprement. Faire la même chose au niveau 4 donnerait une bosse molle.

## Remonte

```etape
Clique la flèche vers le haut jusqu'au niveau 4.
```

Tes rides et tes écailles sont là, posées sur la nouvelle forme.

## La règle d'or

| Tu veux changer… | Va au niveau… |
|---|---|
| Les proportions, la silhouette | 0 ou 1 |
| Les volumes moyens | 2 ou 3 |
| Les rides, les pores, les éraflures | Le plus haut |

N'ajoute un niveau que lorsque le niveau courant ne peut plus porter ce que tu veux sculpter. Monter trop tôt, c'est sculpter des détails sur une forme pas encore juste.

```astuce
Un exemple classique : un visage au niveau 3, le nez trop court. Descends au niveau 0, tire le nez avec **Grab**, remonte. Les pores sont toujours là, sur le nouveau nez.
```

## Remesh ou multirésolution ?

| | **Remesh (voxels)** | Multirésolution |
|---|---|---|
| Point de départ | N'importe quelle forme | Un maillage propre, en quads de préférence |
| Topologie | Égale partout | Celle de la base, subdivisée |
| Revenir aux grandes formes | Non | Oui |

Un bon chemin : ébaucher, retopologiser pour avoir une base propre, puis détailler en multirésolution.

```etape
Sors avec **Leave sculpting** et enregistre.
verifier: fichier.enregistre
```

```quiz
Tu descends du niveau 4 au niveau 1. Que deviennent tes détails ?
- [ ] Ils sont supprimés.
- [x] Ils sont gardés, et reviennent quand tu remontes.
- [ ] Ils sont aplatis.
explication: La multirésolution garde le détail de chaque niveau. Descendre ne fait que l'enlever de l'affichage.
```

```quiz
Où corriger la silhouette d'un modèle déjà détaillé ?
- [x] Au niveau le plus bas.
- [ ] Au niveau le plus haut.
- [ ] Dans un nouveau fichier.
explication: En bas, peu de sommets portent la forme. Les détails des niveaux hauts suivent.
```

```defi
Sculpte un rocher : forme générale au niveau 1, grandes cassures au niveau 3, grain au niveau 5.
indice: Flatten pour les plans de cassure, Draw inversé et petit pour les fissures. Redescends corriger la masse si elle ne te plaît plus.
```

## En trois lignes

- **+** ajoute un niveau. Les flèches montent et descendent.
- Grandes formes en bas, détails en haut. Rien ne se perd.
- La limite est de 1,5 million de faces.

[Suite : Le remaillage dynamique et la suite](04-remaillage-dynamique.md)
