---
titre: La timeline
niveau: débutant
duree: 15 min
prerequis: 07-3d-bases/02-selectionner-et-transformer
objectifs: Régler la durée d'une scène ; Poser des clés ; Lire l'animation ; Choisir un lissage ; Utiliser la clé automatique
---

## Ce que tu vas réussir

Tu vas faire traverser l'écran à un cube, puis le faire rebondir à l'arrivée. Ta première animation, en quatre clés.

```essayer
nouveau: aryscene
nom: premiere-animation
```

## L'idée

Une **clé** enregistre la pose complète d'un objet à une image donnée : sa position, sa rotation, sa taille. Entre deux clés, Aryadne calcule le passage.

Tu donnes les moments importants. Le logiciel fait le reste.

## La durée de la scène

```etape
Dans **Properties**, ouvre l'onglet **Scene**. Lis **fps**, **Start** et **End**.
```

**fps** est le nombre d'images par seconde : 24 par défaut. À 24 images par seconde, 48 images font deux secondes.

```etape
Mets **End** à 48.
```

## La timeline 3D

Le panneau **3D timeline** est sous la vue. Il porte les boutons de lecture, l'image courante dans **Frame**, et les clés de l'objet choisi.

## Première clé

```etape
Choisis le cube. Vérifie que **Frame** vaut 1.
verifier: scene.selection
```

```etape
Déplace le cube à gauche, avec G puis X.
```

```etape
Appuie sur I dans la vue. Un losange apparaît dans la timeline : la clé est posée.
```

```raccourci
I dans la vue, ou K : poser une clé pour l'objet choisi à l'image courante.
```

## Deuxième clé

```etape
Va à l'image 48 : tape 48 dans **Frame**, ou appuie sur Fin.
verifier: lecture.image >= 48
```

```etape
Déplace le cube à droite. Appuie sur I.
```

## Lis

```etape
Appuie sur Origine pour revenir au début, puis sur Espace. Le cube traverse la vue.
```

```etape
Arrête avec Espace. Avance image par image avec les flèches gauche et droite.
```

| Geste | Effet |
|---|---|
| Espace | Lire, arrêter |
| ← → | Une image |
| Maj+← Maj+→ | Clé précédente, clé suivante |
| Origine, Fin | Début, fin |
| Ctrl+molette | Zoomer dans le temps |
| F | Tout voir |

## Le lissage

Le cube part doucement et s'arrête doucement. C'est le lissage **Soft**, celui par défaut. Le lissage d'une clé décrit le passage **vers la clé suivante**.

```etape
Dans la timeline, fais un clic droit sur la première clé. Choisis **Linear**. Relis : la vitesse est constante, le mouvement mécanique.
```

```etape
Choisis maintenant **Bounce**. Relis : le cube rebondit à l'arrivée.
```

| Lissage | Mouvement |
|---|---|
| **Soft** | Départ et arrivée en douceur |
| **Linear** | Vitesse constante |
| **Ease in**, **Ease out** | Départ, ou arrivée, en douceur |
| **Bounce** | Rebondit à l'arrivée |
| **Spring** | Dépasse puis revient |
| **Hold** | La valeur tient jusqu'à la clé suivante |

La forme du losange change avec le lissage : elle te le rappelle d'un coup d'œil.

## Une clé au milieu

```etape
Va à l'image 24. Monte le cube avec G puis Y. Appuie sur I.
verifier: lecture.image >= 24
```

```etape
Relis depuis le début. Le cube fait un saut en traversant.
```

## La clé automatique

Appuyer sur I à chaque fois lasse vite.

```etape
Dans la timeline, clique le point rouge : la clé automatique. Chaque déplacement pose maintenant sa clé.
```

```etape
Va à l'image 36. Tourne le cube avec R. Aucun I : la clé est déjà là.
verifier: lecture.image >= 36
```

```attention
Éteins la clé automatique quand tu as fini d'animer. Sinon, le moindre déplacement d'essai devient une clé.
```

## Retirer une clé

Survole une clé dans la timeline et appuie sur Suppr. Le bouton en croix de la timeline retire la clé de l'image courante.

## Voir avant et après

```etape
Clique le bouton de pelure d'oignon de la timeline. Le cube apparaît en fantôme aux clés d'avant, en rouge, et d'après, en vert.
```

```quiz
Que contient une clé ?
- [ ] Seulement la position.
- [x] La pose complète : position, rotation et taille.
- [ ] L'image rendue.
explication: Une clé enregistre toujours la pose entière. Chaque canal se règle ensuite dans l'éditeur de courbes.
```

```quiz
Le lissage d'une clé règle quel passage ?
- [ ] Celui qui arrive à cette clé.
- [x] Celui qui va de cette clé à la suivante.
- [ ] Toute l'animation.
explication: Le lissage décrit le trajet vers la clé suivante. La forme du losange le rappelle.
```

```defi
Fais faire trois petits sauts au cube pendant sa traversée, sans toucher à la clé automatique.
indice: Des clés hautes aux images 8, 24, 40 et des clés basses entre elles. Maj+flèches saute de clé en clé.
```

## En trois lignes

- Une clé enregistre une pose à une image. I la pose.
- Le lissage d'une clé règle le trajet vers la suivante.
- Le point rouge pose les clés tout seul. Pense à l'éteindre.

Parcours terminé. Pour changer la forme de tes objets, passe à la [Modélisation](../08-modelisation/index.md). Pour aller plus loin en animation : [Animation et rendu](../15-animation-rendu/index.md).
