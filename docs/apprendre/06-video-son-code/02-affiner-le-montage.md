---
titre: Affiner : outils, titres, transitions, couleur
niveau: intermédiaire
duree: 15 min
prerequis: 06-video-son-code/01-premier-montage
objectifs: Te servir des outils de la timeline ; Poser un titre ; Ajouter un fondu ; Régler le cadre et le son d'un clip ; Étalonner un plan
---

## Ce que tu vas réussir à faire

Tu vas reprendre ton premier montage et le finir : des coupes ajustées, un titre, un fondu entre deux plans, une musique sous la voix et une couleur cohérente.

```essayer
nouveau: aryseq
nom: montage-fini
```

```etape
Clique **Essaie par toi-même**. Pose trois plans sur la timeline, comme dans la leçon précédente. Ou rouvre ton premier montage avec `Ctrl+P`.
verifier: onglet = video
```

## Les outils de la timeline

| Outil | Touche | Usage |
|---|---|---|
| Sélection | `V` | Déplacer, raccourcir par les bords |
| Piste en avant | `A` | Choisit tout ce qui suit. `Maj` : une seule piste |
| Propagation | `B` | Raccourcit sans laisser de trou |
| Déroulement | `N` | Déplace une coupe entre deux clips |
| Étirement temporel | `R` | La longueur change la vitesse |
| Cutter | `C` | Coupe à l'endroit cliqué. `Maj` : toutes les pistes |
| Déplacement | `Y` | Change la partie du média utilisée, sans bouger le clip |
| Main | `H` | Fait défiler la timeline |
| Zoom | `Z` | Clic pour zoomer, `Alt+clic` pour dézoomer |
| Texte | `T` | Pose un titre |

## Ajuster une coupe

```etape
Appuie sur `N`. Glisse la coupe entre le premier et le deuxième plan. L'un s'allonge, l'autre raccourcit. La durée totale ne change pas.
```

```etape
Appuie sur `Y`. Glisse à l'intérieur du deuxième plan. Sa place et sa durée restent, mais il montre un autre moment du média.
```

Ces deux outils règlent le rythme sans rien décaler. Reviens à la sélection avec `V`.

## Un titre

```etape
Place la tête de lecture au début. Appuie sur `T` et clique dans le moniteur. Tape ton titre.
```

Le titre est un clip, sur une piste au-dessus de l'image. Choisis-le : l'onglet **Clip** de l'inspecteur montre son texte, sa **Scale**, sa **Color**, son contour et sa police.

```etape
Change la police et la couleur du titre. Glisse le bord du clip pour qu'il dure trois secondes.
```

## Une transition

```etape
Choisis le premier plan. Appuie sur `Ctrl+D` : un fondu enchaîné se pose vers le plan suivant.
```

Dans l'onglet **Clip**, la section **To the next shot** règle la même chose : **Cut** ou **Dissolve**, avec sa durée.

Le panneau **Effects** liste les transitions et les effets, avec un champ de recherche. Glisse un effet sur un clip, ou double-clique pour l'appliquer au clip choisi.

```astuce
Une coupe franche passe presque toujours mieux qu'un fondu. Garde le fondu pour un passage du temps ou un changement de lieu.
```

## Le cadre d'un clip

L'onglet **Clip** contient la section **Frame** : **Position X**, **Position Y**, **Scale**, **Opacity**, et **Recenter**.

```etape
Choisis un plan. Monte **Scale** pour recadrer plus serré. Corrige avec **Position X** et **Position Y**.
```

### Animer le cadre

```etape
Place la tête de lecture au début du plan. Clique **Turn on animation** : une clé se pose.
```

```etape
Va à la fin du plan. Monte un peu **Scale**. Une deuxième clé se pose. Relis : le plan zoome lentement.
```

Une fois l'animation activée, chaque réglage modifié pose sa clé à la tête de lecture.

## La vitesse

| Réglage | Effet |
|---|---|
| **Speed** | La vitesse du clip |
| **Freeze frame** | Fige l'image de la tête de lecture pendant 2 secondes. La suite recule d'autant |

L'outil d'étirement (`R`) règle la vitesse à la souris : tu allonges le clip, il ralentit.

## Le son d'un clip

La section **Sound** de l'onglet **Clip** : **Volume**, **Fade in**, **Fade out**.

```etape
Glisse une musique du panneau **Media (edit)** sur une piste son, sous tes plans.
```

```etape
Choisis le clip de musique. Baisse son **Volume**. Règle un **Fade out** de deux secondes.
```

`Ctrl+Maj+D` pose un fondu audio sur le clip choisi.

```astuce
Le mixage du montage est simple. Pour nettoyer une voix ou faire baisser la musique toute seule sous la parole, passe par l'éditeur audio de la [leçon 4](04-audio.md).
```

## Étalonner

Choisis un plan. Les réglages de couleur sont dans l'inspecteur.

| Réglage | Effet |
|---|---|
| **Look** | Un point de départ : **Natural**, **Soft**, **Punchy**, **Black and white** |
| **Light**, **Contrast**, **Saturation** | Les réglages d'ensemble |
| Les roues **Shadows**, **Midtones**, **Highlights** | Glisse pour teinter. Double-clic pour remettre à zéro |
| La zone | **Whole image**, ou une forme : **Ellipse**, **Rectangle**, forme libre |
| **Copy grade**, **Paste** | Reporte l'étalonnage sur tous les plans choisis |

```etape
Choisis ton plan le plus réussi. Essaie le **Look** **Punchy**. Ajuste **Light** et **Contrast**.
```

```etape
Clique **Copy grade**. Choisis les autres plans et clique **Paste**. Tes plans ont maintenant la même couleur.
```

Le moniteur peut afficher des **Scopes** : forme d'onde, parade RVB, vectorscope, histogramme. Ils mesurent ce que l'œil juge mal.

**Apply a .cube LUT** applique un look ou le profil d'une caméra. **Grade in nodes** ouvre un graphe de compositing sur les couleurs du clip.

## Repères et chapitres

```etape
Lis le montage. À un moment important, appuie sur `M` : un repère se pose.
```

**Chapter** commence un chapitre à la tête de lecture. Clic droit sur sa bande pour le renommer ou le supprimer.

```quiz
Quel outil déplace une coupe sans changer la durée totale ?
- [ ] Le cutter (C)
- [x] Le déroulement (N)
- [ ] L'étirement (R)
explication: Le déroulement allonge un plan et raccourcit son voisin d'autant.
```

```quiz
Tu as étalonné un plan. Comment donner la même couleur aux autres ?
- [x] Copy grade, puis Paste sur les plans choisis.
- [ ] En les dupliquant.
- [ ] Ce n'est pas possible.
explication: Copy grade copie l'étalonnage du plan. Paste l'applique à toute la sélection.
```

```defi
Ouvre ton montage par un titre de trois secondes sur fond d'image, qui disparaît en fondu, puis enchaîne les plans en coupes franches avec une musique en fond.
indice: Le titre est un clip : il a son **Opacity** et ses clés. **Turn on animation**, puis baisse **Opacity** à la fin.
```

## Récapitulatif

- `N` déplace une coupe, `Y` change le moment montré, `T` pose un titre.
- L'onglet **Clip** règle le cadre, la vitesse, le son et la transition.
- **Look**, les trois roues et **Copy grade** donnent une couleur cohérente.

Leçon suivante : [Sous-titres, montage par le texte et export](03-sous-titres-texte-export.md)
