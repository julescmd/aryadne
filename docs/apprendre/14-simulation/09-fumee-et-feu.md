---
titre: Fumée et feu
niveau: intermédiaire
duree: 25 min
prerequis: 14-simulation/02-premiere-simulation, 14-simulation/06-vent-lecture-et-cache
objectifs: Comprendre ce qu'est un volume ; Monter une source et un solveur de fumée ; Régler flottabilité, tourbillons et dissipation ; Allumer un feu avec la combustion ; Poser un obstacle ; Choisir résolution et itérations de pression
---

## Ce que tu vas réussir

Tu vas faire monter une fumée, la régler, puis allumer un feu de camp et faire contourner une boule à la fumée. Tout se calcule sur ta carte graphique.

## Partie 1 : une fumée qui monte

```essayer
exemple: fumée qui monte
```

```etape
Lance la lecture et laisse-la aller jusqu'à l'image 40. Une fumée sort d'une petite sphère et monte en volutes.
verifier: lecture.image >= 40
```

### Un volume, pas des points

Jusqu'ici, une géométrie était un tableau de points. La fumée est autre chose : une **boîte découpée en petits cubes**, les voxels. Chaque voxel garde quelques nombres.

| Champ | Ce qu'il dit |
|---|---|
| densité | Combien de fumée il y a ici. C'est ce qui cache la lumière |
| température | La chaleur. L'air chaud monte |
| carburant | Ce qui reste à brûler |
| flamme | L'intensité du feu. C'est ce qui brille |
| vitesse | Le mouvement de l'air |

En haut de l'aperçu, tu lis la taille de la grille : `48 × 72 × 48 voxels`.

### Les trois nodes

```etape
Clique le node **Empty volume** (volume vide), en haut. Il crée la boîte : **Center**, **Scale** (ses trois tailles) et **Resolution**.
verifier: graphe.contient = vol_box
```

**Resolution** est le nombre de voxels le long du plus grand côté. Deux fois plus de résolution, c'est huit fois plus de voxels, donc huit fois plus de calcul.

Dans la zone de simulation, deux nodes se suivent.

| Node | Ce qu'il fait |
|---|---|
| **Smoke source** | À chaque image, pose de la fumée chaude dans une petite zone |
| **Smoke solver** | Fait bouger l'air et emporte la fumée |

```etape
Sur **Smoke source**, mets **Radius** à 0,25. Reviens à la première image et relis. La colonne de fumée est plus large.
verifier: graphe.contient = vol_source
```

Remets 0,14.

```etape
Mets **Irregularity** à 0. La source est parfaite, la fumée monte toute droite, comme un champignon. Remets 0,4 : un peu de hasard donne une fumée vivante.
```

### Régler le mouvement

Tout se passe sur **Smoke solver**.

```etape
Mets **Buoyancy** (flottabilité) à 8. Relis. La fumée monte deux fois plus vite.
verifier: graphe.contient = sim_smoke
```

```etape
Mets **Swirls** (tourbillons) à 0. Les volutes s'effacent : la fumée devient lisse. Mets 5 : elles se creusent.
```

```etape
Mets **Dissipation** à 1. La fumée disparaît avant d'arriver en haut.
```

Remets 4, 2 et 0,1.

| Réglage | Sens |
|---|---|
| **Buoyancy** | Poussée vers le haut de l'air chaud |
| **Smoke weight** | Poids de la fumée : une fumée lourde et froide retombe |
| **Swirls** | Rend les petits tourbillons que le calcul efface |
| **Dissipation** | Part de la fumée qui disparaît chaque seconde |
| **Cooling** | Chaleur perdue chaque seconde. Une fumée refroidie cesse de monter |
| **Wind** | Le vent emporte la fumée de côté |
| **Air drag** | Freine tout le mouvement |

```etape
Mets la composante X de **Wind** à 1. Relis. La fumée penche.
```

Remets 0.

## Partie 2 : un feu de camp

```essayer
exemple: feu de camp
```

```etape
Lis jusqu'à l'image 60. Des flammes montent, puis une fumée fine.
verifier: lecture.image >= 60
```

Ici, la source ne pose pas de fumée. Elle pose du **carburant chaud** : **Fuel** à 1, **Temperature** à 1, **Density** à 0. Un node de plus le fait brûler.

```etape
Clique le node **Combustion**, entre la source et le solveur.
verifier: graphe.contient = vol_burn
```

Le carburant plus chaud que **Ignition** (allumage) brûle. En brûlant, il chauffe, il fait de la fumée, et il gonfle l'air autour de lui.

| Réglage | Sens |
|---|---|
| **Ignition** | Température à partir de laquelle le carburant brûle |
| **Burn rate** | Vitesse de combustion. Lente : de hautes flammes. Rapide : un feu court et vif |
| **Heat** | Chaleur dégagée. Elle fait monter les flammes et change leur couleur |
| **Smoke** | Fumée produite |
| **Expansion** | Gonflement des gaz qui brûlent |
| **Cooling** | Refroidissement des flammes. Les plus chaudes refroidissent le plus vite |

```etape
Mets **Burn rate** à 1. Relis. Le carburant met plus de temps à brûler : les flammes montent plus haut.
```

```etape
Mets **Smoke** à 1. Le feu fume beaucoup.
```

La couleur des flammes vient de la température : rouge, puis orange, jaune, et blanc au plus chaud.

```astuce
Pour changer l'aspect sans toucher au calcul, ajoute un node **Smoke look** après la zone : **Opacity**, **Color**, **Flame glow**, **Shadow**. La simulation n'est pas refaite.
```

## Partie 3 : un obstacle

```essayer
exemple: fumée contre un obstacle
```

```etape
Lis jusqu'à l'image 60. La fumée bute sur une boule et la contourne.
verifier: lecture.image >= 60
```

L'obstacle se règle sur **Smoke solver** : **Ball center** et **Ball radius**, ou **Box center** et **Box size**. La boule bleue que tu vois est faite de points, ajoutés à côté pour la montrer.

```etape
Mets **Ball radius** à 0. Relis. La fumée monte tout droit, à travers les points bleus : il n'y a plus d'obstacle.
```

Remets 0,26.

```astuce
Dans une scène 3D, branche un node **Scene object** sur **Ball center** et **Ball radius**. L'obstacle suit alors un objet de la scène, même animé.
```

## Qualité et vitesse

Trois réglages décident du temps de calcul.

| Réglage | Où | Effet |
|---|---|---|
| **Resolution** | **Empty volume** | Le détail. 64 pour chercher, 128 pour un aperçu fin, 256 pour finir |
| **Pressure iterations** | **Smoke solver** | La justesse du mouvement. 2 pour aller vite, 4 par défaut, 8 pour finir |
| **Substeps** | **Smoke solver** | À monter si la fumée va très vite (une explosion) |

En bas de l'aperçu, tu lis ce que le calcul donne : où il se fait, le temps d'un pas, et `divergence`. Ce nombre mesure l'air qui se crée ou disparaît par erreur. Plus il est petit, mieux c'est. Chaque itération de pression le divise par cinq environ.

```attention
Changer **Resolution** repart de zéro : toute la simulation est recalculée.
```

```quiz
La source d'un feu pose du carburant chaud, pas de fumée. D'où vient la fumée ?
- [ ] Du node Smoke solver.
- [x] Du node Combustion : le carburant qui brûle en produit.
- [ ] Du node Empty volume.
explication: Combustion transforme le carburant en chaleur, en flamme et en fumée. Son réglage Smoke dit combien.
```

```quiz
Tu doubles Resolution. Que devient le nombre de voxels ?
- [ ] Il double.
- [ ] Il est multiplié par quatre.
- [x] Il est multiplié par huit.
explication: La boîte a trois dimensions : deux fois plus de voxels dans chacune, donc 2 × 2 × 2.
```

```defi
Fais un feu qui penche sous le vent, avec une fumée noire et épaisse.
indice: Wind sur le Smoke solver, Smoke sur Combustion, puis un node Smoke look avec une couleur sombre et plus d'Opacity.
```

## En trois lignes

- Une fumée : **Empty volume**, puis dans la zone **Smoke source** et **Smoke solver**.
- Un feu : la source pose du carburant chaud, **Combustion** le brûle.
- **Resolution** fait le détail, **Pressure iterations** la justesse.

Pour tous les réglages, garde sous la main le guide [Fumée et feu](../../utilisateur/simulation-fumee.md).
