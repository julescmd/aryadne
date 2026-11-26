---
titre: Animation image par image
niveau: intermédiaire
duree: 20 min
prerequis: 03-2d/02-calques
objectifs: Passer une image en animation ; Dessiner des images avec la pelure d'oignon ; Régler la cadence et la tenue des dessins ; Colorier d'après l'image d'avant ; Exporter en MP4 ou en GIF
---

## Ce que tu vas réussir à faire

Tu vas animer une balle qui rebondit, dessin après dessin, la voir jouer en boucle, puis en sortir un GIF.

Le principe : chaque dessin est un calque. Les calques d'un groupe « Animation » deviennent les images du film.

```essayer
nouveau: aryimg
nom: balle
```

```etape
Clique **Try it yourself**.
verifier: fichier.ouvert = aryimg
```

## Passer en animation

```etape
Ouvre **Layer ▾ › Frame-by-frame animation**. Un panneau d'animation s'ouvre en bas, avec une bande d'images.
verifier: calques >= 2
```

Aryadne a créé un groupe « Animation » et son premier dessin. Le calque de fond reste en dehors du groupe.

Le panneau d'animation, de gauche à droite :

| Élément | Effet |
|---|---|
| Lecture | **Play the animation** |
| `+` | **New empty frame after this one** : une image vide après celle-ci |
| Copie | **Copy this frame after it** : duplique l'image |
| Corbeille | Supprime l'image |
| Rouleau | Colorie d'après l'image d'avant |
| La cadence | En images par seconde |
| Les calques empilés | **Onion skin** : la pelure d'oignon |
| **On** 1, 2, 3 | La tenue de tous les dessins |
| **MP4…**, **GIF…** | L'export |
| La bande | Une vignette par image |

## Le premier dessin

```etape
Prends le pinceau avec `B`. Dessine une balle en haut à gauche de l'image.
```

## Le deuxième dessin, avec la pelure d'oignon

```etape
Clique `+` : **New empty frame after this one**. L'image est vide.
verifier: calques >= 3
```

La balle de l'image d'avant apparaît en rouge, en transparence : c'est la pelure d'oignon, **Onion skin**. Elle est active au départ. Elle montre l'image d'avant en rouge et celle d'après en vert. Tu places ton dessin par rapport à elles.

```etape
Clique le bouton **Onion skin** pour la couper, puis encore pour la remettre.
```

```etape
Dessine la balle un peu plus bas et un peu plus à droite.
```

## Six dessins pour un rebond

```etape
Ajoute encore quatre images. Fais descendre la balle jusqu'au sol, écrase-la un peu au contact, puis fais-la remonter.
verifier: calques >= 7
```

```raccourci
`,` va à l'image précédente. `.` va à l'image suivante.
```

```astuce
Rapproche les dessins en haut du rebond, écarte-les près du sol. La balle ralentit en haut et accélère en tombant.
```

## Lire

```etape
Clique le bouton de lecture du panneau. L'animation tourne en boucle. Clique encore pour arrêter.
```

## La cadence et la tenue

Deux réglages donnent le rythme.

- La **cadence** : le nombre d'images par seconde du film. Elle est à 12 au départ. 24 est la cadence du cinéma.
- La **tenue** : pendant combien d'images chaque dessin reste à l'écran.

Animer « en 2 », c'est tenir chaque dessin 2 images. À 24 images par seconde, tu dessines alors 12 dessins par seconde. C'est le rythme le plus courant en animation dessinée.

```etape
À côté de **On**, clique `2`. Relance la lecture : la balle va deux fois moins vite.
```

```etape
Essaie `3`, puis reviens à `2`.
```

Les boutons **On** règlent tous les dessins d'un coup. Tu peux aussi régler un dessin seul.

```etape
Fais un clic droit sur la vignette du dessin où la balle touche le sol. Choisis **Held 4** : ce dessin reste plus longtemps à l'écran.
```

Le compteur du panneau affiche le dessin courant, le nombre de dessins, le total d'images et la durée en secondes.

```attention
Dupliquer un dessin et le tenir plus longtemps donnent le même résultat à l'écran. Préfère la tenue : tu n'as qu'un seul dessin à corriger.
```

## Colorier d'après l'image d'avant

Colorier chaque image à la main est long. Aryadne peut reprendre les couleurs de l'image précédente.

```etape
Va à la première image avec `,`. Remplis la balle au pot de peinture (`G`).
```

```etape
Va à l'image suivante avec `.`. Clique le rouleau du panneau d'animation. La balle prend la couleur de l'image d'avant.
```

Chaque zone fermée du trait reprend la couleur de la même zone sur l'image précédente.

```astuce
Rien n'est repris ? Ton trait n'est pas fermé. Monte **Close gaps (px)** dans les options du pot de peinture, ou referme le trait au pinceau.
```

## Un décor fixe

Seuls les calques du groupe « Animation » sont des images. Un calque hors du groupe reste à l'écran pendant tout le film.

```etape
Dans le panneau **Layers**, choisis le calque de fond, hors du groupe. Dessine une ligne de sol. Relance la lecture : le sol ne bouge pas.
```

## Exporter

```etape
Clique **GIF…** dans le panneau d'animation. Enregistre le fichier.
```

**MP4…** exporte une vidéo. Les exports vidéo passent par ffmpeg. S'il manque, Aryadne propose de l'installer dans ton dossier utilisateur, sans droits administrateur.

```etape
Enregistre ton image avec `Ctrl+S`. Les dessins, la cadence et les tenues sont gardés dans le fichier.
```

```quiz
Tu animes « en 2 » à 24 images par seconde. Combien de dessins pour une seconde ?
- [ ] 24
- [x] 12
- [ ] 48
explication: Chaque dessin est tenu 2 images. 24 divisé par 2 donne 12 dessins.
```

```quiz
Que montre la pelure d'oignon ?
- [x] L'image d'avant en rouge et celle d'après en vert.
- [ ] Tous les calques du fichier.
- [ ] Le masque du calque.
explication: Elle sert à placer le dessin courant par rapport à ses voisins.
```

```quiz
Comment faire durer un seul dessin plus longtemps ?
- [ ] En baissant la cadence.
- [x] Par un clic droit sur sa vignette, puis une tenue plus longue.
- [ ] En augmentant son opacité.
explication: La cadence change tout le film. La tenue d'un dessin ne change que lui.
```

```defi
Fais rebondir la balle deux fois, le deuxième rebond plus bas que le premier, et exporte en MP4.
indice: Copie tes images avec **Copy this frame after it**, puis redessine la balle plus bas. Tiens plus longtemps le dessin du haut de chaque rebond.
```

## Récapitulatif

- **Layer ▾ › Frame-by-frame animation** : chaque calque du groupe « Animation » est une image.
- La cadence règle le film, la tenue règle chaque dessin. `,` et `.` changent d'image.
- **GIF…** et **MP4…** exportent l'animation.

Parcours suivant : [Dessin vectoriel](../04-vectoriel/index.md)
