---
titre: Couleur et étalonnage
niveau: intermédiaire
duree: 15 min
prerequis: 13-compositing/01-premier-graphe-d-image
objectifs: Poser les noirs et les blancs avec Levels ; Corriger une dominante ; Donner une ambiance par la balance des couleurs ; Mélanger deux versions avec Mix
---

## Ce que tu vas réussir

Tu vas étalonner une image en trois temps, comme un professionnel : la remettre d'aplomb, lui donner une ambiance, puis doser le tout.

```essayer
nouveau: arycomp
nom: etalonnage
```

```etape
Fais entrer une image de ton coffre et relie-la à **Output**. À défaut, pose un **Noise** et relie sa sortie **Color**.
verifier: graphe.liens >= 1
```

## Deux temps, dans l'ordre

1. **Corriger.** L'image retrouve des noirs noirs, des blancs blancs, des couleurs justes.
2. **Styliser.** Tu ajoutes une intention : chaud, froid, délavé.

Styliser une image mal corrigée, c'est repeindre un mur sale.

## Corriger : Levels

```etape
Insère un node **Levels** après l'image.
verifier: graphe.contient = levels
```

| Réglage | Effet |
|---|---|
| **Black** | En dessous de cette valeur, tout devient noir |
| **White** | Au-dessus, tout devient blanc |
| **Gamma** | Éclaircit ou assombrit les tons moyens |

```etape
Monte **Black** jusqu'à ce que les zones les plus sombres soient vraiment noires. Pas plus.
```

```etape
Baisse **White** jusqu'à ce que les zones les plus claires soient vraiment blanches.
```

```etape
Règle **Gamma** pour la clarté d'ensemble.
```

Une image qui occupe toute la plage, du noir au blanc, a du corps.

## Corriger : la dominante

```etape
Insère un node **White balance** après **Levels**.
verifier: graphe.contient = whitebal
```

```etape
Bouge **Temperature**. D'un côté l'image bleuit, de l'autre elle jaunit. Arrête-toi quand les gris sont gris.
```

## Styliser : Color balance

**Color balance** teinte séparément les ombres, les tons moyens et les lumières.

```etape
Insère un node **Color balance** après **White balance**.
verifier: graphe.contient = colorbalance
```

```etape
Donne une teinte bleu-vert à **Shadows (lift)**. Donne une teinte orangée à **Highlights (gain)**.
```

Ombres froides, lumières chaudes : c'est l'ambiance la plus répandue du cinéma. Elle fait ressortir la peau.

```attention
Reste léger. Une teinte à peine visible sur la pastille suffit. Rends le node muet avec M pour juger.
```

## D'autres outils de style

| Node | Effet |
|---|---|
| **Curve (S contrast)** | Un contraste en S, avec **Shadows** et **Highlights** |
| **RGB curves** | Des courbes tracées à la souris : ensemble, rouge, vert, bleu |
| **Split toning** | Une teinte pour les ombres, une pour les lumières |
| **Tint (duotone)** | Deux couleurs seulement |
| **Vibrance** | Sature surtout les couleurs ternes |
| **Tone mapping** | Ramène une image très lumineuse dans la plage affichable : **ACES**, **filmic**, **Reinhard** |

## Doser avec Mix

Ton étalonnage est trop fort ? Plutôt que de retoucher chaque node, mélange le résultat avec l'image de départ.

```etape
Ajoute un node **Mix**. Relie la sortie de l'image d'origine à **A**. Relie la fin de ta chaîne de couleur à **B**. Relie **Mix** à **Output**.
verifier: graphe.contient = mix
```

```etape
Règle **Factor**. À 0, l'image d'origine. À 1, l'étalonnage complet. Trouve le dosage juste.
```

Une sortie peut nourrir plusieurs entrées : l'image part à la fois dans la chaîne et dans **Mix**.

Le réglage **Mode** de **Mix** propose aussi multiplier, écran, incrustation et d'autres modes de fusion.

## Range ton étalonnage

```etape
Choisis **Levels** et **White balance**. Appuie sur Ctrl+J et titre le cadre « correction ». Fais de même pour le style.
```

Dans un mois, tu sauras où régler quoi.

## Étalonner un clip du montage

Le même graphe sert sur une vidéo. Dans le montage, sur un clip : **＋ Node grading**. Aryadne crée un `.arycomp` relié au clip. Le graphe s'applique à ses couleurs, à l'aperçu comme à l'export.

```etape
Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```quiz
Dans quel ordre travaille-t-on ?
- [x] Corriger d'abord, styliser ensuite.
- [ ] Styliser d'abord, corriger ensuite.
- [ ] Peu importe.
explication: Une ambiance posée sur une image mal exposée ou teintée donne un résultat imprévisible.
```

```quiz
Ton étalonnage est trop appuyé. Le plus rapide pour l'adoucir ?
- [ ] Baisser tous les réglages un par un.
- [x] Le mélanger avec l'image d'origine par un Mix, et régler Factor.
- [ ] Recommencer.
explication: Un seul curseur dose toute la chaîne.
```

```defi
Fais trois ambiances de la même image : matin froid, midi neutre, soir doré.
indice: Duplique la chaîne de style avec Maj+D. Change seulement Color balance. Un Switch ou des M te laissent passer de l'une à l'autre.
```

## En trois lignes

- **Levels** pose les noirs et les blancs. **White balance** retire une dominante.
- **Color balance** donne l'ambiance : ombres froides, lumières chaudes.
- **Mix** avec l'original dose tout l'étalonnage d'un curseur.

[Suite : Filtres et masques](03-filtres-et-masques.md)
