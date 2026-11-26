---
titre: Un masque en nodes
niveau: intermédiaire
duree: 15 min
prerequis: 11-materiaux-atelier/04-entrer-dans-un-calque
objectifs: Changer un masque simple en graphe ; Comprendre que la couleur de la sortie fait le masque ; Combiner deux générateurs d'usure ; Revenir à un masque simple
---

## Ce que tu vas réussir

Tu vas fabriquer un masque qu'aucune pastille ne propose : une usure d'arêtes mangée par un bruit, plus de la poussière sur le dessus. Le tout dans un seul masque.

```essayer
nouveau: arymat
nom: masque-en-nodes
```

## Prépare la pile

```etape
Passe l'aperçu sur **Cube**.
```

```etape
Clique **+**. Nomme le calque « rouille ». Donne-lui un brun orangé, **Roughness** 0,9.
verifier: atelier.calques >= 2
```

```etape
Dans la carte **Mask**, clique **Edge wear**. Règle **Amount** vers 0,6.
```

## Change le masque en graphe

```etape
Toujours dans **Mask**, clique la pastille **Node graph**. Le masque devient un graphe, et il s'ouvre.
```

Tu n'es pas reparti de zéro. Le graphe contient déjà ton masque : un node **Edge wear**, avec tes réglages, relié à l'entrée **Color** de la sortie.

Le fil d'Ariane le dit : **← Layers**, puis « rouille · mask ».

## La règle du masque en nodes

La **couleur** de la sortie fait le masque.

- Blanc : le calque se voit.
- Noir : il ne se voit pas.

Les autres entrées de la sortie ne servent pas ici. L'aperçu montre le masque en gris, sur le cube.

## Mange l'usure avec un bruit

```etape
Ajoute un node **Noise**. Mets **Scale** à 6.
verifier: graphe.contient = noise
```

```etape
Ajoute un node **Math**. Choisis l'opération **multiply**.
verifier: graphe.contient = math
```

```etape
Relie **Edge wear** à l'entrée **A** de **Math**. Relie la sortie **Value** de **Noise** à **B**.
```

```etape
Relie la sortie de **Math** à l'entrée **Color** de la sortie. Ce lien remplace l'ancien.
```

Multiplier par un bruit éteint l'usure là où le bruit est sombre. Certaines arêtes restent propres.

```astuce
Pour voir un node seul dans l'aperçu : clic droit sur lui, **Preview node**. **Back to full material** revient à la sortie.
```

## Ajoute la poussière du dessus

```etape
Ajoute un node **Dust (top)**. Il est blanc sur les faces tournées vers le haut.
verifier: graphe.contient = dust
```

```etape
Ajoute un second node **Math**, réglé sur **maximum**. Relie le premier **Math** à **A** et **Dust (top)** à **B**.
verifier: graphe.noeuds >= 6
```

```etape
Relie ce second **Math** à l'entrée **Color** de la sortie.
```

**maximum** garde le plus clair des deux masques. Le calque se pose donc sur les arêtes usées **et** sur le dessus.

| Opération | Ce qu'elle fait à deux masques |
|---|---|
| **multiply** | Garde ce qui est blanc dans les deux |
| **maximum** | Garde ce qui est blanc dans l'un ou l'autre |
| **subtract** | Retire le second du premier |

## Reviens à la pile

```etape
Clique **← Layers**. La vignette du masque, sur la ligne du calque, montre ton nouveau masque.
```

Sur la pile, la carte **Mask** garde deux réglages : **Invert**, et le bouton **Open the mask in nodes**. Un double-clic sur la vignette du masque l'ouvre aussi.

```attention
Cliquer une autre pastille, par exemple **Cavity dirt**, revient à un masque simple. Ton graphe de masque est alors remplacé. Un Ctrl+Z le rend.
```

## Peindre un masque à la main

Parfois, aucun calcul ne remplace ta main. La pastille **Hand-painted** lit un masque que tu peins sur l'objet, dans la scène. Tu choisis un **Channel** de 1 à 4.

Dans un graphe, le node **Painted mask** donne le même masque. Tu peux donc le combiner avec une usure. La marche complète est dans [les calques de peinture](../10-uv-textures/05-calques-de-peinture.md).

```quiz
Dans le graphe d'un masque, qu'est-ce qui décide où le calque se voit ?
- [x] La couleur qui arrive à la sortie.
- [ ] La rugosité de la sortie.
- [ ] Le nombre de nodes.
explication: Blanc : le calque se voit. Noir : non. Les autres entrées de la sortie ne servent pas pour un masque.
```

```quiz
Tu veux un masque blanc là où l'un OU l'autre de deux masques est blanc. Quelle opération ?
- [ ] multiply
- [x] maximum
- [ ] subtract
explication: maximum garde le plus clair des deux. multiply ne garde que ce qui est blanc dans les deux.
```

```defi
Fais une rouille qui ne touche pas le dessus de l'objet.
indice: Soustrais Dust (top) de ton usure avec un Math réglé sur subtract. Ajoute un Clamp si le résultat passe sous zéro.
```

## En trois lignes

- La pastille **Node graph** change le masque en graphe, à partir du masque déjà réglé.
- La couleur de la sortie fait le masque : blanc oui, noir non.
- **multiply** croise deux masques, **maximum** les réunit.

[Suite : Matériaux maîtres liés](06-materiaux-maitres-lies.md)
