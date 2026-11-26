---
titre: Incrustation et passes de rendu
niveau: avancé
duree: 20 min
prerequis: 13-compositing/03-filtres-et-masques
objectifs: Retirer un fond vert ; Nettoyer le débordement ; Superposer un sujet sur un décor ; Relire les passes d'un rendu 3D ; Flouter d'après une passe de profondeur
---

## Ce que tu vas réussir

Tu vas détacher un sujet de son fond vert et le poser sur un autre décor. Sans vidéo sous la main : tu fabriqueras ton fond vert en nodes, pour comprendre chaque étape.

```essayer
nouveau: arycomp
nom: incrustation
```

## Fabrique un faux fond vert

```etape
Pose un node **Color** vert vif, et un node **Checker** : ce sera ton « sujet ».
verifier: graphe.contient = checker
```

```etape
Pose un node **Shape mask**, réglé en **ellipse**, avec un peu de **Softness**.
verifier: graphe.contient = shape
```

```etape
Pose un node **Mix**. Relie **Shape mask** à **Factor**, le vert à **A**, le damier à **B**. Relie **Mix** à **Output**.
verifier: graphe.liens >= 4
```

Tu vois un disque de damier sur fond vert. C'est ta plaque de tournage.

```astuce
Avec une vraie image sur fond vert, remplace tout ce montage par un node **Image**. La suite est identique.
```

## Retire le vert

```etape
Insère un node **Chroma key (green screen)** entre **Mix** et **Output**.
verifier: graphe.contient = chromakey
```

```etape
Clique la pastille de **Key color** et choisis le même vert que ton fond.
```

Le fond disparaît. Le node a deux sorties : **Frame**, l'image détourée, et **Mask**, le masque du sujet.

| Réglage | Effet |
|---|---|
| **Key color** | La couleur à retirer |
| **Tolerance** | À quel point une couleur voisine est retirée aussi |
| **Softness** | La douceur du bord |

```etape
Fais un clic droit sur **Chroma key (green screen)**, puis **Preview node**. Monte **Tolerance** jusqu'à ce que le fond soit bien vide, sans manger le sujet. Reviens à la sortie.
```

## Le débordement

Sur un vrai tournage, le fond vert se reflète sur le sujet : un liseré vert sur les cheveux, les épaules. C'est le débordement.

```etape
Insère un node **Despill** après la sortie **Frame** de la clé. Laisse **Color** sur **green**.
verifier: graphe.contient = despill
```

Le bord doux de ton disque, teinté de vert, redevient neutre.

## Pose le sujet sur un décor

```etape
Pose un node **Gradient**, puis un **Color ramp** aux couleurs d'un ciel. Ce sera le décor.
verifier: graphe.contient = colorramp
```

```etape
Pose un node **Alpha over**. Relie le décor à **Below** et la sortie de **Despill** à **Above**. Relie **Alpha over** à **Output**.
verifier: graphe.contient = alphaover
```

Le disque de damier flotte sur le ciel.

**Alpha over** pose une image sur une autre selon sa transparence. **Opacity** dose le dessus.

## Les autres clés

| Node | Garde |
|---|---|
| **Luma key** | Les zones claires, ou sombres |
| **Difference key** | Ce qui diffère d'une image du même plan sans le sujet |
| **Roto mask (path)** | Un masque tracé point par point |
| **Unmultiply (remove black)** | Change le noir en transparent : flammes, étincelles |
| **Set alpha** | Donne une transparence à une image, d'après un masque |

## Fondre le sujet dans le décor

Un sujet incrusté a souvent l'air collé. Deux gestes aident.

- Un **Grain** unique, tout à la fin, par-dessus le sujet et le décor.
- **Grain match (measured on the plate)** : il mesure le grain d'une image filmée et le pose sur une autre.

## Les passes d'un rendu 3D

Un rendu 3D peut s'enregistrer en **Multilayer EXR (passes)**. Le fichier contient plusieurs images : la couleur, mais aussi la profondeur et d'autres passes.

```etape
Ajoute un node **Multilayer EXR**. Lis ses réglages : **File**, le fichier, et **Layer**, le nom de la passe à lire.
verifier: graphe.contient = exr
```

Un node par passe. Tu les recombines ensuite à ta façon.

### La profondeur de champ après coup

**Depth of field (Z pass)** floute une image d'après sa passe de profondeur.

| Réglage | Effet |
|---|---|
| **Focus distance** | La distance nette |
| **Sharp range** | L'épaisseur de la zone nette |
| **Falloff** | La vitesse à laquelle le flou arrive |
| **Max radius (px)** | Le flou le plus fort |

L'entrée **Frame** reçoit l'image, l'entrée **Depth** la passe de profondeur. Tu choisis la mise au point après le rendu, sans rien recalculer en 3D.

Pour produire le fichier : [Rendu](../15-animation-rendu/07-rendu.md).

```etape
Supprime le node **Multilayer EXR** s'il n'est relié à rien. Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```quiz
À quoi sert Despill ?
- [ ] À retirer le fond vert.
- [x] À retirer le reflet vert sur le sujet.
- [ ] À flouter le bord.
explication: Chroma key retire le fond. Despill nettoie la teinte verte que le fond a laissée sur le sujet.
```

```quiz
Dans Alpha over, où brancher le sujet détouré ?
- [ ] Sur Below.
- [x] Sur Above.
- [ ] Sur Opacity.
explication: Above est posé par-dessus Below, selon sa transparence.
```

```defi
Ajoute une ombre du sujet sur le décor.
indice: Reprends la sortie Mask de la clé, décale-la avec Transform, floute-la, et sers-t'en de Factor dans un Mix qui assombrit le décor.
```

## En trois lignes

- **Chroma key (green screen)** retire le fond, **Despill** nettoie le sujet.
- **Alpha over** pose le sujet sur le décor.
- **Multilayer EXR** relit les passes d'un rendu. La profondeur permet de flouter après coup.

[Suite : Le recalcul incrémental](05-recalcul-incremental.md)
