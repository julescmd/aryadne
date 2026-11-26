---
titre: Projet 2 : un plan avec un tissu simulé, rendu et monté
niveau: avancé
duree: 55 min
prerequis: 14-simulation/06-vent-lecture-et-cache, 15-animation-rendu/07-rendu, 06-video-son-code/01-premier-montage
objectifs: Poser une simulation de tissu dans une scène 3D ; Lui donner un matériau ; Caler son départ sur la timeline ; Rendre une image et sortir le plan en vidéo ; Le monter avec un titre
---

## Ce que tu vas réussir

Un petit film de trois secondes : un drapeau claque au vent sur sa hampe, la caméra avance, un titre apparaît. Le drapeau est une vraie simulation de tissu. Il vit dans la scène comme un objet ordinaire : il est ombré, il porte un matériau, il sort au rendu.

Tu passes par quatre outils : la scène, la simulation, le rendu, le montage.

```essayer
nouveau: aryscene
nom: projet-plan
```

## Étape 1 : le décor

```etape
Supprime le cube. Ajoute un **Plane** et agrandis-le avec S : le sol.
```

```etape
Ajoute un **Cylinder** : la hampe. Dans l'onglet **Object**, mets **Scale** à 0,035 en X et en Z, et à 1,6 en Y. Mets **Location** à (-1,53, 1,6, 0).
verifier: scene.objets >= 4
```

```etape
Dans l'onglet **Scene**, mets **End** à 72. Le plan durera trois secondes.
```

## Étape 2 : le drapeau

Une simulation est un fichier à part, un `.arygeo`. La scène ne le copie pas : elle le lit.

```etape
Ouvre le menu **Add**, puis **Node geometry**, et choisis **New simulated flag**.
verifier: scene.simulations >= 1
```

Un drapeau apparaît contre la hampe, plat et rouge. Son graphe vient d'être créé à côté de la scène. Dans la liste, son icône est une vague.

```astuce
Tu as déjà une simulation dans le coffre ? Elle est dans le même menu, sous **FROM THE VAULT**. Tu peux aussi la glisser depuis le panneau des fichiers sur la vue.
```

```etape
Lis avec Espace. À chaque image de la scène, la simulation avance d'une image. Le tissu part à plat, puis le vent le prend.
verifier: lecture.image >= 20
```

```attention
La première lecture calcule la simulation, image après image. La vue affiche **computing…** pendant ce temps. La deuxième lecture reprend ce qui est déjà calculé.
```

### Le caler sur le plan

Au début du plan, le drapeau est encore plat. Dans un film, il flotte déjà.

```etape
Choisis le drapeau. Dans l'onglet **Object**, section **Simulation**, mets **Start (frames)** à -24.
verifier: scene.selection
```

La simulation a maintenant une seconde d'avance. Sous le réglage, **Simulation frame** te dit quelle image de la simulation tu regardes.

```etape
Reviens à l'image 0. Le drapeau flotte déjà.
```

```attention
La simulation du drapeau dure 120 images. Avec 24 images d'avance, il en reste 96 pour un plan de 72 : c'est assez. Au-delà de sa dernière image, une simulation reste sur place.
```

## Étape 3 : le matériau

Le drapeau reçoit un matériau comme les autres objets. La grille du graphe lui donne des UV : un motif suit le tissu dans ses plis.

```etape
Le drapeau choisi, ouvre l'onglet **Material**. Clique le petit **＋** : un matériau est créé, appliqué au drapeau et ouvert dans l'Atelier.
verifier: fichier.ouvert = arymat
```

```etape
Dans la bibliothèque, filtre **Presets** et clique **Woven fabric**. Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```etape
Reviens à l'onglet de ta scène. Le drapeau porte son tissu.
verifier: scene.materiaux >= 1
```

```astuce
Pour des bandes qui suivent le tissu, entre dans le graphe du calque. Relie la sortie **uv** d'un node **Texture coordinates** à l'entrée **Coordinates** d'un node **Stripes**. Sans ce lien, un motif se pose dans l'espace et glisse sur le tissu quand il bouge.
```

## Étape 4 : régler le vent

Le drapeau se règle dans son graphe. La scène suit.

```etape
Choisis le drapeau. Dans l'onglet **Object**, section **Simulation**, clique l'icône du graphe. Il s'ouvre dans l'onglet **Simulation**.
verifier: graphe.contient = sim_solver
```

```etape
Trouve le node **Combine XYZ**, dans le cadre du vent. Passe **X** de 22 à 30 : le vent pousse plus fort le long du drapeau.
```

```etape
Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```etape
Reviens à l'onglet de ta scène et lis. Le drapeau est plus tendu : la scène a relu le graphe.
verifier: scene.simulations >= 1
```

Détail des réglages du tissu : [Régler un tissu](../14-simulation/04-regler-un-tissu.md) et [Vent, lecture et cache](../14-simulation/06-vent-lecture-et-cache.md).

```astuce
Une simulation longue à calculer se fige dans l'onglet **Simulation** (le cache sur le disque). La scène lit ce cache au lieu de recalculer.
```

## Étape 5 : la caméra et la lumière

```etape
Regarde par la caméra avec 0. Recule-la jusqu'à voir la hampe en entier et le drapeau déployé.
```

```etape
À l'image 0, pose une clé sur la caméra avec I. À l'image 72, avance-la un peu vers le drapeau et pose une clé.
verifier: lecture.image >= 72
```

```etape
Dans l'onglet **World**, coche **Cast shadows**. L'ombre du drapeau bouge sur le sol avec lui.
```

```etape
Place la lumière de la scène en hauteur, de côté. Les plis du tissu se creusent.
```

## Étape 6 : une image, puis la vidéo

Détail des gestes : [Rendu et export](../15-animation-rendu/07-rendu.md).

```etape
Va à une image où le drapeau a une belle forme. Appuie sur F12, choisis **720p** et clique **Render**. Le tracé de chemins rend le tissu, son matériau et son ombre.
```

```etape
Ferme le rendu. Enregistre la scène avec Ctrl+S.
verifier: fichier.enregistre
```

```etape
Ouvre le menu **Capture** et choisis **Animation as video (MP4)…**. Attends la fin du calcul.
```

Chaque image est vue par la caméra, avec la simulation à sa place. La vidéo est enregistrée dans le coffre, à côté de la scène.

```attention
Il faut ffmpeg. S'il manque, Aryadne propose de l'installer.
```

## Étape 7 : le montage

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
Place la tête de lecture au milieu du plan. Appuie sur T et clique dans le moniteur. Tape ton titre.
```

```etape
Le titre est un clip, sur une piste au-dessus de l'image. Choisis-le. Dans **Inspector (edit)**, onglet **Clip**, règle sa **Scale** et sa **Color**. Tire son bord pour qu'il dure jusqu'à la fin du plan. Reviens à l'outil de sélection avec V.
```

```etape
Choisis le plan. Place la tête de lecture une demi-seconde avant sa fin. Dans l'onglet **Clip**, clique **Turn on animation**. Va à la dernière image et mets **Opacity** à 0 : le plan s'éteint en fondu.
```

## Étape 8 : l'export

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
| Scène | Le décor, le drapeau posé comme un objet, son départ calé |
| Simulation | Le vent, réglé dans le graphe |
| Atelier | Le matériau du tissu |
| Rendu | Une image en tracé de chemins, puis la vidéo du plan |
| Montage | Le plan, un titre, un fondu |

Quatre fichiers restent dans le coffre : la scène, le graphe du drapeau, la vidéo, le montage. Aucun ne copie l'autre. Change le vent dans le graphe : la scène suit. Refais la capture : le montage relit la vidéo.

```quiz
Tu changes le vent dans le graphe du drapeau et tu enregistres. Que fait la scène ?
- [ ] Rien : il faut reposer le drapeau.
- [x] Elle relit le graphe et recalcule la simulation.
- [ ] Elle garde l'ancienne simulation jusqu'au prochain rendu.
explication: La scène ne garde que le chemin du graphe. Dès que le fichier change, elle le relit.
```

```quiz
Start (frames) vaut -24. Que vois-tu à l'image 0 de la scène ?
- [ ] La première image de la simulation, à plat.
- [x] La simulation après 24 images : le drapeau flotte déjà.
- [ ] Rien : la simulation commence à l'image 24.
explication: Un départ négatif donne de l'avance à la simulation. Un départ positif la retarde.
```

```quiz
Pourquoi relier Texture coordinates au motif du matériau ?
- [ ] Pour que le rendu soit plus rapide.
- [x] Pour que le motif suive le tissu dans ses plis.
- [ ] Pour que le tissu soit plus souple.
explication: Les UV sont attachées aux points du tissu. Un motif posé dans l'espace reste immobile pendant que le tissu le traverse.
```

```defi
Fais tomber une nappe sur une boule de ta scène. La boule est un objet ordinaire, que tu peux déplacer.
indice: Pose l'exemple « tissu sur une boule » dans une scène. Dans son graphe, ajoute un node Scene object, écris le nom de ta sphère dans Object, et relie Center et Radius aux entrées Ball center et Ball radius du Solver.
```

```defi
Exporte la simulation pour un autre logiciel.
indice: Le drapeau choisi, onglet Object, section Simulation : l'icône de téléchargement écrit un fichier OBJ par image, dans un dossier à côté de la scène.
```

## En trois lignes

- **Add**, **Node geometry** : une simulation devient un objet de la scène. Elle suit la timeline.
- Le tissu se règle dans son graphe, le plan dans la scène. **Start (frames)** cale l'un sur l'autre.
- F12 rend une image, **Capture** sort la vidéo, le montage pose le titre.

[Suite : Projet 3, une affiche](03-affiche-2d-et-vectoriel.md)
