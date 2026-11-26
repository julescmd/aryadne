---
titre: Caméra 2,5D, expressions et export
niveau: avancé
duree: 15 min
prerequis: 05-storyboard-motion/06-parente-masques-effets
objectifs: Donner de la profondeur à des calques ; Animer une caméra ; Écrire une expression ; Exposer des réglages pour le montage ; Caler sur la musique et exporter
---

## Ce que tu vas réussir à faire

Tu vas monter un décor en trois plans de profondeur, y faire avancer une caméra, faire battre un calque avec une formule, puis exporter le tout en MP4.

```essayer
nouveau: arymotion
nom: decor
```

```etape
Clique **Essaie par toi-même**. Trace trois formes de tailles différentes : un fond, un plan moyen, un premier plan.
verifier: onglet = motion
```

## La profondeur

La 2,5D, ce sont des plans à des distances différentes. Ce n'est pas de la vraie 3D.

```etape
Choisis le fond. Dans la section **Depth (2.5D)**, coche **3D layer**. Règle **Depth** sur une grande valeur : il est loin.
```

```etape
Coche **3D layer** sur les deux autres calques. Laisse le plan moyen à zéro. Donne une valeur négative au premier plan : il passe devant le plan de la composition.
```

Plus **Depth** est grand, plus le calque est loin. Cette valeur s'anime par clés.

## La caméra

```etape
Dans la timeline, active le bouton **Camera (2.5D)**. Fais un clic droit dessus pour ses réglages.
```

| Réglage | Effet |
|---|---|
| **Dolly** | La caméra avance, ou recule |
| **Pan** | Un déplacement latéral |
| **Focal length** | La focale |
| **Focus distance** | La distance nette |
| **Blur** | La force du flou de profondeur |

```etape
Change **Pan**. Le premier plan glisse vite, le fond glisse lentement. C'est la parallaxe.
```

```etape
Monte **Blur**. Règle **Focus distance** sur le plan moyen : le fond et le premier plan deviennent flous.
```

```attention
Seuls les calques marqués **3D layer** sont touchés par la caméra. Un calque oublié reste collé à l'écran.
```

## Les expressions

Une expression est une petite formule. Elle calcule la valeur d'une propriété à chaque instant, sans clés.

| Variable | Sens |
|---|---|
| `time` ou `t` | Le temps, en secondes |
| `value` | La valeur animée, sans l'expression |
| `index` | Le rang du calque |
| `fps`, `frame` | La cadence, l'image courante |

```etape
Choisis le plan moyen. Dans l'inspecteur, clique l'icône de fonction à côté de **Rotation**, près du losange des clés. Un champ s'ouvre. Écris-y la formule ci-dessous.
```

```
sin(time * 4) * 20
```

```etape
Relis avec `Espace`. Le calque se balance de 20° de chaque côté.
```

Quelques expressions utiles :

| Expression | Effet |
|---|---|
| `wiggle(2, 10)` | Tremble 2 fois par seconde, de 10 unités |
| `value + [sin(time * 4) * 20, 0]` | Ajoute un va-et-vient horizontal à la position animée |
| `linear(time, 0, 1, 0, 100)` | Passe de 0 à 100 pendant la première seconde |
| `loop(2) * 180` | Recommence toutes les 2 secondes |

Les vecteurs s'écrivent `[x, y]`. Les fonctions disponibles : `sin`, `cos`, `abs`, `min`, `max`, `clamp`, `linear`, `ease`, `noise`, `random`, `wiggle`, `loop`, et les opérations courantes.

```astuce
`value` garde ton animation par clés. `value + wiggle(3, 5)` ajoute un tremblement par-dessus, sans la remplacer.
```

## Les contrôles

Un contrôle est un curseur de la composition. Tu l'utilises dans tes expressions par son nom.

```etape
Ouvre l'onglet **Composition**. Dans **Controls**, ajoute un contrôle. Nomme-le `force`.
```

```etape
Change l'expression de la rotation pour : `sin(time * 4) * force`. Bouge le curseur `force` : le balancement suit.
```

Un nom avec des espaces s'écrit avec des tirets bas dans l'expression. Un contrôle « force du vent » se lit `force_du_vent`.

## Les modèles pour le montage

La section **Template** d'un calque expose un réglage : **Editable text** ou **Editable color**. Ce réglage se change ensuite depuis le montage, sans ouvrir la composition.

```etape
Ajoute un texte. Dans sa section **Template**, coche **Editable text**. Donne au réglage un nom clair, comme `Titre`.
```

C'est ce nom qui s'affichera dans l'inspecteur du montage. Tu fais ainsi un habillage une fois, et tu changes son texte à chaque vidéo.

## La musique et le rythme

```etape
Dans l'onglet **Composition**, section **Music**, clique **Choose a sound…** et prends un son du coffre.
```

Le son est joué à la lecture et dans l'export MP4. Aryadne l'analyse pour trouver ses temps forts.

Dans la timeline, le bouton de calage sur le rythme déplace les clés du calque choisi sur les temps forts les plus proches.

## Les précompositions

Un calque **Composition** joue une autre composition du coffre comme un seul calque. Tu ranges une animation complexe dans sa propre composition, puis tu la poses où tu veux.

| Réglage | Effet |
|---|---|
| La vitesse | 200 % : deux fois plus vite. -100 % : à l'envers |
| Le départ | L'instant de la précomposition joué au début du calque |
| La boucle | Boucle au lieu de garder la dernière image |
| **Open the precomp** | L'ouvre |
| **Reload** | La relit après une modification |

## Exporter

Le menu **Export** de la composition propose trois sorties.

| Format | Notes |
|---|---|
| **Export MP4** | Une vidéo. ffmpeg encode |
| **Lottie** | Une animation vectorielle en JSON, pour le web et les applications |
| **Animated SVG** | Lisible par un navigateur |

```etape
Ouvre **Export**, puis **Export MP4**.
```

Pour Lottie et SVG, **Transparent background** retire le fond.

```attention
Lottie et SVG ne reprennent pas tout. Images, bruit fractal, effets et flous peuvent manquer. Aryadne affiche la liste à l'export : « Not exported ». Lis-la.
```

Le bouton d'envoi passe la composition au montage, sans export.

```quiz
Que vaut `time` dans une expression ?
- [ ] Le numéro de l'image.
- [x] Le temps écoulé, en secondes.
- [ ] La durée de la composition.
explication: time compte en secondes. frame donne l'image courante.
```

```quiz
Un calque ne réagit pas à la caméra. Pourquoi ?
- [x] 3D layer n'est pas coché sur ce calque.
- [ ] Il n'a pas de clés.
- [ ] La caméra est trop loin.
explication: La caméra 2,5D ne touche que les calques marqués 3D layer.
```

```quiz
À quoi sert Editable text dans la section Template ?
- [ ] À corriger l'orthographe.
- [x] À changer le texte depuis le montage, sans ouvrir la composition.
- [ ] À exporter le texte en SVG.
explication: Le réglage est exposé au montage, sous le nom que tu lui donnes.
```

```defi
Fais un habillage de bas d'écran réutilisable : un bandeau animé avec un nom modifiable et une couleur modifiable depuis le montage.
indice: **Editable text** sur le calque du nom, **Editable color** sur le bandeau. Teste en posant la composition dans un montage.
```

## Récapitulatif

- **3D layer** et **Depth** donnent de la profondeur. **Camera (2.5D)** fait la parallaxe et le flou.
- Une expression calcule une valeur avec `time`, `value` et quelques fonctions.
- **Export** sort un MP4, un Lottie ou un SVG animé. **Template** expose des réglages au montage.

Parcours suivant : [Vidéo, son et code](../06-video-son-code/index.md)
