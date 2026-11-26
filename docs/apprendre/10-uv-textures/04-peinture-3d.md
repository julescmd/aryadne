---
titre: La peinture 3D
niveau: intermédiaire
duree: 15 min
prerequis: 10-uv-textures/01-deplier-les-uv
objectifs: Entrer en mode peinture ; Régler le pinceau ; Peindre et gommer sur le modèle ; Poser un décalque ; Savoir où va la peinture
---

## Ce que tu vas réussir

Tu vas peindre directement sur un objet, comme sur une figurine. Puis tu y colleras une image, à la manière d'un autocollant.

```essayer
nouveau: aryscene
nom: peinture-3d
```

## Avant de peindre : des UV

Chaque touche de pinceau est déposée dans une image, posée sur les UV de l'objet. Sans UV, la peinture n'a nulle part où aller.

```etape
Choisis le cube. Passe en mode **UV & materials**, onglet **UV**. Clique **Auto (seams + UVs)**. Reviens à la scène.
verifier: scene.selection
```

## Entre en peinture

```etape
Le cube choisi, passe en mode **Paint** dans la barre de la vue.
```

En bas de la vue, une barre porte les réglages du pinceau. Un cercle suit ta souris.

## Peins

```etape
Clique la pastille de couleur de la barre et choisis un rouge.
```

```etape
Glisse sur le cube. La peinture se pose.
```

| Geste | Effet |
|---|---|
| Glisser | Peindre |
| Alt + glisser, ou molette enfoncée | Tourner autour du modèle |
| Maj + molette enfoncée | Déplacer la vue |
| [ et ] | Taille du pinceau |
| E | Gomme |
| Ctrl+Z | Annuler |

```etape
Tourne autour du cube avec la molette enfoncée. Peins une autre face.
```

## Règle le pinceau

| Réglage | Effet |
|---|---|
| **Radius ([ and ])** | Le rayon du pinceau à l'écran |
| **Opacity** | L'opacité d'une touche |
| **Edge hardness** | 0 : bord très doux. 1 : bord net |

```etape
Mets **Edge hardness** à 1. Trace un trait : son bord est net.
```

```etape
Mets **Edge hardness** à 0 et **Opacity** à 30 %. Passe plusieurs fois au même endroit : la couleur monte peu à peu.
```

Une opacité basse et un bord doux servent aux dégradés, à la saleté, aux ombres peintes. Un bord net sert aux motifs.

## Gomme

```etape
Appuie sur E pour activer **Eraser (E)**. Glisse sur la peinture : elle s'en va. Appuie encore sur E pour revenir au pinceau.
```

**Clear the selected object's paint** efface toute la peinture de l'objet.

## Un décalque

Un décalque projette une image sur l'objet, là où tu cliques : un logo, une étiquette, un numéro.

```etape
Clique **Decal** dans la barre et choisis une image de ton coffre.
```

```etape
Règle la taille du pinceau avec [ et ], puis clique sur le cube. L'image est posée, à la taille du pinceau.
```

```etape
Clique de nouveau sur **Decal** pour le retirer du pinceau.
```

Un décalque se pose une fois, au clic. Il ne se traîne pas.

## Où va la peinture

- Les textures sont des images PNG de 2048 pixels de côté, dans le dossier `textures` du coffre.
- La scène garde le chemin de chaque image dans l'objet.
- Dans la vue et au rendu, la peinture se mélange au matériau de l'objet.

```attention
La peinture suit les UV. Si tu refais les UV après avoir peint, la peinture ne correspond plus. Déplie d'abord, peins ensuite.
```

## Aux coutures

Un trait qui traverse une couture UV peut laisser une fine ligne. C'est une raison de plus de placer les coutures dans les endroits peu visibles.

## Sors

```etape
Clique la croix de la barre : **Leave painting**. Enregistre la scène.
verifier: fichier.enregistre
```

## Ce qui n'existe pas encore

- Un seul pinceau rond. Pas de pinceaux à texture, pas de pochoir, pas de symétrie en peinture.
- La taille de texture est fixe : 2048 pixels.

```quiz
Tu entres en peinture, tu glisses, et rien ne se passe. Que manque-t-il le plus souvent ?
- [ ] Une lumière.
- [x] Des UV sur l'objet.
- [ ] Un matériau.
explication: La peinture est déposée dans une image posée sur les UV. Sans UV, elle n'a nulle part où aller.
```

```quiz
Que règle Edge hardness ?
- [ ] La taille du pinceau.
- [x] La netteté du bord d'une touche.
- [ ] La rugosité de la surface.
explication: À 0 le bord est très doux, à 1 il est net. La rugosité se peint dans un calque à part.
```

```defi
Peins un dé : six faces blanches, avec leurs points noirs.
indice: Bord net, opacité pleine, un petit pinceau, et un clic par point. Les faces opposées font toujours sept.
```

## En trois lignes

- Déplie d'abord : la peinture a besoin d'UV.
- [ et ] règlent la taille, E gomme, **Edge hardness** le bord.
- **Decal** pose une image au clic.

[Suite : Les calques de peinture](05-calques-de-peinture.md)
