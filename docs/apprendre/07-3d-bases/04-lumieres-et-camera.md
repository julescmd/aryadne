---
titre: Lumières et caméra
niveau: débutant
duree: 15 min
prerequis: 07-3d-bases/03-ajouter-et-organiser
objectifs: Distinguer les quatre lumières ; Régler force et couleur ; Regarder par la caméra ; Enregistrer un cadrage ; Faire viser un objet par la caméra
---

## Ce que tu vas réussir

Tu vas éclairer un cube avec trois lumières, comme en studio. Puis tu vas poser une caméra, choisir ton cadre et le garder.

```essayer
nouveau: aryscene
nom: lumieres-et-camera
```

## Qui éclaire la scène

Sans lumière dans la scène, c'est la lumière de la vue qui éclaire. Dès qu'une scène contient ses propres lumières, ce sont elles qui éclairent.

La scène de départ a déjà une lumière.

```etape
Choisis la lumière dans le panneau **Scene**. Dans **Properties**, ouvre l'onglet **Light**.
verifier: scene.selection
```

## Les quatre lumières

| Lumière | Comportement | Comment la régler |
|---|---|---|
| **Sun light** | Une direction, partout la même | Tourne-la |
| **Point light** | Éclaire tout autour d'elle | Déplace-la |
| **Spot** | Un cône | **Angle °**, **Falloff** |
| **Area light** | Un panneau lumineux, ombres douces | **Scale** : largeur et hauteur |

L'onglet **Light** règle **Strength** et la forme. Une lumière ponctuelle a aussi **Range**. La couleur se règle dans l'onglet **Object**.

## Un éclairage à trois points

### La lumière principale

```etape
Ajoute une **Area light** par le menu **Add**. Place-la en haut à gauche du cube, avec G.
verifier: scene.objets >= 4
```

```etape
Tourne-la vers le cube avec R. Monte **Strength** jusqu'à bien éclairer la face avant.
```

### La lumière de débouchage

Elle adoucit les ombres du côté opposé.

```etape
Duplique la lumière avec Maj+D. Amène la copie à droite du cube. Baisse sa **Strength** de moitié.
verifier: scene.objets >= 5
```

```etape
Dans l'onglet **Object**, donne-lui une couleur légèrement bleue.
```

### Le contre-jour

Il détache le sujet du fond.

```etape
Ajoute une **Spot**. Place-la derrière le cube, en hauteur, tournée vers lui.
verifier: scene.objets >= 6
```

```etape
Règle **Angle °** pour que le cône couvre juste le cube.
```

```astuce
Pour qu'une lumière vise toujours le sujet, donne-lui la contrainte **Aim at**, dans l'onglet **Object**, avec le cube pour cible. Tu la déplaces, elle se tourne toute seule.
```

## Les ombres

```etape
Dans **Properties**, ouvre l'onglet **World**. Coche **Cast shadows**.
```

```etape
Monte **Softness**. Le bord des ombres s'élargit en pénombre.
```

## La caméra

Une caméra est un point de vue enregistré.

```etape
Appuie sur 0. Tu regardes par la caméra de la scène.
```

Le cadre est rarement le bon du premier coup. Le plus simple : trouver la vue qui te plaît, puis la donner à la caméra.

```etape
Tourne la vue jusqu'à voir le cube de trois quarts, légèrement de haut.
```

```etape
Choisis la caméra dans le panneau **Scene**. Ouvre l'onglet **Camera**. Clique **Set from view**.
```

```etape
Tourne la vue au hasard, puis appuie sur 0. Tu retrouves ton cadre.
```

| Réglage | Effet |
|---|---|
| **Look through** | Aligne la vue sur cette caméra |
| **Set from view** | Enregistre la vue actuelle dans cette caméra |
| **Field of view** | Le champ de vision. Petit : téléobjectif. Grand : grand-angle |

```etape
Baisse **Field of view**, puis recule la caméra pour garder le cube à la même taille. La perspective s'aplatit.
```

## Une caméra qui suit son sujet

```etape
Choisis la caméra. Dans l'onglet **Object**, ajoute la contrainte **Aim at** et choisis le cube comme cible.
```

```etape
Déplace la caméra avec G. Elle garde le cube au centre.
```

Avec cette contrainte, la rotation X et Y de la caméra ne se règle plus à la main : la cible décide.

## Enregistre

```etape
Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```quiz
Quelle lumière donne les ombres les plus douces ?
- [ ] Point light
- [ ] Sun light
- [x] Area light
explication: Une Area light est un panneau. Plus il est grand, plus l'ombre est douce.
```

```quiz
Tu as trouvé le cadre parfait en tournant la vue. Comment le donner à la caméra ?
- [ ] Appuyer sur 0.
- [x] Choisir la caméra, puis Set from view.
- [ ] Enregistrer la scène.
explication: 0 regarde par la caméra telle qu'elle est. Set from view enregistre la vue actuelle dans la caméra.
```

```defi
Fais un éclairage dramatique : une seule lumière rasante, de côté, et une ombre dure.
indice: Cache deux lumières avec H. Garde une Spot basse sur le côté. Softness à 0.
```

## En trois lignes

- Quatre lumières : **Sun light**, **Point light**, **Spot**, **Area light**.
- 0 regarde par la caméra. **Set from view** lui donne ton cadre.
- **Aim at** fait viser une cible à une lumière ou à une caméra.

[Suite : L'affichage de la vue](05-affichage-de-la-vue.md)
