---
titre: Les masques
niveau: débutant
duree: 15 min
prerequis: 11-materiaux-atelier/02-calques-de-remplissage
objectifs: Poser un masque sur un calque ; Régler quantité, contraste et échelle ; Inverser et casser un masque ; Utiliser un masque intelligent
---

## Ce que tu vas réussir

Tu vas user une peinture sur ses arêtes, salir ses creux et la couvrir de poussière. Trois calques, trois masques, aucun node.

```essayer
nouveau: arymat
nom: masques
```

## Prépare la base

```etape
Au-dessus de l'aperçu, clique **Cube**. Les masques d'usure se jugent sur des arêtes et des creux.
```

```etape
Dans la bibliothèque, filtre **Presets** et clique **Scratched brushed metal**. Ce sera le métal nu.
verifier: atelier.calques >= 2
```

```etape
Clique **+**. Nomme le calque « peinture ». Donne-lui un bleu, **Roughness** 0,4.
verifier: atelier.calques >= 3
```

La peinture couvre tout. Son masque vaut **Everywhere**.

## Un masque, c'est une image en gris

Blanc : le calque se voit. Noir : il ne se voit pas. Gris : il se mélange.

Chaque ligne de la pile montre deux vignettes : la matière, puis le masque.

## Usure des arêtes

```etape
Choisis « peinture ». Dans la carte **Mask**, clique la pastille **Edge wear**.
```

Surprise : la peinture n'est plus que sur les arêtes. Le masque est blanc là où les arêtes sont saillantes. C'est l'inverse de ce qu'on veut.

```etape
Coche **Invert**. La peinture couvre tout, sauf les arêtes. Le métal y réapparaît.
```

Trois curseurs règlent le masque.

| Réglage | Effet |
|---|---|
| **Amount** | Combien de surface le masque couvre |
| **Contrast** | Bord doux ou bord franc |
| **Scale** | La taille du motif |

```etape
Monte **Amount** à 0,7. L'usure mord plus loin dans les faces.
```

```etape
Monte **Contrast** à 0,9. Le bord de la peinture devient net, comme un éclat.
```

## Casser la régularité

Une usure trop régulière sonne faux. **Breakup** découpe le masque avec un second motif.

```etape
Ouvre la liste **Breakup** et choisis **Grunge**. Deux curseurs apparaissent : **Strength** et **Breakup scale**.
```

```etape
Règle **Strength** vers 0,7. L'usure devient irrégulière : certaines arêtes tiennent, d'autres sont à nu.
```

## Saleté des creux

```etape
Clique **+**. Nomme le calque « saleté ». Donne-lui un brun très sombre et une **Roughness** de 0,9.
verifier: atelier.calques >= 4
```

```etape
Dans **Mask**, clique **Cavity dirt**. La saleté se loge dans les creux du cube.
```

```etape
Baisse **Amount** vers 0,3. Il en reste juste un liseré.
```

## Poussière sur le dessus

```etape
Clique **+**. Nomme le calque « poussière ». Donne-lui un beige clair, **Roughness** 1.
verifier: atelier.calques >= 5
```

```etape
Dans **Mask**, clique **Dust on top**. Le calque ne se pose que sur les faces tournées vers le haut.
```

```etape
Baisse **Opacity** du calque à 60 %. La poussière laisse deviner la peinture.
```

## Les autres masques

| Masque | Où le calque apparaît |
|---|---|
| **Drips** | En coulures verticales |
| **Scratches** | En rayures fines |
| **Grunge** | En taches de saleté |
| **Noise** | Selon un bruit |
| **Cracks** | Le long de fissures |
| **Spots** | En taches rondes |
| **Hand-painted** | Là où tu as peint, dans la scène |
| **Node graph** | Là où le dit un graphe de nodes |

Les deux derniers ont leur leçon : [la peinture 3D](../10-uv-textures/04-peinture-3d.md) et [le masque en nodes](05-masque-en-nodes.md).

## Les masques intelligents

Un masque intelligent est un masque déjà réglé : son générateur, ses curseurs, sa cassure.

```etape
Choisis « peinture ». Ouvre **Smart masks** et clique **Chipped paint on edges**.
```

Le masque est remplacé d'un coup. Recoche **Invert** si la peinture ne reste que sur les arêtes.

Les sept masques fournis : **Chipped paint on edges**, **Rust in cavities**, **Dust and dirt**, **Lightly worn edges**, **Rain streaks**, **Scratched metal**, **Cracked ground**.

```attention
Les masques d'usure lisent la courbure et l'orientation du modèle. Sur un modèle très peu détaillé, **Edge wear** et **Cavity dirt** ne trouvent presque rien.
```

```quiz
Ta peinture n'apparaît que sur les arêtes, alors que tu voulais l'inverse. Que fais-tu ?
- [ ] Tu baisses Amount à 0.
- [x] Tu coches Invert.
- [ ] Tu changes l'ordre des calques.
explication: Edge wear est blanc sur les arêtes. Inversé, il couvre tout sauf elles.
```

```quiz
À quoi sert Breakup ?
- [ ] À supprimer le masque.
- [x] À découper le masque avec un second motif, pour le rendre moins régulier.
- [ ] À flouter le bord du masque.
explication: La cassure ajoute de l'irrégularité. Le bord doux ou franc, c'est Contrast.
```

```defi
Ajoute des coulures de pluie sombres sur les côtés du cube.
indice: Un calque sombre et mat, puis Smart masks, Rain streaks. Baisse son opacité pour rester discret.
```

## En trois lignes

- Un masque dit où un calque se voit : blanc oui, noir non.
- **Amount**, **Contrast**, **Scale**, **Invert** et **Breakup** le règlent.
- **Smart masks** pose un masque tout réglé d'un clic.

[Suite : Entrer dans un calque](04-entrer-dans-un-calque.md)
