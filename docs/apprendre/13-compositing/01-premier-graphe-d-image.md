---
titre: Ton premier graphe d'image
niveau: débutant
duree: 15 min
prerequis: 12-noeuds/01-principes
objectifs: Faire entrer une image dans un graphe ; Poser une correction entre l'entrée et la sortie ; Comparer avant et après ; Exporter en PNG
---

## Ce que tu vas réussir

Tu vas monter la chaîne la plus simple du compositing : une image, une correction, une sortie. Puis tu l'exporteras.

```essayer
nouveau: arycomp
nom: premier-compositing
```

```etape
Vérifie que tu es dans l'étape **2D**.
verifier: onglet = 2d
```

Le graphe part avec un seul node : **Output**.

## Le flux de base

1. Une **entrée** : une image.
2. Des **corrections** et des **filtres**.
3. La **sortie** : ce que montre l'aperçu, et ce que l'export écrit.

## Fais entrer une image

Deux façons.

- Glisse une image depuis le panneau des fichiers jusque dans le graphe. Elle devient un node **Image**.
- Ou ajoute un node **Image** avec Maj+A, puis choisis son fichier.

```etape
Fais entrer une image de ton coffre dans le graphe.
verifier: graphe.contient = image
```

```astuce
Pas d'image sous la main ? Pose un node **Noise** à la place, et relie sa sortie **Color**. Toute la leçon marche pareil.
```

```etape
Relie la sortie **Frame** de ton image à l'entrée **Frame** de **Output**. L'image s'affiche dans l'aperçu.
verifier: graphe.liens >= 1
```

Un node **Image** a deux sorties : **Frame**, l'image, et **Alpha**, sa transparence.

## Une première correction

```etape
Dans la bibliothèque, catégorie **Color**, attrape **Brightness / contrast**. Lâche-le sur le lien entre l'image et la sortie.
verifier: graphe.contient = brightcon
```

```etape
Monte **Contrast** à 0,3. L'image gagne en relief.
```

```etape
Monte un peu **Brightness**. Trop, et les clairs se bouchent : reviens en arrière.
```

## Avant, après

```etape
Choisis le node **Brightness / contrast** et appuie sur M. Il devient muet : tu vois l'image d'origine. Appuie encore sur M.
```

C'est le réflexe de tout étalonneur : comparer sans cesse avec le départ. Une correction qu'on ne voit pas en l'éteignant ne sert à rien. Une correction qui saute aux yeux est souvent trop forte.

## Une seconde correction

```etape
Insère un node **Hue / saturation** après le premier. Baisse **Saturation** à 0,7.
verifier: graphe.contient = hsv
```

L'ordre compte. Chaque node travaille sur ce que le précédent lui donne.

## Un cadre de cinéma

```etape
Insère un node **Letterbox** juste avant la sortie. Laisse **Aspect** à 2,39.
verifier: graphe.contient = letterbox
```

Deux bandes noires donnent le format du cinéma large.

## Exporte

```etape
Enregistre le graphe avec Ctrl+S.
verifier: fichier.enregistre
```

```etape
À droite de la barre du graphe, clique **Export PNG**.
```

L'image est écrite en pleine résolution, celle de l'image d'entrée. L'aperçu, lui, travaille plus petit pour rester rapide.

```attention
L'export est en PNG. Un EXR en sortie n'est pas prévu dans la version actuelle.
```

## Ce que tu peux brancher en entrée

| Node | Rôle |
|---|---|
| **Image** | Une image du coffre |
| **Multilayer EXR** | Un rendu 3D à plusieurs passes |
| **Video texture** | Une image d'une vidéo, à un **Time (s)** donné |
| Les motifs | **Noise**, **Gradient**, **Checker** : des images fabriquées |

```quiz
Dans un compositing, où place-t-on une correction ?
- [x] Entre l'entrée et la sortie.
- [ ] Après la sortie.
- [ ] N'importe où, sans lien.
explication: L'aperçu montre ce qui arrive à Output. Une correction doit être sur ce chemin.
```

```quiz
Comment comparer vite avec et sans une correction ?
- [ ] En la supprimant.
- [x] En la rendant muette avec M.
- [ ] En exportant deux fois.
explication: Un node muet laisse passer son entrée. Un second M le rallume.
```

```defi
Donne à ton image un air de vieille photo : peu de couleur, contraste doux, coins sombres.
indice: Hue / saturation avec une saturation basse, puis un node Vignette avant la sortie.
```

## En trois lignes

- Une image entre, des corrections la changent, **Output** la montre.
- M compare avec et sans.
- **Export PNG** écrit l'image en pleine résolution.

[Suite : Couleur et étalonnage](02-couleur-et-etalonnage.md)
