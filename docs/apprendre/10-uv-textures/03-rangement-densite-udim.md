---
titre: Rangement, densité et UDIM
niveau: avancé
duree: 15 min
prerequis: 10-uv-textures/02-les-coutures
objectifs: Ranger les îlots au plus serré ; Régler marge et taille de texture ; Égaliser la densité de texels ; Répartir sur plusieurs tuiles UDIM
---

## Ce que tu vas réussir

Tu vas tirer le plus de détail possible d'une image de texture : des îlots serrés, une densité égale, et plusieurs tuiles quand une seule ne suffit plus.

```essayer
nouveau: aryscene
nom: densite-udim
```

```etape
Ajoute un **Torus** et une **Sphere**. Choisis-les avec le cube et joins-les avec Ctrl+J. Tu as un seul objet fait de trois pièces.
verifier: scene.objets >= 3
```

```etape
Choisis cet objet, passe en mode **UV & materials**, onglet **UV**. Clique **Auto (seams + UVs)**.
verifier: scene.selection
```

## Le rangement

Chaque pixel de texture que les îlots ne couvrent pas est perdu. Ranger serré, c'est gagner du détail.

| Réglage | Effet |
|---|---|
| **Optimized packing** | Essaie plusieurs ordres, rotations et largeurs, et garde le plus serré |
| **Rotation** | Autorise la rotation des îlots |
| **Texture** | La taille de texture visée, en pixels |
| **Margin** | L'espace entre îlots, en pixels |

```etape
Lis la valeur **Fill**. Coche **Optimized packing** et **Rotation**, puis clique **Pack**. **Fill** monte.
```

### La marge

```etape
Mets **Margin** à 2, puis **Pack**. Remets 8, puis **Pack**.
```

Une marge trop faible, et la couleur d'un îlot bave sur son voisin quand la texture est réduite. Une marge trop large, et tu gaspilles de la place. Elle se compte en pixels : c'est pourquoi **Texture** doit valoir la taille que tu comptes cuire.

## La densité de texels

La densité est le nombre de pixels de texture par unité de surface. Si elle n'est pas égale, un objet a l'air net à un endroit et flou à un autre.

```etape
Clique **Checker**. Regarde si les carrés ont la même taille sur le cube, le tore et la sphère.
```

**Density:** affiche la densité moyenne actuelle.

```etape
Clique **= Current** : la densité moyenne devient la valeur à appliquer. Puis clique **Apply density**. Tous les îlots sont mis à cette densité.
```

```attention
Si la densité demandée est trop forte, les îlots débordent du carré. Il faut alors une texture plus grande, ou plusieurs tuiles.
```

```astuce
Dans une scène à plusieurs objets, note la densité d'un objet et applique la même aux autres. Tout aura le même niveau de netteté.
```

## Les UDIM

Une seule image a ses limites. Les **UDIM** répartissent les îlots sur plusieurs carrés UV, les tuiles : 1001, 1002, 1003… Chaque tuile a ses propres images à la cuisson. Tu multiplies le détail sans grossir chaque image.

```etape
À côté de **UDIM**, mets le nombre de tuiles à 3.
```

```etape
Clique **Distribute**. Les îlots sont répartis sur trois tuiles. L'éditeur UV les montre côte à côte, et la plage de tuiles s'affiche : 1001 à 1003.
```

```etape
Clique **Checker**. Les carrés sont plus petits qu'avant : chaque îlot a plus de pixels.
```

```etape
Remets 1 tuile et clique **Distribute**. Tout revient dans le carré de base.
```

Le nombre de tuiles va de 1 à 20.

```attention
Les UDIM sont gérées au rangement et à la cuisson. Leur affichage dans la vue 3D et la peinture sur plusieurs tuiles ne sont pas garantis dans la version actuelle. Pour un objet à peindre à la main, reste sur une tuile.
```

```etape
Reviens à la scène et enregistre.
verifier: fichier.enregistre
```

```quiz
Pourquoi la marge se règle-t-elle en pixels ?
- [x] Parce qu'elle dépend de la taille de l'image qu'on va cuire.
- [ ] Parce que les UV sont en pixels.
- [ ] Pour aller plus vite.
explication: Huit pixels d'écart sur une image de 512 ou de 4096, ce n'est pas la même distance dans le carré UV.
```

```quiz
Que t'apportent trois tuiles UDIM au lieu d'une ?
- [ ] Un modèle plus léger.
- [x] Plus de pixels par îlot, sans agrandir chaque image.
- [ ] Moins de coutures.
explication: Chaque tuile a ses propres images. Les îlots se partagent trois carrés au lieu d'un.
```

```defi
Donne au tore deux fois plus de densité qu'au reste, parce qu'il sera vu de près.
indice: Dans l'éditeur UV, prends ses îlots avec L et agrandis-les avec S. Range ensuite sans écraser ton réglage : essaie sur deux tuiles.
```

## En trois lignes

- **Pack** range. **Optimized packing** et **Rotation** serrent davantage.
- **Apply density** égalise le nombre de pixels par unité de surface.
- **Distribute** répartit les îlots sur plusieurs tuiles UDIM.

[Suite : La peinture 3D](04-peinture-3d.md)
