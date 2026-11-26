---
titre: Le mode édition
niveau: débutant
duree: 15 min
prerequis: 07-3d-bases/02-selectionner-et-transformer
objectifs: Entrer et sortir du mode édition ; Passer entre sommets, arêtes et faces ; Sélectionner boucles et chemins ; Déplacer une partie du maillage
---

## Ce que tu vas réussir

Tu vas ouvrir un cube et en changer la forme en déplaçant ses sommets, ses arêtes et ses faces. À la fin, ton cube sera une petite maison.

```essayer
nouveau: aryscene
nom: mode-edition
```

## Entrer

```etape
Clique le cube, puis appuie sur Tab. La barre de la vue indique **Edit mode**.
verifier: scene.selection
```

Seuls les objets à maillage ont un mode édition. Lumières et caméras n'en ont pas.

```attention
En entrant en mode édition, le maillage devient un fichier OBJ à côté de la scène. Le modèle d'origine n'est pas touché.
```

## Trois façons de voir un maillage

| Touche | Mode | Tu choisis |
|---|---|---|
| 1 | **Vertices** | Des points |
| 2 | **Edges** | Des arêtes |
| 3 | **Faces** | Des faces |

```etape
Appuie sur 1, 2, puis 3. Les éléments surlignés changent.
```

```attention
En mode édition, 1, 2 et 3 choisissent le mode de sélection. Ils ne changent plus la vue comme en mode objet.
```

## Déplacer une face

```etape
En mode **Faces**, clique la face du dessus du cube.
```

```etape
Appuie sur G, puis sur l'axe vertical, et monte la face. Le cube s'allonge. Valide d'un clic.
```

G, R et S marchent comme en mode objet. Ils n'agissent que sur ce qui est choisi.

## Le toit : une arête

```etape
Passe en mode **Edges** avec 2. Clique une arête du dessus.
```

```etape
Monte-la avec G et l'axe vertical. Le dessus du cube s'incline : tu as un pan de toit.
```

Annule avec Ctrl+Z. Tu feras un vrai toit à la leçon suivante.

## Sélectionner plus

| Geste | Effet |
|---|---|
| Maj+clic | Ajouter |
| Glisser | Rectangle |
| A, Alt+A | Tout, rien |
| Ctrl+I | Inverser |
| Alt+clic | Toute la boucle d'arêtes |
| Ctrl+clic | Le chemin le plus court depuis le dernier élément |
| Ctrl + | Agrandir la sélection |
| Ctrl − | La rétrécir |
| L | La pièce reliée sous le pointeur |

```etape
En mode **Edges**, fais un Alt+clic sur une arête du haut. Toute la boucle autour du cube est choisie.
```

```etape
Appuie sur S et resserre la boucle. Le haut du cube rétrécit.
```

```etape
Appuie sur A pour tout choisir, puis sur Alt+A pour ne rien choisir.
```

## Voir à travers

Par défaut, tu ne choisis que ce que tu vois.

```etape
Clique **X-ray** dans la barre, ou appuie sur Alt+Z. Les sommets cachés deviennent visibles et sélectionnables.
```

```etape
En mode **Vertices**, trace un rectangle autour du haut du cube, vu de face. Avec **X-ray**, les quatre sommets du haut sont pris, y compris ceux de derrière.
```

## Glisser le long des arêtes

```etape
Choisis un sommet. Appuie sur G, puis encore sur G. Le sommet glisse le long de ses arêtes au lieu de quitter la surface.
```

C'est **Slide**. Il déplace sans changer la silhouette.

## Sortir

```etape
Appuie sur Tab. Tu es revenu en mode objet.
```

```raccourci
Tab : entrer en mode édition, ou en sortir.
```

L'historique d'annulation est propre au mode édition. Ctrl+Z y défait tes gestes de modélisation.

```quiz
En mode édition, que fait la touche 2 ?
- [ ] Elle passe en vue de dessous.
- [x] Elle passe en sélection d'arêtes.
- [ ] Elle duplique la sélection.
explication: En mode édition, 1, 2 et 3 choisissent sommets, arêtes ou faces.
```

```quiz
Comment choisir d'un coup toute une boucle d'arêtes ?
- [ ] Ctrl+clic
- [x] Alt+clic
- [ ] Maj+clic
explication: Alt+clic prend la boucle. Ctrl+clic prend le chemin le plus court depuis la dernière sélection.
```

```defi
Transforme le cube en pyramide tronquée : large en bas, étroite en haut.
indice: Mode Faces, la face du dessus, puis S pour la réduire.
```

## En trois lignes

- Tab entre en mode édition. 1, 2, 3 choisissent sommets, arêtes, faces.
- G, R, S agissent sur la sélection. G deux fois fait glisser.
- Alt+clic prend une boucle. **X-ray** laisse choisir ce qui est caché.

[Suite : Extruder, incruster, biseauter](02-extruder-incruster-biseauter.md)
