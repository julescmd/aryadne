---
titre: Obstacles et auto-collision
niveau: intermédiaire
duree: 15 min
prerequis: 14-simulation/04-regler-un-tissu
objectifs: Placer le sol, une boule et une boîte ; Régler le frottement ; Activer l'auto-collision ; Choisir une épaisseur sûre
---

## Ce que tu vas réussir

Tu vas déplacer les obstacles d'un tissu. Tu vas le faire glisser, puis accrocher. Puis tu vas empêcher un tissu de se traverser lui-même.

```essayer
exemple: tissu sur la carte graphique
```

## Trois obstacles, tous dans le Solver

Le node **Solver** connaît trois obstacles.

| Obstacle | Entrées | Pour l'enlever |
|---|---|---|
| Le sol | Réglage **Floor**, entrée **Floor height** | **Floor** sur **not** |
| Une boule | **Ball center**, **Ball radius** | Rayon à 0 |
| Une boîte | **Box center**, **Box size** | Taille à 0 |

```attention
Dans l'interface en anglais, le choix « non » s'affiche **not**.
```

Les obstacles sont résolus en même temps que les fils. Ils n'étirent donc pas le tissu après coup.

```etape
Lis la simulation jusqu'à l'image 60. La nappe épouse une boule et une boîte.
verifier: lecture.image >= 60
```

Tu vois la boule, pas la boîte. Les points bleus de la boule viennent des nodes du bas du graphe. Ils sont là pour l'œil. L'obstacle réel, c'est le **Solver** qui le porte.

## Déplace la boule

```etape
Sur **Solver**, mets **Ball center** à (0, 0,7, 0).
```

```etape
En bas du graphe, trouve le node **Move, rotate** de la boule montrée. Mets sa **Translation** à (0, 0,7, 0) aussi.
```

```etape
Reviens à la première image et lis. Le tissu se pose sur la boule à son nouvel endroit.
verifier: lecture.image >= 40
```

```astuce
Pour ne régler la position qu'une fois, pose un node **Combine XYZ** et relie-le aux deux entrées : **Ball center** et **Translation**.
```

## Enlève la boîte

```etape
Sur **Solver**, mets les trois valeurs de **Box size** à 0. Relis. Le tissu retombe droit là où la boîte le retenait.
```

## Frottement

**Friction** règle l'accroche au contact des obstacles.

| Valeur | Effet |
|---|---|
| 0 | Le tissu glisse |
| 0,4 | Coton sur du bois |
| 1 | Il accroche fort |

À 0,4, le tissu tient jusqu'à une pente d'environ 20 degrés. Sur une pente plus raide, il glisse.

```etape
Mets **Friction** à 0. Relis depuis la première image. Le tissu glisse de la boule et finit au sol.
```

```etape
Mets **Friction** à 1. Relis. Le tissu accroche à la boule et ne glisse presque plus.
```

Reviens à 0,4.

## Le tissu traverse la boule ?

Un tissu qui tombe vite peut franchir un obstacle en un seul pas. Monte **Substeps**. Chaque sous-pas est plus court : le tissu n'a plus le temps de passer à travers.

## L'auto-collision

Sans elle, un tissu qui se replie passe à travers lui-même.

```essayer
exemple: tissu qui se replie
```

```etape
Lis jusqu'à l'image 60. Le tissu, debout, s'affaisse et se replie sur le sol. Ses couches restent séparées.
verifier: lecture.image >= 60
```

```etape
Ouvre le menu du tissu, dans le coin haut droit de l'aperçu. Clique la puce **Self-collision** pour la désactiver.
```

```etape
Relis depuis la première image. Les couches passent maintenant les unes à travers les autres.
```

```etape
Réactive **Self-collision**. Un curseur **Thickness** apparaît dessous.
```

## L'épaisseur

Le tissu est fait de points. Ce sont les points qui se repoussent. **Thickness** est la distance qu'ils gardent entre eux, en mètres.

- À 0, l'épaisseur est choisie d'après la grille : 0,9 fois la taille d'une maille.
- Ne descends pas sous 0,7 maille. Une couche passerait entre les points d'une autre.
- Un tissu plus fin, avec plus de points, donne des couches plus rapprochées.
- Deux points voisins dans le tissu ne se repoussent jamais. Le tissu se froisse librement.

```etape
Mets **Thickness** à 0,1. Relis. Les couches gardent un écart bien visible.
```

Remets 0 ensuite.

```attention
L'auto-collision coûte du temps de calcul. Si le tissu se traverse quand même, monte **Substeps** : un pli qui se referme vite peut franchir l'épaisseur en un seul sous-pas.
```

## Ce qui n'existe pas encore

- Les obstacles sont un sol, une boule et une boîte. Pas de collision avec la forme exacte d'un modèle.
- Dans une scène 3D, le node **Scene object** donne au Solver la boule ou la boîte qui entoure un objet de la scène. L'objet peut bouger : la simulation le suit. Voir [Géométrie et simulation en nœuds](../../utilisateur/noeuds-geometrie.md#dans-une-scène-3d).
- L'auto-collision se fait entre points, pas entre faces. Il n'y a pas de frottement entre les couches.
- Elle n'existe que dans le solveur **Cloth (graphics card)**.

```quiz
Comment retirer la boule du Solver ?
- [ ] Supprimer les nodes bleus du bas du graphe.
- [x] Mettre Ball radius à 0.
- [ ] Mettre Friction à 0.
explication: Les nodes du bas ne font que montrer la boule. L'obstacle est dans le Solver, et un rayon de 0 l'enlève.
```

```quiz
Thickness vaut 0. Quelle épaisseur le solveur prend-il ?
- [ ] Aucune : l'auto-collision est coupée.
- [x] 0,9 fois la taille d'une maille.
- [ ] Un centimètre.
explication: À 0, l'épaisseur est choisie d'après la maille. C'est le réglage le plus sûr.
```

```defi
Dans l'exemple du tissu qui se replie, fais tomber le tissu sur une boule au lieu du sol nu.
indice: Donne un Ball radius et un Ball center au Solver. Pour la voir, ajoute Points on a sphere et Move, rotate, puis relie-les à l'entrée libre du node Merge.
```

## En trois lignes

- Sol, boule et boîte se règlent dans le **Solver**. Les points montrés ne sont qu'un repère.
- **Friction** : 0 glisse, 0,4 coton sur du bois, 1 accroche.
- **Self-collision** garde les couches à distance. **Thickness** à 0 est le choix sûr.

[Suite : Vent, lecture et cache](06-vent-lecture-et-cache.md)
