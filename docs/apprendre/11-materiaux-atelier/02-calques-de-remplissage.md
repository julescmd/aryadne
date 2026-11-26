---
titre: Les calques de remplissage
niveau: débutant
duree: 15 min
prerequis: 11-materiaux-atelier/01-decouvrir-l-atelier
objectifs: Créer un calque de remplissage ; Régler couleur, rugosité et métal ; Ordonner et dupliquer des calques ; Mélanger avec l'opacité
---

## Ce que tu vas réussir

Tu vas fabriquer trois matières à la main : un plastique rouge, un or poli, un caoutchouc noir. Puis tu vas les empiler et les mélanger.

```essayer
nouveau: arymat
nom: remplissages
```

## Un calque de remplissage

C'est le calque le plus simple. Il remplit toute la surface d'une seule matière. Il expose trois paramètres.

| Paramètre | 0 | 1 |
|---|---|---|
| **Color** | | La couleur de base |
| **Roughness** | Miroir | Mat |
| **Metal** | Non métallique | Métal |

```etape
Dans l'en-tête de la pile, clique **+**. Un calque de remplissage arrive en haut.
verifier: atelier.calques >= 2
```

Il recouvre le calque **Graph** du départ. Ses propriétés s'affichent en bas à droite, en trois cartes : **Layer**, **Parameters**, **Mask**.

## Un plastique rouge

```etape
Dans la carte **Layer**, change **Name** : écris « plastique ».
```

```etape
Dans la carte **Parameters**, clique la pastille de **Color**. Choisis un rouge vif.
```

```etape
Mets **Roughness** à 0,3 et laisse **Metal** à 0.
```

Tu obtiens un plastique : coloré, avec un reflet blanc assez net.

```astuce
Un paramètre changé porte une petite flèche : **Back to the graph's value**. Elle rend au paramètre sa valeur d'origine.
```

## Un or poli

```etape
Clique **+** de nouveau. Nomme le calque « or ».
verifier: atelier.calques >= 3
```

```etape
Donne-lui un jaune orangé. Mets **Metal** à 1 et **Roughness** à 0,15.
```

Avec **Metal** à 1, les reflets prennent la couleur de la matière. C'est ce qui distingue un métal d'un plastique.

```etape
Monte **Roughness** à 0,6. L'or devient brossé : le reflet s'étale.
```

## Un caoutchouc noir

```etape
Choisis le calque « plastique ». Dans l'en-tête, clique **Duplicate layer**. La copie s'appelle « plastique (copy) ».
verifier: atelier.calques >= 4
```

```etape
Renomme la copie « caoutchouc ». Donne-lui un gris très sombre et une **Roughness** de 0,9.
```

## L'ordre des calques

```etape
Choisis « caoutchouc ». Clique **Move up** jusqu'à ce qu'il soit en haut de la pile.
```

L'aperçu est noir : le calque du dessus cache les autres.

```etape
Clique **Move down** une fois. L'or repasse devant.
```

## L'opacité

**Opacity** mélange un calque avec ce qui est dessous.

```etape
Choisis le calque du dessus. Dans la carte **Layer**, baisse **Opacity** à 50 %.
```

Les deux matières se mélangent : la couleur, mais aussi la rugosité et le métal. Un calque couvre tous les canaux à la fois.

```attention
L'opacité fait un mélange uniforme, partout. Pour qu'un calque n'apparaisse qu'à certains endroits, il faut un masque. C'est la leçon suivante.
```

## Enregistre

```etape
Remets **Opacity** à 100 %. Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```quiz
Quel réglage fait la différence entre un plastique jaune et de l'or ?
- [ ] Roughness
- [x] Metal
- [ ] Opacity
explication: À 1, Metal teinte les reflets avec la couleur de la matière. Roughness ne règle que la netteté du reflet.
```

```quiz
Tu baisses l'opacité d'un calque à 50 %. Qu'est-ce qui se mélange ?
- [ ] Seulement la couleur.
- [x] Tous les canaux : couleur, rugosité, métal.
- [ ] Rien, le calque disparaît.
explication: Un calque couvre tous les canaux à la fois. On ne peut pas ne poser que sa rugosité.
```

```defi
Fabrique un chrome, puis un cuivre mat, sur deux calques.
indice: Chrome : gris très clair, Metal 1, Roughness proche de 0. Cuivre : brun orangé, Metal 1, Roughness vers 0,7.
```

## En trois lignes

- **+** crée un calque de remplissage : **Color**, **Roughness**, **Metal**.
- L'ordre compte : le calque du dessus cache les autres.
- **Opacity** mélange tous les canaux d'un calque avec le dessous.

[Suite : Les masques](03-les-masques.md)
