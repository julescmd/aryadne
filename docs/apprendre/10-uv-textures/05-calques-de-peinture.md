---
titre: Les calques de peinture
niveau: intermédiaire
duree: 20 min
prerequis: 10-uv-textures/04-peinture-3d, 11-materiaux-atelier/03-les-masques
objectifs: Ajouter et ordonner des calques de peinture ; Peindre la rugosité et le métal ; Peindre un masque lu par l'Atelier ; Doser un calque par son opacité
---

## Ce que tu vas réussir

Tu vas peindre sur plusieurs calques : une couleur, puis des zones brillantes, puis un masque qui dit à un matériau où poser sa rouille. Ta main pilote l'Atelier.

```essayer
nouveau: aryscene
nom: calques-de-peinture
```

```etape
Choisis le cube. Déplie-le : mode **UV & materials**, onglet **UV**, **Auto (seams + UVs)**. Reviens à la scène, puis passe en mode **Paint**.
verifier: scene.selection
```

## Le panneau des calques

Le panneau **PAINT LAYERS** liste les calques de l'objet choisi, du bas vers le haut. Chaque calque est une image PNG du coffre.

Pour l'instant il est vide : « No layer yet: paint, or add one. ».

| Bouton | Effet |
|---|---|
| **Add a layer** | Nouveau calque. Tu choisis son canal |
| Œil | Montrer ou cacher le calque |
| Pinceau | Peindre sur ce calque |
| Curseur | L'opacité du calque |
| Retirer | Retire le calque. Son image reste dans le coffre |

## Les canaux

Un calque appartient à un canal. Le canal dit ce que ta peinture change.

| Canal | Ce que tu peins |
|---|---|
| **Color** | La couleur de la surface |
| **Roughness** | La rugosité. 0 : lisse. 1 : rugueux |
| **Metal** | Le métal. 0 : non métallique. 1 : métal |
| **Mask 1 (Atelier)** à **Mask 4 (Atelier)** | Un masque, lu par un calque de l'Atelier |

Pour la rugosité, le métal et les masques, tu ne peins pas une couleur. Tu peins une **valeur**, de 0 à 1.

## Un calque de couleur

```etape
Clique **Add a layer** et choisis le canal **Color**.
```

```etape
Peins de larges bandes d'une couleur vive sur le cube.
```

## Un second calque de couleur

```etape
Clique **Add a layer**, canal **Color**, de nouveau. Vérifie que son pinceau est allumé : tu peins sur celui-ci.
```

```etape
Peins des motifs d'une autre couleur par-dessus les bandes.
```

```etape
Clique l'œil du second calque. Les motifs disparaissent, les bandes restent intactes. Rallume-le.
```

```etape
Baisse le curseur d'opacité du second calque. Les motifs deviennent discrets.
```

C'est l'intérêt des calques : chaque idée a sa couche. Tu corriges l'une sans abîmer l'autre.

## Peindre la rugosité

```etape
Clique **Add a layer** et choisis **Roughness**.
```

```etape
Peins avec une valeur proche de 0, c'est-à-dire un noir. Les zones peintes deviennent brillantes : regarde le reflet de la lumière en tournant.
```

```astuce
Des traces brillantes là où les mains touchent un objet, des zones mates là où il est sale : la rugosité peinte raconte l'usage mieux que la couleur.
```

## Peindre un masque pour l'Atelier

C'est le pont entre la peinture et les matériaux. Dans l'Atelier, un calque peut avoir le masque **Hand-painted**. Il se pose alors là où tu as peint, sur l'objet.

### Côté matériau

```etape
Sors de la peinture avec **Leave painting**. Choisis le cube. Dans **Properties**, onglet **Material**, clique le petit **＋** : un matériau est créé pour le cube et s'ouvre dans l'Atelier.
verifier: fichier.ouvert = arymat
```

```etape
Clique **+** dans l'en-tête de la pile. Nomme le calque « rouille ». Donne-lui un brun orangé et une **Roughness** de 0,9.
verifier: atelier.calques >= 2
```

```etape
Dans la carte **Mask**, clique **Hand-painted**. Laisse **Channel** sur 1.
```

La rouille disparaît de l'aperçu : rien n'est encore peint dans ce canal.

### Côté scène

```etape
Reviens à la scène. Choisis le cube et passe en mode **Paint**.
verifier: sous_onglet = scene
```

```etape
Clique **Add a layer** et choisis **Mask 1 (Atelier)**.
```

```etape
Peins en blanc le bas du cube et quelques coins. La rouille apparaît là où tu peins.
```

Tu peins un masque, pas une couleur. La matière, elle, vient du calque de l'Atelier : tu peux changer la rouille en mousse sans repeindre.

```attention
Le canal du calque de peinture doit être le même que le **Channel** choisi dans l'Atelier. Quatre canaux par objet, pas plus.
```

## Aller plus loin : mêler main et calcul

Dans le graphe d'un masque en nodes, le node **Painted mask** donne ce que tu as peint. Multiplie-le par un **Grunge** : ta main donne l'endroit, le motif donne le grain. Voir [Un masque en nodes](../11-materiaux-atelier/05-masque-en-nodes.md).

```etape
Sors de la peinture et enregistre la scène.
verifier: fichier.enregistre
```

```quiz
Que peins-tu dans un calque du canal Roughness ?
- [ ] Une couleur.
- [x] Une valeur de 0 à 1 : lisse ou rugueux.
- [ ] Un masque pour l'Atelier.
explication: Rugosité, métal et masques se peignent en valeurs. Seul le canal Color reçoit une couleur.
```

```quiz
Ton calque Atelier en Hand-painted, canal 2, ne montre rien. Pourquoi ?
- [ ] Il faut cuire les textures.
- [x] L'objet n'a pas encore de calque de peinture Mask 2 (Atelier), ou rien n'y est peint.
- [ ] Le calque est trop bas dans la pile.
explication: Le masque peint se lit dans le calque de peinture du même canal. Sans peinture, le masque est noir.
```

```defi
Salis le bas du cube avec de la boue, sur le canal 2, sans toucher à la rouille du canal 1.
indice: Un second calque Atelier, masque Hand-painted, Channel 2. Puis un calque de peinture Mask 2 (Atelier).
```

## En trois lignes

- Un calque de peinture appartient à un canal : couleur, rugosité, métal ou masque.
- Chaque idée a son calque : œil, opacité, ordre.
- **Mask 1 (Atelier)** fait de ta peinture le masque d'un calque de matériau.

[Suite : La cuisson](06-la-cuisson.md)
