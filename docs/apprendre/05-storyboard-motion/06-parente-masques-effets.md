---
titre: Parenté, caches, masques et effets
niveau: intermédiaire
duree: 15 min
prerequis: 05-storyboard-motion/04-premiere-composition
objectifs: Piloter plusieurs calques avec un parent ; Révéler un calque à travers un autre ; Masquer un calque ; Ajouter effets, répétiteur et tremblement ; Plier une image avec la marionnette
---

## Ce que tu vas réussir à faire

Tu vas animer un petit système : une planète, sa lune qui la suit, un texte révélé par une forme, et un fond qui ondule. Chaque outil de cette leçon t'évite de poser des clés à la main.

```essayer
nouveau: arymotion
nom: systeme
```

```etape
Clique **Try it yourself**. Supprime les deux calques de départ : choisis-les dans la timeline, puis menu **Layer**, **Delete**. Trace un grand cercle au centre avec `E` : la planète. Trace un petit cercle à côté : la lune.
verifier: onglet = motion
```

## La parenté

Un calque peut suivre un autre calque : son **Parent**. Déplacer, tourner ou agrandir le parent emporte l'enfant.

```etape
Choisis la lune. Dans l'onglet **Properties**, section **Layer**, règle **Parent** sur le calque de la planète.
```

```etape
Choisis la planète. Pose une clé de **Rotation** au début. Avance de deux secondes, tourne la planète d'un tour, la clé se pose. Relis : la lune tourne autour.
```

```astuce
Pour un mouvement d'ensemble, crée un objet nul avec le bouton d'ajout du panneau **Project (motion)**. Il est invisible au rendu. Donne-le comme parent à plusieurs calques, et anime-le lui seul.
```

## Le cache de piste

Le **Matte** fait apparaître un calque à travers le calque juste au-dessus. Ce calque du dessus ne se dessine plus : il sert de pochoir.

```etape
Pose un texte avec `T`. Trace un rectangle par-dessus avec `Q`. Le rectangle doit être juste au-dessus du texte dans la timeline.
```

```etape
Choisis le texte. Dans la section **Layer**, règle **Matte** sur le mode alpha. Le texte ne se voit plus que là où est le rectangle.
```

```etape
Anime la **Location** du rectangle pour qu'il traverse le texte. Le texte se révèle au passage.
```

| Mode de **Matte** | Le calque apparaît |
|---|---|
| Alpha | Là où le calque du dessus est opaque |
| **Alpha inverted** | Là où il est transparent |
| **Luma** | Là où il est clair |
| **Luma inverted** | Là où il est sombre |

## Les masques

Un masque limite un calque à une forme. Il appartient au calque, sans calque en plus.

```etape
Choisis la planète. Dans la section **Masks**, clique **New elliptical mask**. Glisse ses points dans la vue pour ne garder qu'un croissant.
```

| Réglage | Effet |
|---|---|
| **Add**, **Subtract**, **Intersect** | Comment le masque se combine avec les autres |
| **Invert mask** | Garde l'extérieur au lieu de l'intérieur |
| **Smooth outline** | Arrondit le contour |
| **Feather** | Un contour progressif |
| **Mask opacity** | La force du masque |

```etape
Monte **Feather** : le bord du croissant devient doux.
```

Un masque s'anime : pose une clé de masque, puis déplace ses points à un autre instant.

## Le style d'un calque

La section **Style** ajoute un **Stroke**, une **Drop shadow** et un **Glow**.

```etape
Choisis la lune. Active **Glow**. Règle **Radius** et **Strength**.
```

## Les effets

```etape
Trace un grand rectangle : le fond. Descends-le en bas de la pile avec **Layer › Move down**, autant de fois qu'il faut.
```

```etape
Choisis le fond. Dans la section **Effects**, clique **Add an effect**, puis **Wave**.
```

| Effet | Réglages |
|---|---|
| **Wave** | **Amplitude**, **Wavelength**, **Speed**, **Vertical wave** |
| **Turbulence** | **Amount**, **Eddy size**, **Evolution** |
| **Offset** | **x per second**, **y per second** : l'image défile |
| **Puppet** | Des épingles posées sur le calque |

Il y a quatre effets. Il n'y a pas de greffons.

```astuce
Le calque **Fractal noise** fait un bon fond vivant : fumée, nuages, textures qui bougent. Ajoute-le depuis le panneau **Project (motion)**, puis règle **Pattern size**, **Evolution** et **Contrast**.
```

## Le répétiteur

Le **Repeater** fait des copies du calque. Chaque copie est décalée, tournée et réduite par rapport à la précédente.

```etape
Choisis la lune. Dans **Repeater**, monte **Copies** à 6. Donne une **Rotation** de 60° par copie.
```

**End opacity** fait s'estomper les dernières copies : une traînée.

## Bouger tout seul

La section **Path** contient deux automatismes.

- **Auto-orient** : le calque tourne pour suivre la direction de son mouvement. Une voiture sur une route.
- **Wiggle on its own** : de petits mouvements doux au hasard, en plus de l'animation. Une caméra à l'épaule, un objet qui flotte.

```etape
Choisis le texte. Active **Wiggle on its own**. Règle les mouvements par seconde et l'amplitude en **Position**. Relis.
```

**New random seed** change le tirage.

## La marionnette

L'effet **Puppet** plie une image autour d'épingles.

1. Ajoute l'effet **Puppet** au calque.
2. Clique **Place pins**, puis clique sur le calque dans la vue pour poser les épingles.
3. Tire une épingle : l'image se plie.

Glisser une épingle pose une clé à l'instant courant. `Alt+clic` sur une épingle la retire. **Reset pins to rest** remet tout à plat.

```etape
Ajoute **Puppet** à un calque. Pose trois épingles. Tire celle du milieu, à deux instants différents. Relis.
```

## Un bras qui se plie tout seul

La section **Rig** propose **Two-bone IK**. Trois calques reliés par la parenté forment un bras : épaule, coude, main. Un objet nul sert de contrôleur. Tu tires le contrôleur, l'épaule et le coude tournent tout seuls. **Bend the other way** inverse le pli.

```quiz
La lune a la planète pour parent. Tu tournes la planète. Que fait la lune ?
- [x] Elle tourne autour de la planète.
- [ ] Elle reste immobile.
- [ ] Elle disparaît.
explication: L'enfant hérite de la position, de la rotation et de la taille de son parent.
```

```quiz
Quelle différence entre un Matte et un masque ?
- [x] Le Matte se sert du calque juste au-dessus comme pochoir. Le masque est une forme propre au calque.
- [ ] Le masque se sert d'un autre calque. Le Matte est une forme du calque.
- [ ] Aucune.
explication: Le Matte demande un deuxième calque. Le masque vit dans les réglages du calque.
```

```defi
Fais une comète : un petit cercle qui traverse l'écran, avec une traînée qui s'estompe.
indice: **Repeater** avec plusieurs copies décalées vers l'arrière et **End opacity** à zéro. Active **Auto-orient** si la trajectoire est courbe.
```

## Récapitulatif

- **Parent** fait suivre un calque. Un objet nul pilote un groupe.
- **Matte** révèle à travers le calque du dessus. Les masques découpent un calque.
- **Effects**, **Repeater** et **Wiggle on its own** animent sans poser de clés.

Leçon suivante : [Caméra 2,5D, expressions et export](07-camera-expressions-export.md)
