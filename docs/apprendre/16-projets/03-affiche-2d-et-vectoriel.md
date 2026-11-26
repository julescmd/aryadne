---
titre: Projet 3 : une affiche, du rendu au vectoriel
niveau: avancé
duree: 45 min
prerequis: 16-projets/01-objet-texture-et-rendu, 13-compositing/02-couleur-et-etalonnage, 04-vectoriel/01-premieres-formes
objectifs: Étalonner un rendu en nodes ; Composer une affiche sur un plan de travail ; Poser titre et formes vectorielles ; Exporter pour l'écran et pour l'impression
---

## Ce que tu vas réussir

Une affiche finie. Ton rendu du projet 1 en est l'image. Tu l'étalonnes en nodes, tu le poses sur une page, tu ajoutes un titre et des formes nettes, puis tu exportes en PNG et en PDF.

Il te faut l'image « vase » du [projet 1](01-objet-texture-et-rendu.md). À défaut, n'importe quelle image de ton coffre fait l'affaire.

## Étape 1 : étalonner l'image

Détail des gestes : [Couleur et étalonnage](../13-compositing/02-couleur-et-etalonnage.md).

```essayer
nouveau: arycomp
nom: projet-affiche-image
```

```etape
Glisse ton image « vase » depuis le panneau des fichiers dans le graphe. Relie-la à **Output**.
verifier: graphe.contient = image
```

```etape
Insère **Levels**. Pose les noirs avec **Black** et les blancs avec **White**.
verifier: graphe.contient = levels
```

```etape
Insère **Color balance**. Teinte **Shadows (lift)** vers le bleu, **Highlights (gain)** vers l'orangé. Reste léger.
verifier: graphe.contient = colorbalance
```

```etape
Insère **Vignette**, puis **Grain** en dernier, tous deux discrets.
verifier: graphe.contient = grain
```

```etape
Rends chaque node muet avec M, l'un après l'autre, pour juger ce qu'il apporte.
```

```etape
Enregistre avec Ctrl+S, puis clique **Export PNG**. L'image étalonnée est écrite à côté du graphe.
verifier: fichier.enregistre
```

## Étape 2 : la page

Détail des gestes : [Premières formes](../04-vectoriel/01-premieres-formes.md).

```essayer
nouveau: aryvec
nom: projet-affiche
```

```etape
Prends l'outil **Artboard** avec F. Trace un plan de travail, puis ouvre **Size…** et choisis **A4**.
verifier: fichier.ouvert = aryvec
```

```etape
Dans les réglages du plan, donne-lui une **Background color** sombre, prise dans ton image.
```

```etape
Active **Column grid** : 6 colonnes, avec une marge. Ces colonnes guident et aimantent la mise en page. Elles ne sont jamais exportées.
```

## Étape 3 : l'image

```etape
Glisse ton image étalonnée depuis le panneau des fichiers sur le plan de travail.
```

```etape
Avec l'outil **Select**, touche V, redimensionne-la et place-la dans le haut de la page. Laisse le tiers du bas libre pour le texte.
```

```astuce
Des repères magnétiques apparaissent pendant un déplacement : bords et centres des autres formes. Laisse-les t'aligner.
```

## Étape 4 : les formes

```etape
Prends l'outil **Rectangle** avec R. Trace un bandeau en bas de la page, d'un bord à l'autre.
verifier: calques >= 1
```

```etape
Dans **Fill**, choisis **Linear**. Règle le dégradé de la couleur du fond vers une teinte plus claire.
```

```etape
Prends l'outil **Ellipse** avec E. Tiens Maj et trace un cercle derrière le vase. Donne-lui une couleur chaude et une **Opacity** basse.
```

```etape
Le cercle choisi, appuie sur [ jusqu'à ce qu'il passe derrière l'image.
```

Si ton image a un fond opaque, le cercle disparaît derrière elle. Garde-le alors par-dessus, avec le mode de fusion **Screen** ou **Overlay**, dans **Appearance**.

## Étape 5 : le texte

```etape
Prends l'outil **Text** avec T. Clique sur le bandeau et écris ton titre.
```

```etape
Règle sa taille pour qu'il occupe la largeur de quatre colonnes. Choisis une couleur claire.
```

```etape
Clique de nouveau avec l'outil **Text**, plus bas. Écris une ligne plus petite : une date, un lieu, un nom.
```

```etape
Choisis le titre et le sous-titre. Dans l'alignement, clique **Left**, par rapport à **The selection**.
```

Deux tailles de texte, pas plus. Un seul alignement. C'est ce qui fait tenir une affiche.

## Étape 6 : les finitions

```etape
Choisis le titre. Dans **Appearance**, ajoute une **Drop shadow** très légère : il se détache de l'image.
```

```etape
Clique le bouton **Lucide icon library**. Cherche une icône en anglais, par exemple « flower ». Pose-la près du sous-titre.
```

L'icône arrive en tracés modifiables. Donne-lui la couleur du texte.

```etape
Recule : appuie sur Ctrl+0 pour tout cadrer. Plisse les yeux. Ce que tu vois encore, c'est la hiérarchie de ton affiche.
```

## Étape 7 : l'export

Détail des gestes : [Mode dev et export](../04-vectoriel/07-mode-dev-et-export.md).

```etape
Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```etape
Choisis le plan de travail. Dans la section **Export**, exporte en **PNG** : c'est la version pour l'écran.
```

```etape
Exporte ensuite en **PDF**. Mets **Bleed** à 3 mm et coche **Crop marks** : c'est la version pour l'imprimeur.
```

| Format | Pour |
|---|---|
| PNG | L'écran, les réseaux |
| PDF | L'impression. **Bleed**, **Crop marks**, **CMYK colors (print)** |
| SVG | Le web, ou une reprise dans un autre logiciel |

### D'autres formats, sans tout refaire

```etape
Le plan choisi, clique **Adapt…** et choisis **Story**. Une copie du plan est posée à côté, au format vertical.
```

Replace les éléments sur ce nouveau plan. Tu as l'affiche et sa version pour téléphone dans le même fichier.

## Ce que tu viens de faire

| Outil | Ce que tu y as fait |
|---|---|
| Compositing | Étalonner le rendu |
| Vectoriel | La page, l'image, les formes, le texte |
| Export | PNG pour l'écran, PDF pour l'impression |

Ton rendu, ton graphe d'étalonnage et ton affiche sont trois fichiers du coffre. Tu peux rouvrir chacun et le corriger.

```quiz
À quoi sert la grille de colonnes d'un plan de travail ?
- [x] À guider et aimanter la mise en page. Elle n'est jamais exportée.
- [ ] À découper l'image en morceaux.
- [ ] À compter les caractères du titre.
explication: Les colonnes sont une aide de composition. Elles n'apparaissent dans aucun export.
```

```quiz
Quel export pour un imprimeur ?
- [ ] PNG
- [x] PDF, avec fonds perdus et traits de coupe.
- [ ] SVG
explication: Le PDF porte Bleed et Crop marks. L'imprimeur coupe dans les fonds perdus.
```

```defi
Fais une série de trois affiches : même mise en page, trois images, trois couleurs.
indice: Duplique le plan de travail. Change l'image, puis utilise Recolor sur la sélection pour changer une couleur partout d'un coup.
```

## En trois lignes

- Le rendu s'étalonne en nodes, puis s'exporte en PNG.
- Un plan de travail, une grille, une image, un titre : l'affiche.
- PNG pour l'écran, PDF avec fonds perdus pour l'impression.

Tu as fini les projets guidés. Tu sais maintenant passer d'un outil à l'autre : c'est là qu'Aryadne prend tout son sens.
