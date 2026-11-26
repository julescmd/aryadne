---
titre: Les modificateurs
niveau: intermédiaire
duree: 20 min
prerequis: 08-modelisation/02-extruder-incruster-biseauter
objectifs: Ajouter et ordonner des modificateurs ; Lisser avec Subdivision ; Travailler en miroir ; Limiter un effet à une zone ; Appliquer la pile
---

## Ce que tu vas réussir

Tu vas transformer un cube en vase torsadé sans toucher à un seul sommet. Puis tu vas changer d'avis, et tout modifier en deux clics.

```essayer
nouveau: aryscene
nom: modificateurs
```

## L'idée

Un modificateur transforme le maillage du précédent, sans toucher au fichier d'origine. Ils s'appliquent de haut en bas. Tu peux en changer l'ordre, les éteindre, les retirer : rien n'est définitif.

```etape
Choisis le cube. Dans **Properties**, ouvre l'onglet **Modifiers**.
verifier: scene.selection
```

Le bouton **Modifiers** de la barre de la vue ouvre la même pile.

## Subdivision : lisser

```etape
Clique **＋ Add**. Dans la recherche, tape « subdivision ». Clique le modificateur.
```

```etape
Monte **Levels** à 3. Le cube devient une boule lisse.
```

| Réglage | Effet |
|---|---|
| **Levels** | Chaque niveau multiplie les faces par quatre |
| **Catmull-Clark** | Lisse et densifie |
| **Simple** | Densifie sans lisser |
| **Optimal display** | Le fil de fer ne montre que la cage d'origine |

```attention
La subdivision est plafonnée. Au-delà d'environ 2,5 millions de triangles affichés, le nombre de niveaux est limité et un message le dit.
```

## L'ordre compte

```etape
Ajoute un modificateur **Twist**. Monte son angle. La boule se tord à peine : une boule tordue reste une boule.
```

```etape
Ajoute un modificateur **Taper** et monte **Factor**. La forme se resserre vers un bout.
```

```etape
Attrape la poignée de **Subdivision** et glisse-la tout en bas de la pile.
```

Le résultat change. Le cube est d'abord tordu et resserré, puis lissé. Tu obtiens un vase torsadé, aux formes bien plus franches.

```astuce
Règle générale : les déformations d'abord, la subdivision en dernier. Tu déformes peu de sommets, et le lissage arrondit le résultat.
```

## Éteindre pour comparer

```etape
Clique l'œil de **Twist**. La torsion disparaît. Reclique.
```

Chaque modificateur a aussi **Duplicate** et **Delete**.

## Donner une épaisseur

Un vase a des parois.

```etape
Entre en mode édition avec Tab. En mode **Faces**, choisis la face du dessus. Appuie sur X et choisis **Delete faces**. Sors avec Tab.
```

Le vase est ouvert, mais sa paroi n'a pas d'épaisseur.

```etape
Ajoute un modificateur **Solidify**. Règle **Thickness**. Place-le au-dessus de **Subdivision** dans la pile.
```

## Limiter un effet à une zone

Chaque déformation a un bloc **Influence and area**.

```etape
Déplie **Twist**. Coche **Limit to a zone**. Règle **Start** et **End** pour que la torsion ne touche que le milieu du vase.
```

```etape
Monte **Falloff**. Le passage entre la zone tordue et le reste devient progressif.
```

| Réglage | Effet |
|---|---|
| **Influence** | Dose l'effet, de 0 à 1 |
| **Limit to a zone** | N'agit que sur une partie, le long d'un axe |
| **Start**, **End** | Les bornes |
| **Falloff** | La largeur du fondu |
| **Invert the zone** | Agit partout sauf dans la zone |

## Le miroir

```etape
Ajoute un cube neuf par **Add**. Entre en mode édition, déplace quelques sommets d'un seul côté, puis sors.
verifier: scene.objets >= 4
```

```etape
Ajoute-lui un modificateur **Mirror**. L'autre côté reprend la forme, en reflet.
```

**Weld in the middle** soude les deux moitiés. **Cut first (bisect)** coupe d'abord le maillage sur le plan.

## Les familles

| Famille | Exemples |
|---|---|
| **Generate** | **Mirror**, **Array**, **Subdivision**, **Solidify**, **Wireframe**, **Screw**, **Boolean (shape)**, **Scatter (instances)** |
| **Deform** | **Smooth**, **Twist**, **Bend**, **Taper**, **Stretch**, **Inflate** |
| **Organic** | **Displace (noise)**, **Wave**, **Ripple**, **Melt**, **Crumple** |
| **Topology** | **Decimate**, **Weld**, **Triangulate**, **Remesh (voxels)** |
| **Effects** | **Explode**, **Voxelize**, **Build** |

## Appliquer

Tant qu'un modificateur est dans la pile, tu ne peux pas modeler son résultat en mode édition : tu modèles toujours le maillage de base.

```etape
Choisis le vase. Sur **Twist**, clique **Apply**. La torsion entre dans le maillage de base. Le modificateur quitte la pile.
```

Les options de la pile vont plus loin : **Apply all**, **Clear the stack**, et **Restore the file's mesh**, qui oublie tout ce qui a été appliqué.

```attention
**Decimate** et **Remesh (voxels)** changent le nombre de sommets. Les UV posées avant sont perdues.
```

```etape
Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```quiz
Dans quel ordre placer une torsion et une subdivision pour un résultat net ?
- [x] La torsion d'abord, la subdivision ensuite.
- [ ] La subdivision d'abord, la torsion ensuite.
- [ ] L'ordre ne change rien.
explication: Les modificateurs s'appliquent de haut en bas. Déformer puis lisser garde des formes franches.
```

```quiz
Que fait Apply sur un modificateur ?
- [ ] Il le désactive.
- [x] Son résultat devient le maillage de base.
- [ ] Il l'exporte.
explication: Appliquer rend l'effet définitif dans le maillage. Restore the file's mesh permet encore de revenir en arrière.
```

```defi
Fais un escalier en colimaçon à partir d'une seule marche.
indice: Une marche plate, puis le modificateur Screw, ou Array avec un décalage et une rotation par copie.
```

## En trois lignes

- Les modificateurs s'empilent et s'appliquent de haut en bas.
- Œil, poignée et **Influence and area** : tout reste réglable.
- **Apply** rend l'effet définitif dans le maillage.

[Suite : Scatter, booléen et relief](05-scatter-booleen-relief.md)
