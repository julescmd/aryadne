---
titre: Scatter, booléen et relief
niveau: intermédiaire
duree: 15 min
prerequis: 08-modelisation/04-les-modificateurs
objectifs: Semer des copies sur une surface avec Scatter ; Creuser une forme avec Boolean ; Donner du relief avec Displace ; Refaire une surface avec Remesh
---

## Ce que tu vas réussir

Tu vas fabriquer un bout de terrain : un sol bosselé, de l'herbe, des cailloux, et un trou creusé dedans. Quatre modificateurs, zéro sommet déplacé à la main.

```essayer
nouveau: aryscene
nom: terrain
```

## Le sol

```etape
Ajoute un **Plane** par le menu **Add**. Agrandis-le avec S. Supprime le cube de départ.
verifier: scene.objets >= 3
```

```etape
Choisis le plan. Ouvre l'onglet **Modifiers**. Ajoute une **Subdivision**, en **Simple**, avec **Levels** à 5.
verifier: scene.selection
```

Un plan n'a que quatre sommets. Un relief a besoin de sommets à déplacer : la subdivision simple les fournit, sans arrondir les bords.

## Displace : le relief

```etape
Ajoute un modificateur **Displace (noise)**. Le sol se bosselle.
```

| Réglage | Effet |
|---|---|
| **Strength** | La hauteur des bosses |
| **Scale** | Leur taille |
| **Octaves** | Le nombre de couches de détail |
| **Roughness** | La part des petits détails |
| **Seed** | Un autre tirage |
| **Direction** | **Normal**, ou un axe |

```etape
Règle **Scale** pour de larges collines, puis monte **Octaves** pour y ajouter du grain.
```

```etape
Change **Seed**. Le terrain est différent, avec le même caractère.
```

## Scatter : semer

**Scatter (instances)** pose des copies d'une petite forme sur la surface.

```etape
Ajoute un modificateur **Scatter (instances)**. Place-le **sous** **Displace (noise)** dans la pile, pour semer sur le terrain déjà bosselé.
```

```etape
Dans **Shape**, choisis **grass**. Monte **Count** vers 2000.
```

| Réglage | Effet |
|---|---|
| **Shape** | **pebbles**, **grass**, **cubes**, **spheres**, **cones** |
| **Count** | Le nombre de copies, de 1 à 5000 |
| **Scale** | Leur taille |
| **Size variation** | 0 : toutes pareilles. 1 : très inégales |
| **Top only** | Ne sème que sur les faces tournées vers le haut |
| **Seed** | Un autre tirage |
| **Follow the surface** | Les copies se dressent selon la surface. Décoché : toutes droites |

```etape
Monte **Size variation** à 0,8. L'herbe paraît plus naturelle.
```

```etape
Décoche **Follow the surface**. Les brins poussent tous vers le haut, comme de la vraie herbe sur une pente.
```

### Une seconde couche

```etape
Sur **Scatter (instances)**, clique **Duplicate**. Dans la copie, choisis **pebbles**, baisse **Count** à 60 et change **Seed**.
```

Deux semis, deux tirages : l'herbe et les cailloux ne tombent pas aux mêmes endroits.

```astuce
**Top only** garde les copies sur le dessus. Sur un rocher, la mousse ne pousse pas dessous.
```

## Boolean : creuser

**Boolean (shape)** ajoute, creuse ou garde l'intersection avec une forme simple.

```etape
Ajoute un cube neuf par **Add** et choisis-le. Ajoute-lui un modificateur **Boolean (shape)**.
verifier: scene.objets >= 4
```

```etape
Choisis **Operation** : **Difference**, et **Shape** : **Sphere**. Règle **Center** et **Half size** pour que la sphère morde un coin du cube.
```

| Réglage | Effet |
|---|---|
| **Operation** | **Union**, **Difference**, **Intersection** |
| **Shape** | **Cube**, **Sphere**, **Cylinder** |
| **Center**, **Half size** | Où est la forme, et sa taille |
| **Voxel size** | Plus petit : plus de détail, plus de faces |
| **Smoothing** | Adoucit le résultat |

```attention
Le booléen agit avec une forme simple, pas avec un autre objet de la scène. Il passe par des voxels : le résultat est un maillage régulier, pas une coupe nette. Baisse **Voxel size** pour des bords plus précis.
```

## Remesh : repartir d'une surface saine

**Remesh (voxels)** refait toute la surface en quads réguliers, sans trous ni faces qui se croisent.

```etape
Sur le cube creusé, ajoute un **Remesh (voxels)**. Baisse **Voxel size** : le détail augmente, le nombre de faces aussi.
```

C'est le point de départ habituel avant de sculpter. Voir [La sculpture](../09-sculpture/index.md).

## Trois effets à essayer

| Modificateur | Effet |
|---|---|
| **Wave** | Des vagues, droites ou circulaires |
| **Melt** | La forme s'affaisse vers le sol |
| **Explode** | Les faces s'écartent le long de leurs normales |

```etape
Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```quiz
Tu veux de l'herbe sur un terrain bosselé. Où placer Scatter dans la pile ?
- [ ] Au-dessus de Displace.
- [x] Sous Displace, pour semer sur le terrain déjà déformé.
- [ ] Peu importe.
explication: Les modificateurs s'appliquent de haut en bas. Semer avant de bosseler déformerait aussi l'herbe.
```

```quiz
Avec quoi le modificateur Boolean creuse-t-il ?
- [ ] Avec n'importe quel objet de la scène.
- [x] Avec une forme simple : cube, sphère ou cylindre.
- [ ] Avec une image.
explication: Dans le code actuel, le booléen utilise une forme intégrée, placée par Center et Half size.
```

```defi
Fais un rocher moussu : une sphère déformée, avec de l'herbe seulement sur le dessus.
indice: Sphere, Displace (noise) fort, puis Scatter en grass avec Top only monté.
```

## En trois lignes

- **Displace (noise)** bosselle, à condition d'avoir assez de sommets.
- **Scatter (instances)** sème des copies. **Seed** change le tirage.
- **Boolean (shape)** creuse avec une forme simple. **Remesh (voxels)** refait une surface saine.

[Suite : La retopologie](06-la-retopologie.md)
