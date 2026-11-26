---
titre: Préparer et premiers traits
niveau: intermédiaire
duree: 15 min
prerequis: 07-3d-bases/02-selectionner-et-transformer
objectifs: Donner assez de sommets à un maillage ; Entrer en sculpture ; Régler rayon et force ; Naviguer en sculptant ; Annuler un trait
---

## Ce que tu vas réussir

Tu vas préparer une boule d'argile, y poser tes premiers traits, creuser, lisser et annuler. À la fin, la barre des pinceaux n'a plus de secret.

```essayer
nouveau: aryscene
nom: premiere-sculpture
```

## Le piège du débutant

Un pinceau ne peut déplacer que les sommets qui existent. Sur un cube à 8 sommets, il ne se passe presque rien.

Trois façons d'avoir assez de sommets :

- le modificateur **Remesh (voxels)**, puis **Apply** ;
- les niveaux de multirésolution, à la leçon 3 ;
- le remaillage dynamique, à la leçon 4.

## Prépare la boule

```etape
Ajoute une **Sphere** par le menu **Add**. Supprime le cube de départ.
verifier: scene.objets >= 3
```

```etape
Choisis la sphère. Dans l'onglet **Modifiers**, ajoute **Remesh (voxels)**. Baisse **Voxel size** jusqu'à voir une surface fine et régulière.
verifier: scene.selection
```

```etape
Clique **Apply** sur le modificateur. Les quads réguliers deviennent le maillage de base.
```

**Remesh (voxels)** donne une topologie égale partout, sans trous ni faces qui se croisent. C'est l'argile idéale.

## Entre en sculpture

```etape
Dans la barre de la vue, passe du mode **Object mode** au mode **Sculpt**.
```

Un cercle suit ta souris : c'est le pinceau. En bas de la vue, une barre porte les outils.

De gauche à droite : les sept pinceaux, le rayon, la force, la symétrie, les niveaux de multirésolution, le remaillage dynamique, et la croix pour sortir.

```attention
La sculpture travaille sur le même maillage que le mode édition : un fichier OBJ à côté de la scène. Le modèle d'origine n'est pas touché.
```

## Premier trait

```etape
Glisse sur la sphère avec le clic gauche. La surface gonfle sous le pinceau **Draw**.
```

```etape
Tiens Ctrl et glisse. Le cercle change de couleur et le pinceau creuse au lieu d'ajouter.
```

```etape
Tiens Maj et glisse sur ton trait. Il s'adoucit : Maj passe au lissage, quel que soit le pinceau choisi.
```

Ces trois gestes couvrent déjà beaucoup : ajouter, creuser, lisser.

## Rayon et force

| Réglage | Geste |
|---|---|
| Rayon | Les touches [ et ], ou le champ en pixels de la barre |
| Force | Le curseur de la barre |

```etape
Appuie plusieurs fois sur ]. Le cercle grandit. Fais un trait large.
```

```etape
Appuie sur [ pour le réduire. Baisse la force. Fais un trait fin et léger.
```

Le rayon est une taille à l'écran. Zoomer rend donc le pinceau plus fin par rapport au modèle.

```astuce
Commence gros et fort pour les masses. Finis petit et doux pour les détails. Changer de taille en cours de route est normal : garde un doigt sur [ et ].
```

## Tourner autour du modèle

Le clic gauche sculpte. Pour bouger la vue :

| Geste | Effet |
|---|---|
| Molette enfoncée, ou Alt + glisser | Tourner |
| Maj + molette enfoncée | Déplacer la vue |
| Molette | Zoom |

```etape
Tourne autour de la sphère, puis sculpte l'autre côté.
```

Regarde ta forme sous tous les angles, sans arrêt. Une forme juste de face est souvent fausse de profil.

## Annuler

```etape
Fais un trait raté exprès. Annule-le avec Ctrl+Z. Rétablis-le avec Ctrl+Maj+Z.
```

Chaque trait est une étape d'annulation.

## Sortir

```etape
Clique la croix de la barre : **Leave sculpting**. Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```quiz
Tu sculptes un cube neuf et rien ne bouge. Pourquoi ?
- [ ] Le pinceau est trop grand.
- [x] Le cube n'a que 8 sommets : il n'y a presque rien à déplacer.
- [ ] Il faut d'abord enregistrer.
explication: Un pinceau déplace des sommets. Sans sommets, pas de sculpture. Remesh (voxels) en fournit.
```

```quiz
Quelle touche, maintenue pendant un trait, lisse la surface ?
- [ ] Ctrl
- [x] Maj
- [ ] Alt
explication: Maj passe au lissage avec n'importe quel pinceau. Ctrl inverse le pinceau, Alt + glisser tourne la vue.
```

```defi
Sculpte une pomme : un creux en haut, un creux plus petit en bas, une forme un peu irrégulière.
indice: Ctrl avec le pinceau Draw pour les creux, Maj pour lisser. Tourne souvent autour.
```

## En trois lignes

- Sans sommets, pas de sculpture : **Remesh (voxels)**, puis **Apply**.
- Clic gauche : ajouter. Ctrl : creuser. Maj : lisser.
- [ et ] règlent le rayon. La molette enfoncée tourne la vue.

[Suite : Les sept pinceaux](02-les-sept-pinceaux.md)
