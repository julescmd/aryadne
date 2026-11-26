---
titre: Mocap et suivi de caméra
niveau: avancé
duree: 15 min
prerequis: 15-animation-rendu/05-squelette-et-rig, 15-animation-rendu/03-outils-poses-calques
objectifs: Importer une capture de mouvement BVH ; La poser sur un squelette existant ; La corriger sans la détruire ; Retrouver le mouvement d'une caméra filmée ; Lire l'erreur du suivi
---

## Ce que tu vas réussir

Tu vas amener du réel dans ta scène : un mouvement d'acteur capturé, puis le mouvement d'une vraie caméra. Tu sauras aussi dire quand ça ne peut pas marcher.

```essayer
nouveau: aryscene
nom: mocap-et-suivi
```

```attention
Cette leçon demande deux fichiers à toi : un fichier `.bvh` de capture de mouvement, et une courte vidéo où la caméra se déplace. Sans eux, lis la leçon : tu sauras quoi faire le jour où tu les auras.
```

## Partie 1 : la capture de mouvement

Un fichier BVH contient un squelette et son mouvement, image par image.

### Un squelette neuf

```etape
Vérifie qu'aucun os n'est choisi. Ouvre **Add**, puis **Motion capture (BVH)…**, et choisis ton fichier.
```

Un squelette est créé : un os par articulation du fichier. Un message donne le nombre d'os : « bones imported ».

```etape
Lis avec Espace. Le squelette joue le mouvement capturé.
verifier: lecture.image >= 24
```

Une clé est posée par image, ramenée à la cadence de la scène.

### Sur un squelette existant

Si un os est choisi au moment de l'import, le mouvement va sur **son** squelette. Les os sont retrouvés **par leur nom** : Hips, Spine, LeftArm…

Le message devient « bones animated ». Si aucun os ne porte le nom d'une articulation du fichier, un message le signale.

```attention
Il n'y a pas de retargeting. Le mouvement ne s'adapte pas à un squelette de proportions ou de noms différents. Les noms doivent correspondre.
```

### Corriger sans détruire

Une capture, c'est une clé par image sur des dizaines d'os. Y toucher directement est un piège. Trois outils te l'évitent.

| Besoin | Outil |
|---|---|
| Corriger une pose, relever un bras | Un calque d'animation |
| En faire un cycle réutilisable | Une action, dans **Animation mixer** |
| Alléger le nombre de clés | **Simplify**, dans l'éditeur de courbes |

```etape
Dans l'onglet **Scene**, section **Animation layers**, clique **New animation layer**, puis le point du calque.
```

```etape
Choisis un os du bras. À une image de ton choix, tourne-le et pose une clé. Lis : le bras est relevé pendant tout le mouvement, et la capture dessous est intacte.
verifier: scene.selection
```

## Partie 2 : le suivi de caméra

Le suivi de caméra retrouve le mouvement de la caméra d'une vidéo. Il pose dans la scène une caméra animée, des repères de la scène filmée, et la vidéo en fond. Tu peux alors placer des objets 3D qui tiennent dans le plan.

### Les conditions

Lis-les avant de filmer.

- **La caméra doit se déplacer.** Un panoramique sur trépied ne donne pas de relief : le calcul échoue ou donne n'importe quoi.
- **Il faut des détails nets** : textures, coins, objets fixes. Un ciel uni ou une image floue ne se suivent pas.
- Seules les premières secondes de la vidéo sont suivies.
- ffmpeg est nécessaire pour lire la vidéo.

### Lance le suivi

```etape
Ouvre **Add**, puis **Tracked camera from a video…**, et choisis ta vidéo.
```

La fenêtre **Camera tracking** s'ouvre.

```etape
Laisse **Estimate the lens field of view** coché. Si tu connais le champ horizontal de ton objectif, décoche et donne-le : le résultat sera plus juste.
```

```etape
Clique **Track**.
```

À la fin, un message donne le nombre d'images suivies et l'erreur moyenne, en pixels.

### Lis l'erreur

L'erreur moyenne est la distance, en pixels, entre l'endroit où un point devrait apparaître et celui où il apparaît vraiment.

| Erreur | Verdict |
|---|---|
| Moins de 1 pixel | Bon suivi |
| Entre les deux | À juger à l'œil, en lisant le plan |
| Plus de 2 ou 3 | Les objets ajoutés glisseront |

### Place un objet dans le plan

```etape
Appuie sur 0. Tu regardes par la **Tracked camera**, avec la vidéo en fond.
```

```etape
Ajoute un **Cube**. Place-le sur un des repères de la scène filmée, au sol.
verifier: scene.objets >= 4
```

```etape
Lis. Le cube reste accroché au sol filmé pendant que la caméra bouge.
```

L'échelle et l'orientation du sol sont arbitraires. Corrige-les à la main, en déplaçant et en tournant le groupe du suivi.

### Ce qui n'existe pas encore

- Pas de réglage manuel des points suivis.
- Pas de masque pour exclure ce qui bouge : un passant, une voiture.
- Pas de correction de la distorsion de l'objectif.
- Pas de suivi d'un objet qui bouge devant une caméra fixe.

Pour accrocher un simple titre à un élément d'une vidéo, le suivi d'un point en 2D se trouve dans le montage.

```etape
Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```quiz
Comment un BVH retrouve-t-il les os d'un squelette existant ?
- [ ] Par leur position.
- [x] Par leur nom.
- [ ] Par leur longueur.
explication: Hips, Spine, LeftArm… Les noms doivent correspondre : il n'y a pas de retargeting.
```

```quiz
Quelle prise de vue ne peut pas être suivie ?
- [ ] Un travelling le long d'une façade en briques.
- [x] Un panoramique sur trépied devant un ciel uni.
- [ ] Une marche caméra à la main dans une rue.
explication: Sans déplacement de la caméra, pas de relief. Sans détails nets, rien à suivre.
```

```defi
Avec ta capture de mouvement, fabrique une boucle de marche qui avance sur dix cycles.
indice: Isole un cycle, fais-en une action, puis Repeat à 10 et Travels each cycle dans Animation mixer.
```

## En trois lignes

- **Motion capture (BVH)…** crée un squelette, ou anime celui de l'os choisi, par les noms.
- Un calque d'animation corrige une capture sans la toucher.
- Le suivi de caméra veut une caméra qui se déplace et des détails nets. Sous 1 pixel d'erreur, c'est bon.

[Suite : Rendu et export](07-rendu.md)
