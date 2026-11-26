---
titre: Couper, souder, nettoyer
niveau: intermédiaire
duree: 20 min
prerequis: 08-modelisation/02-extruder-incruster-biseauter
objectifs: Ajouter une boucle d'arêtes ; Couper au couteau ; Fusionner et dissoudre ; Boucher un trou ; Travailler en miroir et en édition proportionnelle
---

## Ce que tu vas réussir

Tu vas ajouter du détail là où il en faut, retirer ce qui est de trop, et réparer un trou. Les gestes de tous les jours, une fois la forme posée.

```essayer
nouveau: aryscene
nom: couper-souder
```

```etape
Choisis le cube et entre en mode édition avec Tab.
verifier: scene.selection
```

## La coupe en boucle

Une boucle d'arêtes fait le tour du maillage. En ajouter une donne des sommets là où tu en as besoin.

```etape
Appuie sur Ctrl+R. Survole une arête verticale : l'aperçu d'une boucle s'affiche autour du cube.
```

```etape
Tourne la molette : le nombre de coupes change. Reviens à une seule.
```

```etape
Clique : la coupe est faite, puis elle glisse avec la souris. Clique encore pour la poser. Un clic droit la pose au milieu.
```

```raccourci
Ctrl+R : coupe en boucle. Les touches 1 à 9 donnent le nombre de coupes.
```

```etape
La nouvelle boucle est choisie. Appuie sur S et élargis-la. Le cube prend du ventre.
```

## Le couteau

Le couteau coupe où tu veux, pas seulement en boucle.

```etape
Appuie sur K. Clique sur une arête, puis sur l'arête d'en face, à travers une face.
```

```etape
Appuie sur Entrée pour couper.
```

- Chaque segment doit traverser une face, d'un bord à l'autre.
- Retour arrière enlève le dernier point.
- Entrée ou clic droit coupe. Échap annule.

## Fusionner

```etape
Passe en mode **Vertices** avec 1. Choisis deux sommets voisins avec Maj+clic.
```

```etape
Appuie sur M. Les deux sommets n'en font plus qu'un, au centre.
```

**Merge by distance**, dans le menu du clic droit, fusionne tous les sommets plus proches qu'une distance. C'est le remède aux sommets en double après un import.

## Supprimer ou dissoudre

Deux façons de retirer, deux résultats.

| Action | Résultat |
|---|---|
| **Delete** | L'élément disparaît, et un trou reste |
| **Dissolve** | L'élément disparaît, la surface reste fermée |

```etape
Passe en mode **Edges**. Choisis la boucle que tu as ajoutée avec Alt+clic. Appuie sur X : le menu de suppression s'ouvre. Choisis **Dissolve edges**.
```

La boucle a disparu, le cube est intact.

```etape
Passe en mode **Faces**. Choisis une face. Appuie sur X et choisis **Delete faces**. Il y a maintenant un trou.
```

## Boucher un trou

```etape
Passe en mode **Edges**. Fais un Alt+clic sur le bord du trou pour prendre sa boucle.
```

```etape
Appuie sur F. Une face ferme le trou.
```

```raccourci
F : créer une face entre les éléments choisis.
```

Deux autres outils rebouchent, dans le menu du clic droit.

- **Grid fill** remplit une boucle par une grille de quads. Il faut une seule boucle, avec un nombre pair de sommets.
- **Bridge edge loops** relie deux boucles par des faces. C'est ainsi qu'on perce un tunnel ou qu'on raccorde deux pièces.

## Détacher

| Outil | Touche | Effet |
|---|---|---|
| **Rip** | V | Déchire le maillage aux sommets choisis |
| **Split** | Y | Détache les faces choisies, sans les déplacer |
| **Separate selection** | P | Les faces choisies deviennent un nouvel objet |

## Trois aides de la barre

### Le miroir en X

```etape
Clique **X mirror** dans la barre. Passe en mode **Vertices** et déplace un sommet du côté droit. Son reflet, à gauche, suit.
```

Pour un personnage ou un véhicule, tu ne modèles qu'un côté.

### L'édition proportionnelle

```etape
Clique **Proportional editing**, ou appuie sur O. Déplace un sommet avec G : ses voisins suivent, de moins en moins.
```

```etape
Pendant le déplacement, tourne la molette. Le rayon d'influence change.
```

C'est l'outil des formes douces : une colline, une joue, un pli.

### L'aimantation

**Snap to vertices**, ou Maj+Tab : pendant un G, le sommet choisi le plus proche se pose sur le sommet visé. Pendant tout déplacement, Ctrl aimante par crans.

```etape
Éteins **X mirror** et **Proportional editing**. Sors avec Tab.
```

## Ce qui n'existe pas encore

- Pas de sélection au lasso ni au cercle.
- L'aimantation ne se fait que sur les sommets.
- Le miroir d'édition n'existe que sur X.

```quiz
Tu veux retirer une boucle d'arêtes sans faire de trou. Que choisis-tu ?
- [ ] Delete edges
- [x] Dissolve edges
- [ ] Rip
explication: Dissoudre retire l'élément et garde la surface fermée. Supprimer laisse un trou.
```

```quiz
Quel outil ajoute une boucle d'arêtes tout autour du maillage ?
- [x] Loop cut, avec Ctrl+R.
- [ ] Knife, avec K.
- [ ] Fill, avec F.
explication: La coupe en boucle fait le tour. Le couteau coupe à main levée, face par face.
```

```defi
Perce une fenêtre qui traverse le cube de part en part.
indice: Inset sur deux faces opposées, supprime les deux faces intérieures, choisis les deux boucles de bord, puis Bridge edge loops.
```

## En trois lignes

- Ctrl+R ajoute une boucle, K coupe librement.
- M fusionne. Dissoudre retire sans trouer. F rebouche.
- **X mirror** et **Proportional editing** font gagner la moitié du travail.

[Suite : Les modificateurs](04-les-modificateurs.md)
