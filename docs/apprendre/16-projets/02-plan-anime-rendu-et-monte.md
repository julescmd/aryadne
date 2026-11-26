---
titre: Projet 2 : un plan animé, rendu et monté
niveau: avancé
duree: 50 min
prerequis: 15-animation-rendu/01-cles-lissages-et-rythme, 15-animation-rendu/07-rendu, 06-video-son-code/01-premier-montage
objectifs: Animer un plan court ; Faire bouger la caméra ; Sortir le plan en vidéo ; Le monter avec un titre ; Exporter le film
---

## Ce que tu vas réussir

Un petit film de quelques secondes : une balle rebondit devant la caméra, un titre apparaît, le tout exporté en MP4. Tu passes de la 3D au montage.

```essayer
nouveau: aryscene
nom: projet-plan
```

## Étape 1 : le décor

```etape
Ajoute une **Sphere** : la balle. Ajoute un **Plane** et agrandis-le : le sol. Supprime le cube.
verifier: scene.objets >= 4
```

```etape
Dans l'onglet **Scene**, mets **End** à 72. Le plan durera trois secondes.
```

```etape
Donne une couleur à la balle : choisis-la, onglet **Object**, et règle sa couleur.
verifier: scene.selection
```

```astuce
Pour une vraie matière, fais comme au projet 1 : le **＋** de l'onglet **Material**, puis un préréglage dans l'Atelier.
```

## Étape 2 : l'animation

Détail des gestes : [Clés, lissages et rythme](../15-animation-rendu/01-cles-lissages-et-rythme.md).

```etape
Passe en vue de face avec 1. Choisis la balle. À l'image 1, place-la en haut à gauche et pose une clé avec I.
```

```etape
À l'image 14, pose-la au sol, un peu plus à droite. Clé.
verifier: lecture.image >= 14
```

```etape
À l'image 28, remonte-la moins haut, encore plus à droite. Clé. Continue : au sol à 40, petit saut à 50, au sol à 58, immobile à 72.
verifier: lecture.image >= 72
```

```etape
Clic droit sur chaque clé haute : **Ease in**. Clic droit sur chaque clé du sol : **Ease out**.
```

```etape
Lis avec Espace. La balle traverse le cadre en rebondissant, puis s'arrête.
```

## Étape 3 : la caméra

Une caméra fixe filme. Une caméra qui suit raconte.

```etape
Choisis la caméra. Dans l'onglet **Object**, ajoute la contrainte **Aim at**, avec la balle pour cible.
```

```etape
Regarde par la caméra avec 0 et lis. La caméra garde la balle au centre pendant tout le plan.
```

```etape
Recule la caméra jusqu'à voir le sol sous la balle à chaque rebond.
```

### Un léger travelling

```etape
Choisis la caméra. À l'image 1, pose une clé. À l'image 72, avance-la un peu vers la balle et pose une clé.
```

Les clés règlent sa position. La contrainte règle sa direction. Les deux s'additionnent.

## Étape 4 : la lumière

```etape
Dans l'onglet **World**, coche **Cast shadows** et monte un peu **Softness**. L'ombre de la balle touche le sol à chaque rebond : c'est elle qui donne le contact.
```

```etape
Place la lumière de la scène en hauteur, de côté. L'ombre s'allonge et se lit mieux.
```

## Étape 5 : la vidéo

Détail des gestes : [Rendu et export](../15-animation-rendu/07-rendu.md).

```etape
Enregistre la scène avec Ctrl+S.
verifier: fichier.enregistre
```

```etape
Ouvre le menu **Capture** et choisis **Animation as video (MP4)…**. Attends la fin du calcul.
```

La vidéo est enregistrée dans le coffre, à côté de la scène ou de son modèle. Chaque image est vue par la caméra.

```attention
Il faut ffmpeg. S'il manque, Aryadne propose de l'installer.
```

## Étape 6 : le montage

Détail des gestes : [Ton premier montage](../06-video-son-code/01-premier-montage.md).

```essayer
nouveau: aryseq
nom: projet-film
```

```etape
Depuis le panneau des fichiers, glisse ta vidéo dans le panneau **Media (edit)**.
verifier: fichier.ouvert = aryseq
```

```etape
Survole sa carte et appuie sur E : **Add at the end**. Le plan est posé sur la piste V1.
```

```etape
Lis avec Espace.
```

### Un titre

```etape
Place la tête de lecture à l'image où la balle s'arrête. Appuie sur T et clique dans le moniteur. Tape ton titre.
```

```etape
Le titre est un clip, sur une piste au-dessus de l'image. Choisis-le. Dans **Inspector (edit)**, onglet **Clip**, règle sa **Scale** et sa **Color**.
```

```etape
Tire le bord du clip de titre pour qu'il dure jusqu'à la fin du plan. Reviens à l'outil de sélection avec V.
```

### Un fondu de fin

```etape
Choisis le plan. Place la tête de lecture une demi-seconde avant sa fin. Dans l'onglet **Clip**, clique **Turn on animation** : une clé est posée. Va à la dernière image et mets **Opacity** à 0 : une seconde clé est posée, et le plan s'éteint en fondu.
```

## Étape 7 : l'export

```etape
Enregistre le montage avec Ctrl+S.
verifier: fichier.enregistre
```

```etape
Appuie sur Ctrl+M : **Export as…**. Choisis **MP4 H.264 (YouTube, social)** et lance.
```

Ton film est dans le coffre.

## Ce que tu viens de faire

| Outil | Ce que tu y as fait |
|---|---|
| Scène | Décor, lumière, ombres |
| Timeline 3D | Sept clés pour la balle, deux pour la caméra |
| Capture | La vidéo du plan |
| Montage | Le plan, un titre, un fondu |
| Export | Le film |

Le montage ne copie pas ta vidéo : il garde son chemin. La scène, la vidéo et le montage restent trois fichiers du coffre, que tu peux rouvrir.

```quiz
Pourquoi l'ombre compte-t-elle tant dans un rebond ?
- [ ] Elle rend l'image plus sombre.
- [x] Elle montre le moment précis où la balle touche le sol.
- [ ] Elle remplace les clés.
explication: Sans ombre, une balle semble flotter. L'ombre qui rejoint la balle signe le contact.
```

```quiz
La caméra a une contrainte Aim at et des clés de position. Que se passe-t-il ?
- [ ] Les clés sont ignorées.
- [x] Les clés déplacent la caméra, la contrainte l'oriente vers la cible.
- [ ] La contrainte est ignorée.
explication: Aim at ne règle que la direction. La position reste libre, donc animable.
```

```defi
Ajoute un second plan : la même action, filmée de très près et de côté. Monte les deux à la suite.
indice: Duplique la scène avec Ctrl+Maj+S, change la caméra, refais la capture. Dans le montage, coupe sur un rebond.
```

## En trois lignes

- Peu de clés, les bons lissages, et une ombre pour le contact.
- **Capture**, puis **Animation as video (MP4)…** : le plan devient une vidéo.
- Dans le montage : E pose le plan, T pose un titre, Ctrl+M exporte.

[Suite : Projet 3, une affiche](03-affiche-2d-et-vectoriel.md)
