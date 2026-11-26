---
titre: Filtres et masques
niveau: intermédiaire
duree: 15 min
prerequis: 13-compositing/01-premier-graphe-d-image
objectifs: Enchaîner halo, vignette et grain ; Dessiner un masque de forme ; Doser un flou par un masque ; Comprendre l'ordre des filtres
---

## Ce que tu vas réussir

Tu vas donner à une image un fini de cinéma : un halo dans les lumières, des coins sombres, du grain. Puis tu flouteras tout, sauf ton sujet.

```essayer
nouveau: arycomp
nom: filtres-et-masques
```

```etape
Fais entrer une image et relie-la à **Output**. À défaut, pose un **Checker** et relie sa sortie **Color**.
verifier: graphe.liens >= 1
```

## Le halo

**Glow** fait rayonner les zones claires, comme une optique un peu grasse.

```etape
Insère un node **Glow** après l'image.
verifier: graphe.contient = glow
```

| Réglage | Effet |
|---|---|
| **Threshold** | Seules les zones plus claires que ce seuil rayonnent |
| **Radius** | La largeur du halo |
| **Strength** | Sa force |

```etape
Baisse **Threshold** : de plus en plus de zones rayonnent. Remonte-le pour ne garder que les vraies lumières.
```

## La vignette

```etape
Insère un node **Vignette** après **Glow**. Règle **Strength** vers 0,4.
verifier: graphe.contient = vignette
```

Les coins s'assombrissent. Le regard va au centre.

## Le grain

```etape
Insère un node **Grain** juste avant la sortie. Laisse **Strength** basse.
verifier: graphe.contient = grain
```

Le grain unifie l'image. Il casse l'aspect trop propre d'une image de synthèse.

## L'ordre des filtres

Suis l'ordre de la lumière dans une caméra.

1. Ce qui se passe **dans la scène** : la couleur, la lumière.
2. Ce qui se passe **dans l'objectif** : halo, flou, aberration, distorsion, vignette.
3. Ce qui se passe **sur le capteur** : le grain.

```etape
Déplace **Grain** avant **Glow** : coupe ses liens et rebranche. Le grain est maintenant flouté par le halo. Remets-le en dernier.
```

Le grain va toujours à la fin. Sinon il est abîmé par ce qui suit.

## D'autres filtres

| Node | Effet |
|---|---|
| **Blur** | Flou |
| **Directional blur** | Flou dans une direction |
| **Radial blur (zoom)** | Flou qui part du centre |
| **Sharpen** | Netteté |
| **Chromatic aberration** | Décale le rouge et le bleu sur les bords |
| **Lens distortion** | Barillet ou coussinet |
| **Sun beams** | Rayons de lumière |
| **Halftone**, **Scanlines**, **Glitch**, **Pixelate** | Des effets marqués |
| **Transform**, **Flip** | Déplacer, tourner, retourner |

## Un masque de forme

Un masque est une image en gris. Blanc : l'effet s'applique. Noir : non.

```etape
Ajoute un node **Shape mask**. Garde **Shape** sur **ellipse**.
verifier: graphe.contient = shape
```

```etape
Fais un clic droit sur **Shape mask** et choisis **Preview node**. Tu vois le masque seul : une ellipse blanche sur fond noir.
```

```etape
Règle **Center x**, **Center y**, **Width** et **Height** pour entourer ton sujet. Monte **Softness** : le bord devient progressif.
```

```etape
Refais un clic droit et reviens à la sortie avec **Back to full material**.
```

## Un flou dosé par le masque

**Masked blur (tilt-shift)** floute plus ou moins selon un masque.

Le masque est blanc **sur** le sujet. Or c'est le reste que tu veux flouter. Il faut l'inverser.

```etape
Ajoute un node **Invert** et relie-lui **Shape mask**.
verifier: graphe.contient = invert
```

```etape
Insère un node **Masked blur (tilt-shift)** après l'image, avant **Glow**. Relie **Invert** à son entrée **Mask**. Monte **Radius**.
verifier: graphe.contient = maskblur
```

Le sujet reste net. Le reste est flou. C'est un faux effet de faible profondeur de champ.

```astuce
Un masque sert à tout. Relie-le au **Factor** d'un **Mix** pour ne corriger la couleur que dans une zone.
```

## Deux masques valent mieux qu'un

Les masques se combinent avec **Math**, comme dans les matériaux.

- **multiply** : garde ce qui est blanc dans les deux.
- **maximum** : garde ce qui est blanc dans l'un ou l'autre.

```etape
Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```quiz
Où placer le grain dans une chaîne de filtres ?
- [ ] Au début.
- [x] En dernier, juste avant la sortie.
- [ ] Avant le flou.
explication: Le grain vient du capteur, après l'objectif. Placé avant, il serait flouté ou déformé.
```

```quiz
Ton masque est blanc sur le sujet, et tu veux flouter le reste. Que fais-tu ?
- [x] Tu inverses le masque avec Invert.
- [ ] Tu baisses Softness.
- [ ] Tu changes la forme en rectangle.
explication: Le flou masqué agit là où le masque est blanc. Inversé, il agit partout sauf sur le sujet.
```

```defi
Fais un effet de miniature : une bande nette au milieu de l'image, le haut et le bas flous.
indice: Shape mask en rectangle, très large et peu haut, avec beaucoup de Softness, inversé, dans Masked blur.
```

## En trois lignes

- **Glow**, **Vignette**, **Grain** : le fini de cinéma, dans cet ordre.
- **Shape mask** dessine une zone. **Invert** la retourne.
- **Masked blur (tilt-shift)** floute là où le masque est blanc.

[Suite : Incrustation et passes de rendu](04-incrustation-et-passes.md)
