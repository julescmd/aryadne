---
titre: La retopologie
niveau: avancé
duree: 20 min
prerequis: 08-modelisation/05-scatter-booleen-relief
objectifs: Lancer une retopologie automatique ; La guider par des traits et de la densité peinte ; Régler symétrie et arêtes vives ; Appliquer le résultat
---

## Ce que tu vas réussir

Tu vas prendre un maillage dense et irrégulier, et en tirer un maillage propre, tout en quads. Tu vas le guider pour que ses boucles tombent là où tu les veux.

## Pourquoi

Un sculpt ou un scan a des millions de triangles, placés au hasard. Impossible à déplier proprement, à animer, à mettre dans un jeu. La retopologie refait la surface avec peu de faces, en **quads**, dont les boucles suivent les formes.

Dans Aryadne, elle est automatique. Tu la guides.

```essayer
nouveau: aryscene
nom: retopologie
```

## Fabrique un modèle dense

```etape
Choisis le cube. Ajoute-lui les modificateurs **Subdivision**, **Levels** 2, puis **Displace (noise)**, puis **Remesh (voxels)** avec une petite **Voxel size**.
verifier: scene.selection
```

```etape
Dans les options de la pile, clique **Apply all**. Le résultat devient le maillage de base : un caillou dense.
```

## Ouvre l'outil

```etape
Dans la barre de la vue, passe en mode **UV & materials**. Ouvre l'onglet **Retopo**.
```

## Premier essai

```etape
Mets **Target faces** à 500.
```

```etape
Clique **Run retopology**. L'avancement s'affiche : **Preparing mesh**, **Computing flow**, **Placing poles**, **Building quads**.
```

Le résultat apparaît en fil de fer bleu par-dessus le modèle, avec le nombre de pôles. **Cancel** arrête un calcul en cours.

```attention
Le nombre de faces obtenu s'approche de **Target faces**, sans l'atteindre exactement.
```

## Les pôles

Un pôle est un sommet où se rejoignent 3 ou 5 arêtes, au lieu de 4. Un maillage de quads en a forcément : un cube en donne 8.

Moins de pôles donne un maillage plus régulier, mais des boucles qui suivent moins bien les formes. Le réglage **Follow curvature** décide du compromis.

```etape
Monte **Follow curvature** à 1,5 et relance. Les boucles épousent mieux les bosses, avec quelques pôles en plus.
```

## Guide avec des traits

```etape
Glisse sur le modèle pour dessiner un trait. Les boucles de quads suivront sa direction.
```

Pendant ce temps, Maj ou le clic droit tournent la vue.

```etape
Dessine un trait **fermé** autour d'une bosse. Relance. Une boucle fait maintenant le tour de la bosse.
```

Sur un visage, tu fermes un trait autour de chaque œil et de la bouche. Sur un membre, autour du genou et du coude.

| Bouton | Effet |
|---|---|
| **Undo last** | Retire le dernier guide |
| **Clear guides** | Les retire tous |
| **Guide reach** | Jusqu'où un trait influence le flux autour de lui |

## Peins la densité

```etape
Active **Density brush**. Peins une zone du modèle : elle recevra plus de quads. Relance.
```

**Radius** et **Density** règlent le pinceau. **Clear density** efface la peinture. Sur un personnage, tu densifies le visage, les mains et les articulations.

## Les autres réglages

| Réglage | Effet |
|---|---|
| **Symmetry** | Le maillage est calculé sur une moitié puis recopié en miroir, avec une boucle sur le plan |
| **Automatic loops** | Aryadne cherche les rebords qui font le tour d'un détail et y met des boucles |
| **Adaptive size** | Les quads rétrécissent là où les boucles se resserrent |
| **Hard edges** | Au-delà de cet angle, une arête devient une boucle nette. 0 : désactivé |
| **Separate objects** | Chaque morceau non relié est traité à part : **Same density** ou **By detail** |

```astuce
Forme mécanique : monte **Hard edges** vers 30 degrés. Forme organique : laisse-le bas, coche **Automatic loops** et dessine des guides fermés.
```

## Applique

Sous le résultat, une ligne donne le nombre de quads, de pôles et de sommets. La case **Show** affiche ou cache le fil de fer bleu.

```etape
Quand le maillage te plaît, clique **Apply**. La retopologie remplace le maillage de l'objet, et tu reviens à la scène.
```

Un message confirme : « Retopology applied. Ctrl+Z restores the old mesh ». En bas de la vue, le nombre de triangles a baissé.

```etape
Annule avec Ctrl+Z : le caillou dense revient. Rétablis avec Ctrl+Maj+Z : la retopologie est de nouveau là.
```

**Apply** tient en une seule étape de l'historique. Et rien n'est perdu.

| Ce qui devient | Comment |
|---|---|
| L'ancien maillage | Son fichier reste à côté de la scène. Le nouveau s'appelle « … retopo.obj » |
| Les IDs de matériau | Ils suivent : chaque face prend l'ID de la surface qu'elle recouvre |
| Les UV | Ils ne suivent pas : les faces ont changé. Il faut redéplier |
| Les détails | La cuisson les reporte depuis l'ancien maillage, dans une carte de normales |

```astuce
Plus tard, même après avoir fermé la scène, l'ancien maillage se reprend. Reviens dans l'onglet **Retopo** : une flèche de retour suit **Previous mesh**. Un clic le reprend, un second clic revient à la retopologie.
```

Le bouton à la flèche vers le bas, à côté d'**Apply**, écrit seulement un fichier OBJ à côté. L'objet ne change pas : c'est pour envoyer la retopologie ailleurs.

```etape
Enregistre la scène avec Ctrl+S.
verifier: fichier.enregistre
```

## Si ça échoue

Le message « Retopology failed on this model » propose deux pistes : viser moins de faces, ou mettre **Hard edges** à 0. Sur un modèle abîmé, avec des trous ou des faces qui se croisent, passe d'abord par **Remesh (voxels)**.

## Et après

Le nouveau maillage n'a pas d'UV. La suite se passe dans l'onglet **UV** : voir [UV et textures](../10-uv-textures/index.md).

Les détails du modèle dense ne sont pas perdus. Dans l'onglet **UV** du nouveau maillage, le champ **High-poly** est déjà rempli avec l'ancien. Une fois les UV dépliés, **Bake textures** écrit ses reliefs dans une carte de normales : voir [La cuisson](../10-uv-textures/06-la-cuisson.md).

```attention
Il n'y a pas d'outil de retopologie manuelle, pour poser les quads un par un. Le guidage passe par les traits, la densité et les réglages.
```

```quiz
Comment obtenir une boucle de quads autour d'un œil ?
- [ ] En montant Target faces.
- [x] En dessinant un trait fermé tout autour.
- [ ] En cochant Symmetry.
explication: Un guide fermé demande une boucle. Automatic loops en trouve aussi tout seul sur les rebords nets.
```

```quiz
Tu as cliqué Apply, et le résultat ne te plaît pas. Que faire ?
- [ ] Rien : l'ancien maillage est effacé.
- [x] Ctrl+Z dans la scène, ou la flèche Previous mesh dans l'onglet Retopo.
- [ ] Recharger le fichier d'origine à la main.
explication: Apply est une étape annulable, et le fichier de l'ancien maillage reste à côté de la scène.
```

```quiz
Qu'est-ce qu'un pôle ?
- [ ] Une face à cinq côtés.
- [x] Un sommet où se rejoignent 3 ou 5 arêtes au lieu de 4.
- [ ] Le centre du modèle.
explication: Un maillage de quads a forcément des pôles. L'outil choisit où les placer.
```

```defi
Retopologise le même caillou trois fois : à 200, 1000 et 5000 faces. Compare ce que chaque version garde de la forme.
indice: Change seulement Target faces. Regarde à partir de quand les bosses sont bien décrites.
```

## En trois lignes

- La retopologie refait un maillage dense en quads propres.
- Traits, densité peinte et **Follow curvature** guident le flux.
- **Apply** remplace le maillage en une étape annulable. Après, il faut déplier : le nouveau maillage n'a pas d'UV.

Parcours terminé. Pour sculpter le détail, passe à [La sculpture](../09-sculpture/index.md).
