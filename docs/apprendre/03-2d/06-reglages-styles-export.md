---
titre: Réglages, styles de calque et export
niveau: intermédiaire
duree: 15 min
prerequis: 03-2d/04-masques
objectifs: Corriger les couleurs avec un calque de réglage ; Appliquer un filtre ; Donner une ombre et un contour à un calque ; Exporter en PNG, JPG, WebP ou PSD ; Envoyer l'image à un autre outil
---

## Ce que tu vas réussir à faire

Tu vas corriger les couleurs d'une photo sans toucher à ses pixels, donner une ombre portée à un titre, puis exporter le tout.

```essayer
nouveau: aryimg
nom: reglages
```

```etape
Clique **Essaie par toi-même**. Ajoute une photo avec **Layer ▾ › Layer from an image…**.
verifier: calques >= 2
```

## Deux façons de régler

- Un **réglage direct** change les pixels du calque. C'est rapide, mais c'est fait.
- Un **calque de réglage** ne contient aucun pixel. Il change la couleur de tout ce qui est dessous. Tu peux le modifier, le cacher ou le supprimer plus tard.

Préfère le calque de réglage dès que tu veux pouvoir revenir en arrière.

## Un calque de réglage

```etape
En bas du panneau **Layers**, clique le bouton **New adjustment layer**. Choisis le réglage de teinte et de saturation.
verifier: calques >= 3
```

```etape
Le calque de réglage est choisi. Bouge ses curseurs **Hue** et **Saturation** : toute l'image change.
```

```etape
Clique l'œil du calque de réglage. L'image d'origine revient, intacte. Remets l'œil.
```

| Réglage | Curseurs |
|---|---|
| Luminosité et contraste | **Brightness**, **Contrast** |
| Niveaux | **Blacks**, **Gamma**, **Whites** |
| Exposition | En EV |
| Teinte et saturation | **Hue**, **Saturation**, **Brightness** |
| Noir et blanc, inverser, seuil | |
| **Posterize** | Le nombre de niveaux |
| Grain | **Amount**, **Grain size** |
| **Clarity** | Le contraste des tons moyens |
| **Dehaze** | Retire la brume claire qui délave l'image |
| Vibrance | Sature surtout les couleurs ternes |

```astuce
Un calque de réglage naît avec un masque blanc. Quand il est choisi, tu peins dans ce masque. Peins en noir : le réglage ne s'applique plus à cet endroit.
```

## Réglages directs et filtres

Ils agissent sur le calque actif, ou sur la sélection s'il y en a une.

- Menu **Image ▾** : **Brightness and contrast…**, **Hue and saturation…**, **Invert colors** (`Ctrl+I`), **Black and white**.
- Menu **Filters ▾** : **Light blur**, **Strong blur**, **Sharpness**, **Noise**, **Pixelate**.

```etape
Choisis le calque de la photo. Sélectionne un rectangle avec `M`. Ouvre **Filters ▾ › Pixelate** : seule la zone est pixelisée. Annule avec `Ctrl+Z`, puis désélectionne avec `Ctrl+D`.
```

## Les styles de calque

Un style est un effet attaché au calque. Il suit le contenu : si tu repeins, l'effet se met à jour.

```etape
Crée un calque avec le bouton **New Layer**. Écris ou dessine un mot dessus, en gros, avec une couleur claire.
verifier: calques >= 4
```

```etape
Clic droit sur ce calque, puis **Blending Options (styles)…**. La fenêtre **Layer Style** s'ouvre.
```

| Style | Effet |
|---|---|
| **Drop shadow** | Une ombre derrière le calque |
| **Inner Shadow** | Une ombre à l'intérieur : un creux |
| **Outer Glow** | Un halo autour |
| **Bevel and Emboss** | Un relief |
| **Color Overlay** | Une couleur par-dessus le contenu |
| **Gradient Overlay** | Un dégradé par-dessus le contenu |
| **Stroke** | Un contour |

```etape
Coche **Drop shadow** et règle sa taille. Coche **Stroke** et choisis une couleur sombre.
```

```etape
Ferme la fenêtre. Ajoute un trait sur le calque : il prend l'ombre et le contour tout de suite.
```

## Opacity ou Fill

**Opacity** rend tout transparent, effets compris. **Fill** ne rend transparent que le contenu. Les styles restent.

```etape
Baisse **Fill** du calque à 0 %. Le mot disparaît, mais son contour et son ombre restent.
```

## Blend if

**Blend if** montre un calque seulement sur certains tons du dessous, d'après leur luminosité. Sans masque.

```etape
Sur le calque du mot, coche **Blend if**. Bouge **From tones** et **Up to tones** : le calque ne se voit plus que sur les zones claires, ou sombres, de la photo.
```

## Exporter

```etape
Ouvre **Image ▾ › Export as PNG, JPG, WebP or PSD…**.
```

| Format | Quand le choisir |
|---|---|
| PNG | Sans perte, avec la transparence |
| JPG | Une photo légère, sans transparence |
| WebP | Pour le web |
| PSD | Pour continuer dans Photoshop, calques compris |

```attention
Le PSD est écrit en 8 bits RVB. Ce qui ne passe pas encore est listé à l'export : « Not carried to the PSD yet ». Lis cette liste avant d'envoyer le fichier.
```

## Envoyer à un autre outil

Le menu **Image ▾** contient **Send the image to** : un board, le montage, la 3D ou les nodes.

Tu n'es pas obligé d'exporter pour réutiliser ton image. Un `.aryimg` s'utilise directement dans le montage, le board, la 3D et les nodes, comme une image ordinaire.

```quiz
Quel est l'avantage d'un calque de réglage ?
- [x] Il ne change aucun pixel : tu peux le modifier ou le retirer plus tard.
- [ ] Il est plus rapide à calculer.
- [ ] Il fusionne les calques du dessous.
explication: Le réglage vit dans son propre calque. L'image du dessous reste intacte.
```

```quiz
Tu baisses Fill à 0 % sur un calque qui a un contour. Que vois-tu ?
- [ ] Rien.
- [x] Le contour seul.
- [ ] Le calque entier, inchangé.
explication: Fill ne touche que le contenu. Les styles de calque restent visibles.
```

```defi
Fais une vignette de miniature : une photo assombrie par un calque de réglage, et un titre avec contour et ombre portée.
indice: Recadre d'abord en 16:9 avec `C`. Pose le calque de réglage juste au-dessus de la photo, sous le titre.
```

## Récapitulatif

- Un calque de réglage corrige les couleurs sans rien détruire.
- **Blending Options (styles)…** ajoute ombre, lueur, biseau, incrustation et contour.
- **Image ▾ › Export…** sort un PNG, un JPG, un WebP ou un PSD.

Leçon suivante : [Animation image par image](07-animation-image-par-image.md)
