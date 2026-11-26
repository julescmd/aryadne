---
titre: L'affichage de la vue
niveau: débutant
duree: 10 min
prerequis: 07-3d-bases/01-ta-premiere-scene
objectifs: Changer le matériau de la vue ; Afficher le fil de fer ; Choisir un environnement ; Déplacer la lumière de la vue ; Passer du moteur rapide au tracé de chemins
---

## Ce que tu vas réussir

Tu vas apprendre à regarder ton modèle de plusieurs façons : en argile pour juger la forme, en fil de fer pour juger le maillage, en lumière réaliste pour juger l'image.

```essayer
nouveau: aryscene
nom: affichage
```

```etape
Ajoute une **Sphere** et un **Torus** à côté du cube. Trois formes valent mieux qu'une pour juger un affichage.
verifier: scene.objets >= 5
```

## L'onglet World

```etape
Dans **Properties**, ouvre l'onglet **World**. Il règle l'environnement et l'éclairage de la vue.
```

## Le matériau de la vue

Il s'applique aux objets qui n'ont pas encore de matériau à eux.

```etape
Essaie tour à tour **Clay**, **White**, **Chrome** et **Normals**.
```

| Matériau | À quoi il sert |
|---|---|
| **Clay** | Juger la forme, comme en sculpture |
| **White** | Juger l'éclairage |
| **Chrome** | Voir les défauts de surface dans les reflets |
| **Normals** | Voir l'orientation des faces |

```etape
Coche **Smooth shading**. Les facettes de la sphère disparaissent. Décoche : elles reviennent.
```

L'ombrage lisse ne change pas la forme. Il change seulement la façon dont la lumière est calculée.

## Le fil de fer

```etape
Dans **Wireframe**, monte **Opacity**. Les arêtes se dessinent par-dessus les objets.
```

```etape
Coche **Wireframe only**. Il ne reste que les arêtes. Décoche ensuite.
```

Le fil de fer montre comment un objet est construit. Tu t'en serviras sans cesse en modélisation.

## L'environnement

L'environnement est une image qui entoure la scène. Elle l'éclaire et s'y reflète.

```etape
Repasse le matériau de la vue sur **Chrome**. Dans **Environment (HDRI)**, essaie **Studio**, puis **Outdoor**.
```

```etape
Coche **Show HDRI as background**. L'environnement s'affiche derrière la scène.
```

```etape
Dans la vue, maintiens H et glisse. L'environnement tourne, et les reflets avec lui.
```

**Load a .hdr / .exr…** charge ton propre environnement. **Intensity** et **Exposure (EV)** dosent sa force.

## La lumière de la vue

```etape
Dans la vue, maintiens L et glisse. La lumière directionnelle de la vue tourne autour des objets.
```

Ses réglages sont dans **Directional light (L + drag)** : **Intensity**, **Direction**, **Height**, **Color**.

```etape
Clique **Reset lighting** pour revenir aux valeurs de départ.
```

```raccourci
L + glisser : déplacer la lumière de la vue. H + glisser : tourner l'environnement.
```

```attention
La scène de départ contient une lumière. Tant qu'une scène a ses propres lumières, ce sont elles qui éclairent, pas la lumière de la vue.
```

## Deux moteurs

```etape
Dans la barre de la vue, ouvre **Render**. Sous **Viewport engine**, choisis **Path tracing (realistic)**.
```

L'image part bruitée, puis s'affine quand la vue ne bouge plus. La lumière rebondit, les reflets sont vrais.

| Moteur | Sert à |
|---|---|
| **Preview (fast)** | Travailler. Instantané, ombres et reflets approchés |
| **Path tracing (realistic)** | Juger l'image |

```etape
Reviens sur **Preview (fast)**. C'est le moteur de travail.
```

Le rendu a sa leçon : [Rendu](../15-animation-rendu/07-rendu.md).

## Une image vite faite

Le menu **Capture** enregistre ce que tu vois : **PNG image (2400 px)**, ou **Turntable GIF (48 frames)** pour montrer un modèle qui tourne. Les fichiers sont enregistrés à côté du modèle.

```quiz
Que change Smooth shading ?
- [ ] Le nombre de faces du modèle.
- [x] Le calcul de la lumière : les facettes ne se voient plus.
- [ ] La couleur du modèle.
explication: La forme est la même. Seul l'ombrage est adouci entre les faces.
```

```quiz
Quel moteur choisir pour travailler vite ?
- [x] Preview (fast)
- [ ] Path tracing (realistic)
- [ ] Les deux en même temps.
explication: L'aperçu rapide reste fluide. Le tracé de chemins sert à juger l'image finale.
```

```defi
Fabrique une image de présentation de tes trois formes : chrome, environnement Outdoor en fond, fil de fer léger.
indice: Tout est dans l'onglet World. Finis par Capture, PNG image.
```

## En trois lignes

- L'onglet **World** règle matériau de la vue, fil de fer, ombres et environnement.
- L et H, maintenus en glissant, tournent la lumière et l'environnement.
- **Preview (fast)** pour travailler, **Path tracing (realistic)** pour juger.

[Suite : La timeline](06-la-timeline.md)
