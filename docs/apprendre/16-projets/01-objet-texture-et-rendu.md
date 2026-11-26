---
titre: Projet 1 : un objet texturé et rendu
niveau: avancé
duree: 45 min
prerequis: 08-modelisation/04-les-modificateurs, 11-materiaux-atelier/03-les-masques, 15-animation-rendu/07-rendu
objectifs: Modéliser un vase avec des modificateurs ; Lui construire un matériau à calques ; L'éclairer en studio ; Rendre une image finale
---

## Ce que tu vas réussir

Une image finie : un vase en céramique usée, posé dans un studio, rendu en tracé de chemins. Tu passes par quatre outils : la scène, les modificateurs, l'Atelier, le rendu.

```essayer
nouveau: aryscene
nom: projet-vase
```

## Étape 1 : la forme

Détail des gestes : [Les modificateurs](../08-modelisation/04-les-modificateurs.md).

```etape
Choisis le cube. Entre en mode édition avec Tab. En mode **Faces**, choisis la face du dessus et supprime-la : X, puis **Delete faces**. Sors avec Tab.
verifier: scene.selection
```

```etape
Étire le cube en hauteur avec S puis l'axe vertical.
```

Un cube n'a de sommets qu'en haut et en bas. Pour lui donner un ventre, il en faut au milieu.

```etape
Dans l'onglet **Modifiers**, ajoute **Subdivision**. Choisis **Simple** et mets **Levels** à 2. Le cube gagne des sommets sans changer de forme.
```

```etape
Ajoute **Taper**. Règle **Factor** pour resserrer le haut : c'est le col.
```

```etape
Ajoute **Axial bulge (lattice)**. Règle **Factor** pour donner du ventre au milieu.
```

```etape
Ajoute **Solidify** et règle **Thickness** : la paroi prend de l'épaisseur.
```

```etape
Ajoute une seconde **Subdivision**, en **Catmull-Clark**, avec **Levels** à 2. Vérifie qu'elle est tout en bas de la pile. Le vase est lisse.
```

```etape
Dans l'onglet **World**, coche **Smooth shading**.
```

```astuce
La forme ne te plaît pas ? Rien n'est figé. Bouge les curseurs de **Taper** et de **Axial bulge (lattice)** jusqu'à la bonne silhouette.
```

## Étape 2 : le matériau

Détail des gestes : [Les masques](../11-materiaux-atelier/03-les-masques.md).

```etape
Le vase choisi, ouvre l'onglet **Material** des propriétés. Clique le petit **＋** : un matériau est créé, appliqué au vase et ouvert dans l'Atelier.
verifier: fichier.ouvert = arymat
```

```etape
Dans la bibliothèque, filtre **Presets** et clique **Ceramic**. Supprime le calque **Graph** du départ.
verifier: atelier.calques >= 1
```

```etape
Clique **+**. Nomme le calque « émail ». Donne-lui un bleu profond, **Roughness** 0,15.
verifier: atelier.calques >= 2
```

```etape
Dans **Mask**, clique **Edge wear** et coche **Invert**. Règle **Amount** : la céramique nue ressort sur le bord du col.
```

```etape
Clique **+**. Nomme le calque « saleté ». Brun sombre, **Roughness** 0,9. Masque **Cavity dirt**, **Amount** vers 0,3.
verifier: atelier.calques >= 3
```

```etape
Clique **+**. Nomme le calque « poussière ». Beige clair, **Roughness** 1. Masque **Dust on top**, **Opacity** à 40 %.
verifier: atelier.calques >= 4
```

```etape
Enregistre le matériau avec Ctrl+S.
verifier: fichier.enregistre
```

```etape
Reviens à l'onglet de la scène. Le vase porte son matériau, mis à jour en direct.
verifier: sous_onglet = scene
```

## Étape 3 : la mise en scène

Détail des gestes : [Lumières et caméra](../07-3d-bases/04-lumieres-et-camera.md).

```etape
Ajoute un **Plane** sous le vase. Agrandis-le largement : c'est la table.
verifier: scene.objets >= 4
```

```etape
Supprime la lumière de départ. Ajoute une **Area light**, en haut à gauche, tournée vers le vase. Monte sa **Strength**.
```

```etape
Ajoute une seconde **Area light**, plus faible, à droite, pour déboucher les ombres.
verifier: scene.objets >= 5
```

```etape
Tourne la vue pour voir le vase de trois quarts, un peu de haut. Choisis la caméra et clique **Set from view**, dans l'onglet **Camera**.
```

```etape
Baisse **Field of view** et recule la caméra. Le vase se déforme moins.
```

## Étape 4 : le rendu

Détail des gestes : [Rendu et export](../15-animation-rendu/07-rendu.md).

```etape
Ouvre **Render** et choisis **Path tracing (realistic)**. Mets **Bounces** à 6.
```

```etape
Regarde par la caméra avec 0. Monte un peu **Depth of field** et règle **Focus distance** sur le vase.
```

```etape
Retouche tes lumières en regardant l'image s'affiner. Une lumière trop forte brûle l'émail : baisse-la.
```

```etape
Appuie sur F12. Choisis **1080p**, 512 **Samples**, **Denoise**. Clique **Render**.
```

```etape
Clique **Save…** et enregistre l'image dans ton coffre, sous le nom « vase ».
```

```etape
Enregistre la scène avec Ctrl+S.
verifier: fichier.enregistre
```

## Ce que tu viens de faire

| Outil | Ce que tu y as fait |
|---|---|
| Mode édition | Ouvrir le haut du cube |
| Modificateurs | Toute la forme, sans la figer |
| Atelier | Quatre calques, trois masques |
| Scène | Table, lumières, caméra |
| Rendu | L'image finale |

Aucune de ces étapes n'est définitive. Tu peux rouvrir le matériau, changer une lumière, épaissir la paroi, et relancer le rendu.

```quiz
Le matériau du vase est-il copié dans la scène ?
- [ ] Oui, il faut le réimporter après chaque changement.
- [x] Non : la scène pointe vers le fichier du matériau, qui se met à jour en direct.
- [ ] Il est cuit en images.
explication: Un matériau appliqué suit les changements de son fichier. La cuisson ne sert qu'à l'export vers d'autres logiciels.
```

```quiz
Pourquoi placer la subdivision Catmull-Clark en bas de la pile ?
- [x] Pour lisser le résultat des déformations et de l'épaisseur.
- [ ] Parce que c'est le plus lent.
- [ ] Pour qu'elle soit ignorée.
explication: Les modificateurs s'appliquent de haut en bas. On déforme d'abord un maillage léger, puis on lisse.
```

```defi
Décline ton image : trois vases de formes et de couleurs différentes, côte à côte.
indice: Duplique le vase avec Maj+D et change les curseurs de ses modificateurs. Pour la couleur, il faut un matériau par vase : enregistre ta pile en matériau intelligent, puis repose-la dans un matériau neuf.
```

## En trois lignes

- La forme reste en modificateurs, le matériau en calques : tout se retouche.
- Le **＋** de l'onglet **Material** crée, applique et ouvre le matériau.
- 0 pour la caméra, F12 pour l'image.

[Suite : Projet 2, un plan avec un tissu simulé, rendu et monté](02-plan-anime-rendu-et-monte.md)
