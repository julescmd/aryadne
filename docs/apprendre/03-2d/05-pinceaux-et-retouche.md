---
titre: Pinceaux et retouche
niveau: intermédiaire
duree: 15 min
prerequis: 03-2d/02-calques
objectifs: Choisir un pinceau prêt ; Régler le lissage, le grain et la pression ; Dessiner en symétrie et en perspective ; Corriger une photo au tampon et au correcteur
---

## Ce que tu vas réussir à faire

Tu vas dessiner avec un crayon qui accroche le papier, tracer un motif en symétrie, puis effacer une tache sur une photo.

```essayer
nouveau: aryimg
nom: pinceaux
```

```etape
Clique **Essaie par toi-même**. Prends le pinceau avec `B`.
verifier: fichier.ouvert = aryimg
```

## Les pinceaux prêts

Dans la barre du haut, le bouton à l'icône de pinceau ouvre les pinceaux prêts. Chacun règle d'un coup la taille, la dureté, le flux, le grain, la pression et le lissage.

| Pinceau | Caractère |
|---|---|
| **Pencil** | Un crayon fin, avec du grain |
| **Ink** | Un trait net pour l'encrage |
| **Marker** | Un feutre |
| **Chalk** | Une craie qui accroche |
| **Airbrush** | Un voile doux |
| **Dry paint** | Une peinture sèche |
| **Round brush** | Le pinceau rond de base |

```etape
Ouvre les pinceaux prêts. Choisis **Pencil**. Trace quelques lignes.
```

```etape
Choisis **Chalk**. Trace un trait large. Compare avec le crayon.
```

## Les réglages un par un

| Réglage | Effet |
|---|---|
| **Size** | Le diamètre |
| **Hardness** | La dureté du bord |
| Le flux | La quantité de peinture déposée à chaque passage |
| **Opacity** | L'opacité du trait entier |
| **Smoothing** | Le stabilisateur |
| **Grain** | Le grain du papier : le pinceau accroche comme un crayon ou une craie |

```etape
Reviens à **Ink**. Monte **Smoothing**. Trace une longue courbe lentement : la ligne est plus propre.
```

```astuce
Opacité ou flux ? L'opacité plafonne tout le trait. Le flux s'accumule quand tu repasses au même endroit. Pour modeler une ombre en plusieurs passages, baisse le flux.
```

## La pression de la tablette

Deux boutons relient la pression du stylet au trait. Survole-les pour lire leur nom.

- **Tablet pressure controls size** : plus tu appuies, plus le trait est large.
- **Tablet pressure controls opacity** : plus tu appuies, plus le trait est opaque.

Sans tablette, ces boutons ne changent rien.

## Dessiner en symétrie

Deux boutons de symétrie reflètent ton trait : de l'autre côté de l'axe vertical, ou de l'axe horizontal.

```etape
Active la symétrie verticale. Dessine la moitié d'un visage ou d'un vase : l'autre moitié se trace toute seule.
```

```etape
Active aussi la symétrie horizontale. Dessine un motif dans un coin : il se répète dans les quatre.
```

```etape
Désactive les deux symétries.
```

## Des textures qui se raccordent

**Wrap around** répète l'image autour d'elle-même. Un trait qui sort par un bord revient par le bord opposé. C'est ce qu'il faut pour peindre une texture qui se répète sans couture.

```etape
Active **Wrap around**. Trace un trait qui sort par la droite : il revient par la gauche. Désactive ensuite.
```

## Tracer en perspective

**Perspective** pose des points de fuite sur la toile. Tu les déplaces à la souris. **Snap strokes to the vanishing lines** aimante tes traits sur leurs lignes.

```etape
Active **Perspective**. Place un point de fuite. Active l'aimantation et trace quelques lignes : elles filent toutes vers le point.
```

```etape
Désactive **Perspective**.
```

## Retoucher une photo

```etape
Ajoute une photo avec **Layer ▾ › Layer from an image…**.
verifier: calques >= 2
```

| Outil | Touche | Usage |
|---|---|---|
| **Clone stamp** | `S` | Recopie une zone ailleurs. `Alt+clic` choisit la source |
| **Spot healing** | `J` | Peins sur une tache : elle s'efface |
| **Smudge** | `R` | Étale la peinture. Le flux règle la force |
| **Liquify** | `U` | Pousse les pixels pour déformer |

```etape
Appuie sur `J`. Peins sur un petit défaut de la photo : une poussière, un bouton, un fil.
```

```etape
Appuie sur `S`. Fais `Alt+clic` sur une zone propre. Peins ensuite sur une zone à couvrir : la source est recopiée.
```

```astuce
Le correcteur marche bien sur les petits défauts entourés d'une matière régulière. Pour un objet entier, ou près d'un bord net, le tampon te donne le contrôle.
```

```etape
Appuie sur `R`. Étale un contour de la photo du bout du pinceau. Annule avec `Ctrl+Z`.
```

## Poser du texte

L'outil **Text** (`T`) pose du texte sur l'image.

```attention
Une fois posé, le texte devient des pixels. Tu ne peux plus le corriger au clavier. Pour un titre que tu veux garder modifiable, passe par le [dessin vectoriel](../04-vectoriel/index.md).
```

```quiz
Comment choisir la source du tampon de duplication ?
- [ ] Avec un clic droit.
- [x] Avec Alt+clic.
- [ ] Avec la pipette.
explication: Alt+clic fixe la zone à recopier. Tu peins ensuite là où tu veux la poser.
```

```quiz
À quoi sert Wrap around ?
- [ ] À tourner la vue.
- [x] À peindre une texture qui se raccorde d'un bord à l'autre.
- [ ] À courber un calque.
explication: L'image se répète autour d'elle-même. Un trait qui sort d'un côté rentre par l'autre.
```

```defi
Peins une texture de pavés qui se répète sans couture visible.
indice: Active **Wrap around** et dessine des pavés qui traversent les bords. Désactive-le, puis vérifie que les bords gauche et droit se répondent.
```

## Récapitulatif

- Les pinceaux prêts règlent tout d'un coup. **Smoothing** stabilise, **Grain** donne la matière.
- Symétrie, **Wrap around** et **Perspective** guident le trait.
- `J` efface une tache, `S` recopie une zone après `Alt+clic`.

Leçon suivante : [Réglages, styles de calque et export](06-reglages-styles-export.md)
