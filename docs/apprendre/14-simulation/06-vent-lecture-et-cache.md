---
titre: Vent, lecture et cache
niveau: intermédiaire
duree: 20 min
prerequis: 14-simulation/05-obstacles-et-auto-collision
objectifs: Faire souffler le vent ; Régler la durée et la cadence ; Comprendre ce qui vide le cache ; Figer une simulation sur le disque ; Mesurer sa carte graphique
---

## Ce que tu vas réussir

Tu vas faire souffler le vent sur un tissu accroché. Tu vas allonger la simulation. Puis tu vas la figer sur le disque pour la relire sans calcul.

```essayer
exemple: tissu sur une boule
```

## Le vent

Le vent se règle dans le node **Forces**. C'est un vecteur : une direction et une force à la fois.

```etape
Sur **Forces**, mets **Wind** à (6, 0, 0). Reviens à la première image et lis.
verifier: lecture.image >= 40
```

Le tissu est poussé vers les X positifs. Il s'enroule de côté sur la boule.

```etape
Mets **Air drag** à 0,3. Relis. Le tissu se balance moins et se stabilise plus vite.
```

**Air drag** est le frottement de l'air. Il freine la vitesse de tous les points, à chaque image.

```astuce
**Wind** accepte un champ : une valeur par point. Un vent relié à un node **Noise** ne pousse pas tous les points pareil.
```

## Un vent qui change dans le temps

```etape
Ajoute un node **Time**, puis un node **Expression**. Relie **Seconds** à l'entrée **a**. Écris la formule : `sin(a * 3) * 6`
verifier: graphe.contient = expr
```

```etape
Ajoute un node **Combine XYZ**. Tape 6 dans **X**. Relie **Expression** à **Z**. Relie sa sortie à **Wind**.
verifier: graphe.contient = combxyz
```

```etape
Relis depuis la première image. Le vent tourne d'avant en arrière, et le tissu bat.
verifier: lecture.image >= 60
```

## La durée et la cadence

```etape
Dans la barre de lecture, clique la roue dentée : **Length, frame rate and cache**.
```

Le menu propose **First frame**, **Last frame** et **Frames per second**. Dessous, la taille du **Cache in memory** et le bouton **Clear the cache**.

```etape
Mets **Last frame** à 240. La simulation dure maintenant dix secondes à 24 images par seconde.
```

## La barre de cache

Sous la barre de lecture, une bande fine. Chaque image déjà calculée y est teintée.

```etape
Lis jusqu'à l'image 100. Puis clique au milieu de la bande teintée. L'image s'affiche tout de suite : elle vient du cache.
verifier: lecture.image >= 50
```

- Revenir en arrière relit le cache.
- Aller à une image lointaine recalcule depuis le dernier état connu.
- Pendant un calcul long, « Simulating… » s'affiche. L'interface reste utilisable.

## Ce qui vide le cache

```etape
Change la couleur du node **Set an attribute** placé après **Simulation end**. La bande reste teintée.
```

Ce node est **en aval** de la zone. La simulation n'en dépend pas.

```etape
Change maintenant **Wind** dans **Forces**. La bande se vide.
```

Ce réglage est dans la zone. Les images calculées ne valent plus. La règle : un changement **en amont** ou **dans** la zone vide ses images. Un changement **en aval** ne coûte rien.

```astuce
Règle d'abord la simulation. Garde les couleurs et les retouches d'après la zone pour la fin : elles ne font rien recalculer.
```

## Figer sur le disque

Le cache en mémoire a un budget : 512 Mo par défaut. Il se vide aussi à la fermeture. Pour garder une simulation, fige-la.

```etape
Enregistre le fichier avec Ctrl+S.
verifier: fichier.enregistre
```

```etape
Clique le node **Simulation end**. Dans le panneau de côté, trouve la carte **SIMULATION CACHE**.
```

```etape
Clique **Bake to disk**. Toutes les images sont calculées et écrites dans le coffre.
```

La carte indique maintenant « Baked: frames come from disk. ». La bande de cache change de couleur sur toute sa longueur. La zone se relit sans calcul, même si tu changes l'amont.

```etape
Change **Wind**. Rien ne bouge : la zone est figée.
```

```etape
Clique **Release the cache**. La zone se recalcule de nouveau d'après ses réglages. Les fichiers sont supprimés.
```

Les images vont dans le dossier `.cache`, à la racine du coffre, sous le nom du graphe.

## Ta carte suit-elle ?

Aryadne peut mesurer ce que ta carte graphique fait vraiment.

```etape
Ouvre le menu **Help**, puis **Measure the graphics card**. Attends quelques secondes : le rapport s'affiche.
```

Regarde trois choses.

- **Software rendering (no real graphics card)** : aucune vraie carte n'a été trouvée. Vérifie tes pilotes.
- **At 24 frames per second**, **Passes per frame** : si le tissu d'un million de sommets fait plus de 32 passes, il se simule en temps réel chez toi.
- La ligne avec **self-collision** : ce que l'auto-collision coûte par sous-pas.

**Copy the report** met le rapport dans le presse-papiers.

```astuce
Ta carte est trop lente pour ton tissu ? Réduis les points de la grille pour régler, puis remonte-les et fige sur le disque.
```

```quiz
Tu changes la couleur du tissu après Simulation end. Que devient le cache ?
- [x] Il reste : ce node est en aval de la zone.
- [ ] Il se vide en entier.
- [ ] Il se fige sur le disque.
explication: Un node en aval ne change pas la simulation. Seul l'amont ou l'intérieur de la zone vide ses images.
```

```quiz
À quoi sert Bake to disk ?
- [ ] À exporter le tissu vers une scène 3D.
- [x] À écrire toutes les images dans le coffre pour les relire sans calcul.
- [ ] À accélérer le solveur.
explication: La zone figée se relit du disque. Release the cache la rend de nouveau calculable.
```

```defi
Fais souffler un vent qui monte en force pendant deux secondes, puis reste constant.
indice: Dans Expression, la fonction clamp borne une valeur. Essaie `clamp(a / 2, 0, 1) * 8`.
```

## En trois lignes

- **Wind** et **Air drag** vivent dans **Forces**. Le vent peut être un champ ou une formule.
- Un changement dans la zone ou en amont vide le cache. En aval, non.
- **Bake to disk** garde la simulation. **Release the cache** la libère.

[Suite : Particules et vagues](07-particules-et-vagues.md)
