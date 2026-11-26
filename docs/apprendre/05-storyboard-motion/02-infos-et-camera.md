---
titre: Les informations d'une case et la caméra 2D
niveau: débutant
duree: 15 min
prerequis: 05-storyboard-motion/01-premier-storyboard
objectifs: Noter la scène, la valeur de plan et le mouvement de caméra ; Donner une durée à chaque case ; Écrire l'action et le dialogue ; Animer un cadre sur le dessin avec la caméra 2D
---

## Ce que tu vas réussir à faire

Tu vas transformer tes dessins en vrai document de tournage : chaque case dira quel plan, quelle caméra, combien de temps, et ce qui s'y dit. Puis tu feras un travelling sur un dessin fixe.

```essayer
nouveau: arystory
nom: decoupage
```

```etape
Clique **Try it yourself**. Le storyboard arrive avec trois cases vides. Dessine vite dans chacune : un paysage large, un personnage, un visage.
verifier: onglet = video
```

## Le panneau de la case

À droite du dessin, chaque case a ses champs.

| Champ | Contenu |
|---|---|
| **Scene** | La séquence ou la scène : « 1 », « INT. CUISINE – JOUR » |
| La valeur de plan | Plan d'ensemble, plan large, plan moyen, plan américain, plan rapproché, gros plan, très gros plan, plongée, contre-plongée |
| **Camera** | Fixe, panoramique, travelling avant, arrière ou latéral, zoom, caméra à l'épaule, grue |
| **Duration** | La durée de la case dans l'animatique, en secondes |
| **Action** | Ce qui se passe à l'image |
| **Dialogue** | Les répliques, la voix off |
| **Notes** | Le son, les effets, les transitions |

```attention
Les listes de valeurs de plan et de mouvements de caméra sont fixes. Plusieurs de leurs libellés restent en français, même quand l'interface est en anglais.
```

## Remplir la première case

```etape
Va sur la première case. Dans **Scene**, tape `EXT. PLAGE – MATIN`.
```

```etape
Choisis la valeur de plan d'ensemble. Laisse la caméra fixe.
```

```etape
Règle **Duration** sur 4 secondes. Dans **Action**, écris ce qu'on voit.
```

Le panneau affiche l'instant où commence la case et la durée totale du storyboard.

## Les cases suivantes

```etape
Va sur la deuxième case avec `→`. Tape la même scène. Choisis un plan moyen, 2 secondes.
```

```etape
Sur la troisième case : un gros plan, 1,5 seconde. Écris une réplique dans **Dialogue**.
```

Les cases d'une même scène se suivent. Quand le champ **Scene** change, une nouvelle scène commence. Ces changements deviendront des repères au montage.

```astuce
Lis tes durées à voix haute en regardant chaque case. Si tu as fini de lire l'image avant la fin du temps, la case est trop longue.
```

## La caméra 2D

Une case est un dessin fixe. La caméra 2D y ajoute un mouvement : un pano, un zoom ou un travelling **sur le dessin**. Le cadre passe d'un cadre A à un cadre B pendant la durée de la case.

```etape
Reviens sur la première case, le paysage. Coche **2D camera**. Deux cadres apparaissent sur le dessin : A, le départ, et B, l'arrivée.
```

```etape
Laisse le cadre A large, sur tout le paysage.
```

```etape
Glisse le cadre B sur un détail du paysage. Resserre-le avec la molette.
```

Le bouton à côté de **2D camera** ouvre et ferme ce réglage des cadres. Ferme-le pour dessiner de nouveau.

Tu viens de faire un travelling avant. Il se verra à la lecture de l'animatique, dans l'export MP4 et au montage.

```etape
Mets le champ **Camera** de cette case sur le travelling avant, pour que le document dise la même chose que l'image.
```

| Mouvement voulu | Cadre A | Cadre B |
|---|---|---|
| Zoom avant | Large | Serré, au même endroit |
| Panoramique | À gauche | À droite, même taille |
| Travelling arrière | Serré sur un détail | Large |

```attention
La caméra 2D va de A à B en ligne droite. Il n'y a pas de trajectoire à plusieurs points. Pour un mouvement en deux temps, fais deux cases.
```

## Changer le format de l'image

Dans la barre du storyboard, le bouton **Image format** change le format de l'image de toutes les cases.

## Depuis un scénario

Un scénario `.fountain` s'écrit dans Aryadne comme un texte. Il donne les scènes, les personnages et un minutage estimé. Le lien avec un storyboard n'est pas automatique : recopie les intitulés de scène dans le champ **Scene**.

```quiz
À quoi sert le champ Duration d'une case ?
- [ ] À régler la taille du dessin.
- [x] À fixer le temps que la case reste à l'écran dans l'animatique.
- [ ] À compter les traits.
explication: Chaque case a sa durée. Leur somme donne la durée du film.
```

```quiz
Que fait la caméra 2D ?
- [ ] Elle tourne autour d'un objet 3D.
- [x] Elle déplace un cadre de A à B sur le dessin, pendant la durée de la case.
- [ ] Elle filme l'écran.
explication: C'est un mouvement de cadre sur une image fixe : pano, zoom ou travelling.
```

```defi
Fais un panoramique sur un dessin large : un personnage à gauche, ce qu'il regarde à droite.
indice: Dessine la case en pensant large. Cadre A sur le personnage, cadre B de même taille sur l'objet.
```

## Récapitulatif

- Chaque case porte sa scène, son plan, sa caméra, sa durée, son action et son dialogue.
- **2D camera** fait passer le cadre de A à B sur le dessin.
- Un changement dans le champ **Scene** marque une nouvelle scène.

Leçon suivante : [L'animatique, la couleur et l'export](03-animatique-et-export.md)
