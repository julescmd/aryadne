---
titre: Les coutures
niveau: intermédiaire
duree: 20 min
prerequis: 10-uv-textures/01-deplier-les-uv
objectifs: Placer des coutures à la main ; Utiliser les coutures automatiques ; Régler le compromis entre îlots et étirement ; Déplier avec ses propres coutures ; Épingler et redéplier
---

## Ce que tu vas réussir

Tu vas choisir toi-même où ton modèle se coupe. Tu cacheras les coutures là où personne ne regarde.

```essayer
nouveau: aryscene
nom: coutures
```

```etape
Ajoute un **Cylinder**. Choisis-le, passe en mode **UV & materials** et ouvre l'onglet **Seams**.
verifier: scene.objets >= 4
```

## Où mettre une couture

Une couture se voit toujours un peu : la texture y casse. Trois règles.

1. Cache-la : sous l'objet, derrière, dans un creux, le long d'une arête vive.
2. Mets-en assez pour que chaque morceau se pose à plat.
3. N'en mets pas plus.

## À la main

| Geste | Effet |
|---|---|
| Clic sur une arête | La choisir |
| Maj+clic | Ajouter |
| Alt+clic | Toute la boucle |
| Ctrl+clic | Le chemin le plus court depuis la dernière arête |
| Clic droit, ou Ctrl+E | Le menu des coutures |

Un cylindre se déplie comme une boîte de conserve : deux couvercles, et un corps qu'on ouvre d'un trait.

```etape
Fais un Alt+clic sur une arête du bord du haut. Toute la boucle est choisie.
```

```etape
Fais un clic droit et choisis **Mark seam**.
```

```etape
Recommence pour le bord du bas : Alt+clic, puis **Mark seam**.
```

```etape
Clique une arête verticale à l'arrière du cylindre. Fais un Ctrl+clic sur l'arête verticale du même montant, à l'autre bout. Le chemin est choisi. Marque-le en couture.
```

```etape
Clique **Unwrap with these seams**. L'onglet **UV** s'ouvre sur le résultat : deux disques et un rectangle.
```

```etape
Clique **Checker**. Les carrés sont réguliers sur le corps du cylindre.
```

## Automatiquement

Reviens à l'onglet **Seams**. Cinq boutons posent des coutures pour toi.

| Bouton | Ce qu'il fait |
|---|---|
| **Automatic seams** | La coupe minimale pour que chaque îlot se déplie à plat. Les coutures vont dans les plis et les creux |
| **Smart seams** | Regroupe les faces en îlots presque plats, puis coupe sur leurs bords |
| **On sharp edges** | Une couture sur chaque arête plus vive que **Sharp angle** |
| **Selection outline** | Une couture autour des faces choisies |
| **Seams from islands** | Recrée les coutures d'après les UV existantes |

```etape
Clique **Automatic seams**. Compare avec tes coutures : sur un cylindre, l'outil trouve aussi une seule couture droite pour le corps.
```

## Régler l'automatique

| Réglage | Effet |
|---|---|
| Curseur **Fewer islands** / **Less stretching** | Peu d'îlots, quitte à étirer, ou beaucoup d'îlots sans étirement |
| **Always cut beyond (°)** | Une arête plus vive que cet angle est toujours coupée |
| **Separate materials** | Chaque ID de matériau a ses propres îlots |
| **Hide seams (hollows, underside)** | Préfère placer les coutures là où on ne les voit pas |

```etape
Pousse le curseur vers **Less stretching** et relance **Automatic seams**. Les îlots sont plus nombreux.
```

```etape
Pousse-le vers **Fewer islands** et relance. Il y en a moins, avec plus d'étirement.
```

```astuce
Objet mécanique : **On sharp edges**, avec un angle vers 60 degrés. Objet organique : **Automatic seams**, avec **Hide seams** coché.
```

## Épingler et redéplier

Dans l'éditeur UV, tu peux forcer la forme d'un îlot.

| Touche | Effet |
|---|---|
| P | Épingler les sommets choisis |
| Alt+P | Les désépingler |
| U | Déplier la sélection, en gardant les épingles |
| V | Souder |

```etape
Ouvre l'onglet **UV**. Dans l'éditeur UV, choisis deux sommets d'un îlot et appuie sur P.
```

```etape
Déplace un des sommets épinglés avec G, puis appuie sur U. L'îlot se redéplie autour de tes épingles.
```

Avec **Live unwrap** activé, le redépliage se fait pendant que tu déplaces l'épingle.

## Le menu de l'éditeur UV

Le clic droit dans l'éditeur UV propose : **Align horizontally**, **Straighten as grid (quads)**, **Weld (same vertex)**, **Average island scale**, le miroir et la rotation.

**Straighten as grid (quads)** redresse un îlot de quads en grille parfaite. C'est idéal pour une sangle, une route, un tuyau.

```etape
Reviens à la scène avec **Back to the scene** et enregistre.
verifier: fichier.enregistre
```

```quiz
Où placer de préférence une couture ?
- [ ] Au milieu de la face la plus visible.
- [x] Dans un creux, sous l'objet, ou le long d'une arête vive.
- [ ] Peu importe.
explication: Une texture casse toujours un peu à la couture. Mieux vaut la cacher.
```

```quiz
Que fait U dans l'éditeur UV ?
- [x] Il déplie la sélection en gardant les sommets épinglés.
- [ ] Il annule le dépliage.
- [ ] Il range les îlots.
explication: P épingle, U redéplie autour des épingles. Le rangement, c'est Pack.
```

```defi
Déplie un cube comme un patron de dé : une croix d'un seul tenant, six faces.
indice: Marque sept arêtes en coutures, pas une de plus. Les cinq arêtes restantes tiennent la croix.
```

## En trois lignes

- Une couture est une arête où la surface se coupe. Cache-la.
- **Automatic seams** et son curseur règlent le compromis îlots contre étirement.
- P épingle, U redéplie, **Unwrap with these seams** déplie avec tes coutures.

[Suite : Rangement, densité et UDIM](03-rangement-densite-udim.md)
