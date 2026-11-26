---
titre: L'animatique, la couleur et l'export
niveau: intermédiaire
duree: 15 min
prerequis: 05-storyboard-motion/02-infos-et-camera
objectifs: Lire le storyboard comme une animatique ; Colorier une case d'après la précédente ; Exporter une vidéo et une planche ; Envoyer le storyboard au montage
---

## Ce que tu vas réussir à faire

Tu vas voir ton film avant de l'avoir fait : les cases se suivent à leur durée, avec leurs mouvements de caméra. Puis tu l'enverras au montage, où tu remplaceras peu à peu chaque case par le vrai plan.

```essayer
nouveau: arystory
nom: animatique
```

```etape
Clique **Try it yourself**. Dessine dans les trois cases de départ et ajoute-en une quatrième avec `N`. Donne une durée à chacune.
verifier: onglet = video
```

## Lire l'animatique

```etape
Appuie sur `Espace`, ou clique **Play the animatic**. Les cases se suivent, chacune à sa durée.
```

```etape
Regarde deux fois. Repère une case trop longue. Raccourcis sa **Duration**, puis relis.
```

C'est le travail le plus utile du storyboard : régler le rythme quand un changement ne coûte rien.

```attention
L'animatique n'a pas de son. Tu ajouteras voix et musique au montage.
```

## Mettre en couleur

Le pot de peinture (`G`) remplit une zone fermée. Pour une suite de cases, tu n'as à colorier que la première.

```etape
Va sur la première case. Colorie deux ou trois zones avec `G`.
```

```etape
Va sur la deuxième case. Clique le bouton **Color from the previous panel**. Chaque zone fermée reprend la couleur de la même zone sur la case d'avant.
```

Si rien n'est repris, il y a deux raisons possibles. La case d'avant n'a pas de couleur dans ces zones. Ou ton trait n'est pas fermé.

```astuce
Ce bouton marche le mieux sur des cases dupliquées avec `Ctrl+D`, où le dessin a peu bougé.
```

## Exporter

Le menu **Export** propose trois sorties.

| Commande | Résultat |
|---|---|
| **Animatic as MP4…** | Une vidéo de l'animatique |
| **Contact sheet as PNG…** | La planche : toutes les cases sur une seule image |
| **Send to edit** | Un montage, avec une image par case |

```etape
Ouvre **Export**, puis **Contact sheet as PNG…**. Enregistre la planche. C'est le document à imprimer ou à montrer.
```

```etape
Ouvre **Export**, puis **Animatic as MP4…**.
```

```attention
L'export MP4 demande ffmpeg. S'il manque, Aryadne propose de l'installer dans ton dossier utilisateur, sans droits administrateur.
```

## Envoyer au montage

**Send to edit** est le passage normal vers l'étape suivante.

```etape
Ouvre **Export**, puis **Send to edit**. Un montage s'ouvre dans l'outil **Edit**.
verifier: fichier.ouvert = aryseq
```

Ce montage contient :

- une image par case, à sa durée ;
- un repère à chaque changement de scène.

Tu remplaces ensuite chaque case par le plan tourné ou rendu. Le rythme réglé dans l'animatique reste ta référence.

```etape
Dans le montage, appuie sur `Espace` pour lire. Tu retrouves ton animatique, prête à recevoir du son.
```

Tu apprendras à monter dans le parcours [Vidéo, son et code](../06-video-son-code/index.md).

## Limites du storyboard

- Un seul calque de dessin par case. Pour un dessin à calques, passe par la [retouche](../03-2d/index.md).
- Pas de son dans l'animatique.
- La caméra 2D va d'un cadre A à un cadre B, sans étape entre les deux.

```quiz
Que crée Send to edit ?
- [ ] Une vidéo MP4.
- [x] Un montage avec une image par case, à sa durée, et un repère par scène.
- [ ] Une copie du storyboard.
explication: Le montage créé s'ouvre dans l'outil Edit. Tu y remplaces les cases par les plans.
```

```quiz
Le bouton Color from the previous panel ne reprend rien. Pourquoi ?
- [x] Le trait n'est pas fermé.
- [x] La case d'avant n'a pas de couleur dans ces zones.
- [ ] Il faut d'abord exporter.
explication: Le coloriage se fait zone fermée par zone fermée, à partir de la case précédente.
```

```defi
Fais l'animatique d'une publicité de dix secondes en cinq cases, avec au moins un mouvement de caméra 2D, et exporte-la en MP4.
indice: Additionne tes durées : le panneau affiche le total. Garde la case la plus forte pour la fin.
```

## Récapitulatif

- `Espace` lit l'animatique : chaque case à sa durée, avec sa caméra 2D.
- **Color from the previous panel** reprend les couleurs de la case d'avant.
- **Export** sort un MP4, une planche PNG, ou envoie tout au montage.

Leçon suivante : [Ta première composition](04-premiere-composition.md)
