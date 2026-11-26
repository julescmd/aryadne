---
titre: Ajouter et organiser des objets
niveau: débutant
duree: 15 min
prerequis: 07-3d-bases/02-selectionner-et-transformer
objectifs: Ajouter des formes ; Dupliquer, cacher et supprimer ; Lier des objets par la parenté ; Joindre plusieurs maillages
---

## Ce que tu vas réussir

Tu vas construire une petite table avec six formes : un plateau, quatre pieds, un vase. Tu vas les lier pour les déplacer d'un seul geste.

```essayer
nouveau: aryscene
nom: petite-table
```

## Ajouter une forme

Le menu **Add** de la barre ajoute un objet. Maj+A ouvre le même menu sous la souris.

| Entrée | Ce qu'elle ajoute |
|---|---|
| Shapes | **Cube**, **Sphere**, **Plane**, **Cylinder**, **Cone**, **Torus** |
| **Empty** | Un objet sans forme, pour en diriger d'autres |
| **3D text…** | Du texte en volume |
| **Models in the vault** | Un modèle de ton coffre |
| **Pick a file…** | Un modèle ou une image du disque |

Tu peux aussi glisser un modèle depuis ton bureau sur la fenêtre.

## Le plateau

Le cube de départ sera le plateau.

```etape
Choisis le cube. Appuie sur S, puis sur Y, et écrase-le pour en faire une planche. Valide.
verifier: scene.selection
```

```etape
Monte-le avec G puis Y, d'environ une unité.
```

```attention
Si la planche s'écrase de côté au lieu de s'aplatir, l'axe vertical de ta scène n'est pas Y. Annule, et essaie S puis Z.
```

## Un pied

```etape
Appuie sur Maj+A dans la vue et choisis **Cylinder**.
verifier: scene.objets >= 4
```

```etape
Affine-le avec S, puis place-le sous un coin du plateau avec G. Passe en vue de dessus, avec 7, pour viser juste.
```

## Trois autres pieds

```etape
Le pied choisi, appuie sur Maj+D. Une copie est créée. Amène-la sous un autre coin avec G.
verifier: scene.objets >= 5
```

```etape
Recommence deux fois. Tu as quatre pieds.
verifier: scene.objets >= 7
```

```raccourci
Maj+D : dupliquer l'objet choisi.
```

## Un vase

```etape
Ajoute une **Sphere**. Réduis-la et pose-la sur le plateau.
verifier: scene.objets >= 8
```

## Lier par la parenté

Pour l'instant, déplacer le plateau laisse les pieds sur place. Tu vas faire du plateau leur **parent**.

```etape
Clique un pied. Tiens Maj et clique les trois autres, puis le vase. Clique le plateau **en dernier** : il devient l'objet actif.
```

```etape
Appuie sur P. Les objets choisis deviennent les enfants du plateau.
```

```etape
Clique dans le vide, puis choisis le plateau seul. Déplace-le avec G. Toute la table suit.
```

Un enfant suit les déplacements, les rotations et les changements de taille de son parent. Il garde sa liberté : tu peux encore déplacer le vase seul.

Alt+P retire le parent des objets choisis.

## Un parent invisible : Empty

Pour déplacer un ensemble sans qu'aucun objet ne commande les autres, ajoute un **Empty** et fais-en le parent de tous. Il n'apparaît pas au rendu.

## Cacher, montrer, supprimer

```etape
Choisis le vase et appuie sur H. Il est caché. Appuie sur Alt+H : tout réapparaît.
```

L'œil du panneau **Scene** fait la même chose, objet par objet.

```etape
Choisis la caméra dans le panneau **Scene**, puis l'un des pieds. Supprime le pied avec X ou Suppr. Annule aussitôt avec Ctrl+Z.
```

Supprimer retire l'objet de la scène. Le fichier du modèle, lui, reste dans le coffre.

## Joindre

Quatre pieds, ce sont quatre objets. Tu peux en faire un seul maillage.

```etape
Choisis les quatre pieds, l'un après l'autre avec Maj. Appuie sur Ctrl+J : **Join into active**.
```

Il faut au moins deux objets à maillage. Le résultat prend la place de l'objet actif, celui que tu as cliqué en dernier.

## Le menu Object

Tout ce que tu viens de faire au clavier est aussi dans le menu **Object**, et dans le clic droit sur un objet.

| Raccourci | Menu | Effet |
|---|---|---|
| Maj+D | **Duplicate** | Dupliquer |
| X ou Suppr | **Delete** | Retirer de la scène |
| H, Alt+H | **Hide**, **Show all** | Cacher, tout montrer |
| P, Alt+P | **Parent to active**, **Clear parent** | Lier, délier |
| Ctrl+J | **Join into active** | Joindre des maillages |

```quiz
Tu veux que quatre pieds suivent un plateau. Dans quel ordre cliques-tu ?
- [ ] Le plateau d'abord, puis les pieds.
- [x] Les pieds d'abord, le plateau en dernier, puis P.
- [ ] L'ordre ne compte pas.
explication: Le dernier objet cliqué est l'objet actif. P fait des autres ses enfants.
```

```quiz
Tu supprimes un objet de la scène. Qu'arrive-t-il au fichier de son modèle ?
- [ ] Il est supprimé du coffre.
- [x] Il reste dans le coffre.
- [ ] Il est déplacé dans la corbeille.
explication: La scène ne fait que pointer vers les modèles. Supprimer retire le lien, pas le fichier.
```

```defi
Ajoute quatre chaises autour de la table, sans en modéliser quatre.
indice: Construis une chaise, fais d'un Empty le parent de ses pièces, puis duplique l'ensemble avec Maj+D.
```

## En trois lignes

- **Add**, ou Maj+A, ajoute formes, lumières et modèles.
- Maj+D duplique, H cache, X supprime de la scène.
- P lie les objets choisis à l'objet actif. Ctrl+J les joint en un seul maillage.

[Suite : Lumières et caméra](04-lumieres-et-camera.md)
