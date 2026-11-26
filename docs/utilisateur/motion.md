# Motion design

Étape **Motion**, outil `Comps`. Fichier : `.arymotion`. Comme After Effects, en plus simple : des compositions animées par clés.

## La composition

Une composition a une taille, une cadence, une durée et une pile de **calques**. Le premier calque de la liste est au-dessus.

`File › New › Animated composition (motion)` : choisis la résolution et la cadence.

## L'écran

| Panneau | Contenu |
|---|---|
| `Project (motion)` | Les compositions, `Add a layer`, les préréglages d'animation |
| `View (motion)` | La composition, avec les poignées du calque choisi |
| `Inspector (motion)` | Onglets `Properties` et `Composition` |
| `Timeline (motion)` | Les calques et leurs clés |

## Les calques

| Genre | Ce que c'est |
|---|---|
| `Text` | Du texte |
| `Rectangle`, `Ellipse` | Une forme |
| `Image` | Une image du coffre |
| `Path` | Un tracé dessiné à main levée, ou venu d'un SVG |
| `Fractal noise` | Un bruit animé : fumée, nuages, textures qui bougent |
| `Null` | Un objet invisible, qui sert de parent pour piloter plusieurs calques |
| `Composition` | Une précomposition : une autre composition du coffre, jouée comme un calque |

### Ajouter

- Menu `Layer`, ou `Add a layer` dans le panneau Projet.
- Outils de la vue : `Select (V)`, `Rectangle (Q)`, `Ellipse (E)`, `Text (T)`, `Freehand path (P)`.
- `Import an SVG` : un logo d'Illustrator, de Figma ou du Vectoriel devient des calques de forme.
- Depuis le [Vectoriel](2d-vectoriel.md#animation) : `Animate in Motion`.

### Gérer

`Duplicate`, `Move up`, `Move down`, `Delete`. Dans la timeline, glisse les bords d'un calque pour régler son `Start` et son `End`.

## Animer

Chaque calque a quatre propriétés de base : `Location`, `Scale`, `Rotation`, `Opacity`. Sans clé, la valeur est fixe. Avec des clés, elle passe de l'une à l'autre.

1. Place la tête de lecture.
2. Dans l'inspecteur, clique le losange d'une propriété : `Add a key at this time`.
3. Déplace la tête de lecture, change la valeur, repose une clé.

`K` pose une clé de position. Glisser un calque dans la vue alors qu'il a déjà des clés en pose une à l'instant courant.

Le passage entre deux clés suit le **lissage de la clé de départ**. Clic droit sur une clé : le menu des lissages. Ce sont les mêmes que dans la 3D. Voir [Animation 3D](3d-animation.md#lissages).

`Smooth keys` : `The chosen key, toward the next one`, ou `All keys of the layer`.

### Timeline

| Geste | Effet |
|---|---|
| `Espace` | Lecture |
| `←` `→` | Image par image |
| `Origine` | Début |
| `Ctrl+molette` | Zoom |
| `Suppr` | Supprimer le calque ou la clé |
| `Maj` pendant un déplacement | Contraindre |

Boutons de la barre :

| Bouton | Effet |
|---|---|
| `Onion skin` | Le calque choisi aux images d'avant (rouge) et d'après (vert) |
| `Motion blur` | Flou de mouvement. Clic droit : `Shutter angle` (180° : le flou du cinéma) et `Samples` |
| `Camera (2.5D)` | Voir plus bas |
| `Graph editor` | Toutes les clés du calque, tangentes comprises |
| Outils d'animation | Tweener, décaler les clés, poses. Voir [Animation 3D](3d-animation.md#outils-danimation) |
| Caler sur le rythme | Les clés du calque choisi vont sur les temps forts les plus proches de la musique |

## Préréglages d'animation

`Animation presets` : `Letter by letter in`, `Bounce`, `Soft slide`, `Fade in`, `Fade out`, `Full turn`. Un clic les applique au calque choisi, à partir de la tête de lecture.

## Propriétés par genre

### Texte

| Réglage | Effet |
|---|---|
| Police | Une police installée, ou `Font file (.ttf, .otf)…` |
| Axes d'une police variable | `Weight`, `Width`, `Slant`, `Italic`, `Optical size`. Animables par clés |
| `Size`, `Spacing` | Taille, espace entre lettres |
| Alignement | `Left`, `Centered`, `Right` |

**Text animator** : `Animate in piece by piece`. Le texte entre par `Letter`, `Word` ou `Line`.

| Réglage | Effet |
|---|---|
| `Offset` | Délai entre deux morceaux |
| `Start x`, `Start y` | D'où part chaque morceau |
| `Start size` | Taille de départ |
| `Duration` | Durée de l'entrée d'un morceau |
| `Order` | `In order`, `Reversed`, `From the center out`, `Random` |

### Forme et tracé

`Corners` (rectangle), `Thickness`, `Closed`, pointe de flèche au bout, `Fill the inside`.

**Trim path** (tracé progressif) : `Start` et `End` du tracé visible, animables. `Animate drawing` fait se dessiner le tracé en une seconde à partir de la tête de lecture. `Erase at the end` l'efface pendant la dernière seconde du calque.

**Clé de forme** : pose une clé, puis déplace les points à un autre instant. La forme se transforme.

### Bruit fractal

`Pattern size`, `Evolution` (par seconde), `Contrast`, `Turbulent (filaments)`, `Seed`.

### Précomposition

| Réglage | Effet |
|---|---|
| Vitesse | 200 % : deux fois plus vite. -100 % : à l'envers |
| Départ | L'instant de la précomposition joué au début du calque |
| Boucle | Boucler au lieu de garder la dernière image |
| `Open the precomp` | L'ouvre |
| `Reload` | La relit après modification |

## Réglages communs à tous les calques

### Layer

`Start`, `End`, `Visible`, `Parent`, `Matte`.

- **Parent** : le calque suit son parent. Déplacer, tourner ou agrandir le parent l'emporte avec lui.
- **Matte** (cache de piste) : ce calque n'apparaît qu'à travers le calque juste au-dessus, qui ne se dessine plus. Modes : alpha, `Alpha inverted`, `Luma`, `Luma inverted`.

### Style

`Stroke` (contour, `Width`), `Drop shadow` (`Blur`), `Glow` (`Radius`, `Strength`).

### Masks

Un masque limite le calque à une forme.

- `Mask` : `New rectangular mask`, `New elliptical mask`. Glisse ses points dans la vue.
- Modes : `Add`, `Subtract`, `Intersect`.
- `Invert mask`, `Smooth outline`, `Feather` (contour progressif), `Mask opacity`.
- Clé de masque : pose une clé, puis déplace les points à un autre instant.

### Effects

`Add an effect` :

| Effet | Réglages |
|---|---|
| `Wave` | `Amplitude`, `Wavelength`, `Speed`, `Vertical wave` |
| `Turbulence` | `Amount`, `Eddy size`, `Evolution` |
| `Offset` | `x per second`, `y per second` : l'image défile |
| `Puppet` | Des épingles posées sur le calque. Les tirer plie l'image autour d'elles |

**Marionnette.** `Place pins`, puis clique sur le calque dans la vue. Tire une épingle : l'image se plie. Glisser une épingle pose une clé. `Alt+clic` la retire. `Reset pins to rest`, `Remove all pins`.

### Repeater

Des copies du calque, chacune décalée, tournée et réduite par rapport à la précédente : `Copies`, décalage, `Rotation`, `Scale`, `End opacity`.

### Path (trajectoire)

- `Auto-orient` : le calque tourne pour suivre la direction de son mouvement. Une voiture sur une route.
- `Wiggle` (tremblement) : `Wiggle on its own`. De petits mouvements doux au hasard, en plus de l'animation. Réglages : mouvements par seconde, amplitude en `Position`, `Rotation`, `Scale`. `New random seed` change le tirage.

### Rig

**Two-bone IK** : ce calque, son enfant et l'enfant de celui-ci forment un bras ou une jambe. Un contrôleur (objet nul) tire le bout. L'épaule et le coude tournent tout seuls. `Bend the other way` inverse le pli. Il faut une chaîne de trois calques reliés par la parenté.

### Depth (2.5D)

`3D layer` donne une profondeur au calque. La caméra de la composition le voit en perspective : parallaxe, flou de profondeur de champ.

- `Depth` : plus grand, plus loin. Négatif : devant le plan de la composition. Animable.

## Caméra 2,5D

Bouton `Camera (2.5D)`, clic droit pour les réglages :

| Réglage | Effet |
|---|---|
| `Dolly` | La caméra avance (positif) ou recule |
| `Pan` | Déplacement latéral |
| `Focal length` | Focale |
| `Focus distance` | Distance nette |
| `Blur` | Force du flou de profondeur |

Seuls les calques marqués `3D layer` sont touchés.

## Expressions

Une expression est une petite formule qui calcule la valeur d'une propriété à chaque instant.

| Variable | Sens |
|---|---|
| `time` ou `t` | Le temps, en secondes |
| `value` | La valeur animée, sans l'expression |
| `index` | Le rang du calque |
| `fps`, `frame` | Cadence, image courante |
| `pi` | 3,14159… |

Fonctions : `sin`, `cos`, `tan`, `abs`, `sqrt`, `floor`, `ceil`, `round`, `min`, `max`, `pow`, `exp`, `log`, `mod(a, b)`, `clamp(x, a, b)`, `linear(x, a, b, c, d)`, `ease(x, a, b, c, d)`, `noise(x)`, `random(n)`, `wiggle(fréquence, amplitude)`, `length(v)`, `loop(durée)`.

Nombres et vecteurs `[x, y]`. Opérateurs : `+`, `-`, `*`, `/`, `%`, `^`, parenthèses.

Exemples :

```
wiggle(2, 10)
value + [sin(time * 4) * 20, 0]
linear(time, 0, 1, 0, 100)
loop(2) * 180
```

### Contrôles

Onglet `Composition`, section **Controls** : un curseur par réglage. Dans une expression, écris son nom, les espaces remplacés par des tirets bas. Un contrôle « force du vent » se lit `force_du_vent`.

## Modèles

Section **Template** d'un calque : `Editable text`, `Editable color`. Le réglage est exposé. On le change depuis le montage, sans ouvrir la composition. Donne-lui un nom clair : c'est ce nom qui s'affichera dans le montage.

## Composition

Onglet `Composition` : `Format` (taille), `Time` (cadence, `Duration`), `Background`, `Music`.

**Music** : `Choose a sound…`. Le son est joué à la lecture et dans l'export MP4. Son analyse sert à caler les clés sur le rythme.

## Exporter

| Format | Notes |
|---|---|
| `Export MP4` | Le rendu est fait sur le processeur, le même que l'aperçu. ffmpeg encode |
| `Lottie` | Animation vectorielle en JSON, pour le web et les applications |
| `Animated SVG` | Lisible par un navigateur |

Pour Lottie et SVG : les calques sont exportés image par image, parenté, expressions et tremblement compris. Les clés inutiles sont retirées. `Transparent background` retire le fond.

Ce qui ne passe pas est listé : « Not exported: … ».

Le bouton d'envoi passe la composition au montage.

## Limites

- Pas de vraie 3D : la profondeur est en 2,5D (des plans à des distances différentes).
- Les effets sont au nombre de quatre. Pas de greffons.
- Lottie et SVG ne reprennent pas tout : images, bruit fractal, effets et flous peuvent manquer. La liste s'affiche à l'export.
- Le rendu se fait sur le processeur. Flou de mouvement à beaucoup d'échantillons : lent.
