---
titre: Rendu et export
niveau: intermédiaire
duree: 20 min
prerequis: 07-3d-bases/04-lumieres-et-camera, 07-3d-bases/05-affichage-de-la-vue
objectifs: Régler le tracé de chemins ; Rendre une image finale ; Enregistrer les passes en EXR ; Sortir une animation en vidéo ; Exporter un modèle
---

## Ce que tu vas réussir

Tu vas tirer d'une scène une image finale propre, puis une vidéo de son animation, puis un fichier de modèle pour un autre logiciel.

```essayer
nouveau: aryscene
nom: rendu
```

```etape
Ajoute une **Sphere** et un **Torus** à côté du cube. Place-les pour composer une petite nature morte.
verifier: scene.objets >= 5
```

## Deux moteurs, un rendu final

| Moteur | Sert à |
|---|---|
| **Preview (fast)** | Travailler. Ombres et reflets approchés |
| **Path tracing (realistic)** | Juger l'image. La lumière rebondit |

Les deux lisent les mêmes matériaux, les mêmes lumières et la même peinture.

```etape
Dans la barre de la vue, ouvre **Render**. Sous **Viewport engine**, choisis **Path tracing (realistic)**.
```

L'image part bruitée. Chaque passe ajoute un échantillon par pixel, et l'image s'affine quand la vue ne bouge plus.

## Présenter un modèle : le studio

Le menu **Render** règle le tracé de chemins.

```etape
Supprime la lumière de la scène. Dans **Render lighting**, choisis **Studio**.
```

**Render lighting** sert quand la scène n'a pas ses propres lumières.

| Choix | Effet |
|---|---|
| **View** | La lumière de la vue |
| **Studio** | Clé, contre-jour et débouchage, sur un fond gris |
| **Outdoor** | Un soleil franc et le ciel de l'environnement |

```etape
Coche **Floor (shadows and bounces)**, **Visible floor** et **Studio backdrop (the floor fades into the distance)**.
```

Un sol infini reçoit les ombres et renvoie la lumière. Avec le fond de studio, il se fond dans le lointain : plus de ligne d'horizon.

## Les réglages qui comptent

| Réglage | Effet |
|---|---|
| **Samples** | Le nombre de passes. Plus haut : moins de bruit, plus long |
| **Bounces** | Le nombre de rebonds de la lumière. Plus haut : intérieurs mieux éclairés |
| **Denoise** | Débruite l'image |
| **Firefly clamp** | Plafonne les points blancs isolés |
| **Colors** | **AgX** ou **Filmic** : la conversion des couleurs pour l'affichage |
| **Graphics card** | Calcule sur la carte graphique, si elle est là |

```etape
Mets **Samples** à 256 et coche **Denoise**.
```

| Problème | Geste |
|---|---|
| Image trop bruitée | Monte **Samples**, ou coche **Denoise** |
| Points blancs isolés | Baisse **Firefly clamp** |
| Intérieur trop sombre | Monte **Bounces** |
| La vue rame quand tu tournes | Décoche **Path tracing while moving** |

```astuce
Le bouton **Install the AI denoiser (~65 MB)** installe Intel Open Image Denoise. Une fois installé, il prend le relais du débruiteur maison.
```

## La profondeur de champ

```etape
Monte un peu **Depth of field**. Règle **Focus distance** jusqu'à ce que l'objet du milieu soit net.
```

L'avant et l'arrière deviennent flous. À 0, tout est net.

## Un trait de dessin

```etape
Monte **Outlines (line art)** à 2 pixels. Un contour se trace autour des formes. Remets 0 si tu préfères sans.
```

## Le rendu final

Le point de vue du rendu est celui de la vue 3D. Pour rendre depuis la caméra, regarde d'abord par elle.

```etape
Appuie sur 0 pour regarder par la caméra. Si le cadre ne te plaît pas, trouve ta vue, choisis la caméra et clique **Set from view**.
```

```etape
Appuie sur F12. La fenêtre du rendu s'ouvre. Le menu **Render**, puis **Render image…**, fait pareil.
```

```etape
Choisis la taille **1080p**. Règle **Samples** et coche **Denoise**. Clique **Render**.
```

Tailles proposées : **720p**, **1080p**, **4K**, portrait, carré, ou des valeurs libres. **Stop** arrête le calcul.

```etape
À la fin, clique **Save…** pour enregistrer l'image en PNG.
```

| Bouton | Effet |
|---|---|
| **Save…** | Enregistre en PNG |
| **Multilayer EXR (passes)** | Enregistre les passes dans un EXR multicouche, pour le compositing |
| Envoyer | Envoie l'image vers un autre outil : retouche, board, montage |
| **Close** | Ferme |

Les passes se relisent dans un graphe d'image : voir [Incrustation et passes de rendu](../13-compositing/04-incrustation-et-passes.md).

## Une animation en vidéo

```etape
Choisis le tore. Pose une clé à l'image 1. À l'image 48, tourne-le d'un demi-tour et pose une clé. Dans l'onglet **Scene**, mets **End** à 48.
verifier: lecture.image >= 48
```

```etape
Ouvre le menu **Capture** et choisis **Animation as video (MP4)…**.
```

Chaque image de la scène est calculée, du début à la fin, vue par sa caméra. S'il n'y a pas de caméra, c'est la vue actuelle. ffmpeg assemble la vidéo : s'il manque, Aryadne propose de l'installer.

```attention
La capture d'animation passe par le moteur de la vue. Fais un essai court avant de lancer un long plan : tu jugeras la qualité et le temps par image.
```

Deux autres sorties rapides du menu **Capture** : **PNG image (2400 px)** et **Turntable GIF (48 frames)**.

## Exporter le modèle

Pour un moteur de jeu ou un autre logiciel, tu exportes le modèle lui-même.

```etape
Appuie sur Ctrl+Maj+S, ou clique **Export…**. La fenêtre **Export model** s'ouvre.
```

| Format | Ce qui est gardé |
|---|---|
| OBJ | Quads et n-gones, UV, matériaux par ID |
| FBX binaire | Quads, UV, matériaux, os, animation |
| GLB | UV, un matériau par ID, os, poids, animation |
| STL binaire | La forme seule, pour l'impression 3D |
| PLY | La forme, pour les scans |

Les modificateurs actifs sont inclus dans le fichier exporté.

Les matériaux d'Aryadne ne voyagent pas tels quels. Pour les emporter, cuis leurs textures : voir [La cuisson](../10-uv-textures/06-la-cuisson.md).

```etape
Enregistre la scène avec Ctrl+S.
verifier: fichier.enregistre
```

## Ce qui n'existe pas encore

- Pas de volumes dans le rendu : ni brouillard, ni fumée.
- Pas de flou de mouvement.
- Un rendu à la fois : pas de file d'attente.

```quiz
D'où est vue l'image du rendu final ?
- [ ] Toujours de la caméra de la scène.
- [x] De la vue 3D telle qu'elle est. Pour la caméra, il faut d'abord regarder par elle avec 0.
- [ ] De face.
explication: F12 rend ce que montre la vue. La touche 0 aligne la vue sur la caméra.
```

```quiz
Ton rendu est plein de petits points blancs isolés. Quel réglage ?
- [ ] Monter Bounces.
- [x] Baisser Firefly clamp.
- [ ] Monter Depth of field.
explication: Firefly clamp plafonne la valeur d'un échantillon. Plus bas, l'image est plus propre.
```

```defi
Rends la même scène en trois ambiances : Studio, Outdoor, puis avec un contour de dessin sur fond clair.
indice: Render lighting pour les deux premières. Outlines (line art) et Backdrop brightness pour la troisième.
```

## En trois lignes

- **Path tracing (realistic)** pour juger. **Samples**, **Bounces** et **Denoise** pour la qualité.
- F12 rend la vue. 0 la place d'abord sur la caméra.
- **Capture** sort une vidéo. **Export…** sort le modèle.

Parcours terminé. Mets tout ensemble dans les [projets guidés](../16-projets/index.md).
