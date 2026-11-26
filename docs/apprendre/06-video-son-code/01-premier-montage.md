---
titre: Ton premier montage
niveau: débutant
duree: 20 min
prerequis: 01-demarrer/05-fichiers-du-coffre
objectifs: Créer un montage ; Importer des médias et en choisir une partie ; Poser, couper et réordonner des plans ; Lire la timeline ; Enregistrer
---

## Ce que tu vas réussir à faire

Tu vas assembler trois plans en une petite séquence, retirer les longueurs et la lire d'un bout à l'autre.

Un montage ne copie pas tes vidéos. Il garde leur chemin, et ce que tu en fais. Les médias restent à leur place.

```essayer
nouveau: aryseq
nom: premier-montage
```

```etape
Clique **Try it yourself**. Un montage vide s'ouvre dans l'étape Video, outil **Edit**.
verifier: fichier.ouvert = aryseq
```

## L'écran

| Panneau | Contenu |
|---|---|
| **Media (edit)** | Deux onglets : **Media**, tes fichiers, et **Source**, le média ouvert |
| **Monitor (edit)** | L'image à la tête de lecture |
| **Inspector (edit)** | Trois onglets : **Clip**, **Effects**, **Advanced** |
| **Edit timeline** | Les pistes. L'image, V1, V2, au-dessus. Le son, A1, A2, en dessous |

## Importer des médias

```etape
Glisse deux ou trois vidéos de ton ordinateur dans le panneau **Media (edit)**. Choisis **Link (keep in place)** si Aryadne demande quoi en faire.
```

Le panneau accepte les vidéos, les images, les sons, les images à calques, les compositions Motion et d'autres montages.

Chaque média est une carte. Les filtres du haut trient : **All**, **Not used yet**, **Videos**, **Images**, **Sounds**.

## Choisir une partie d'un plan

Tu veux rarement un plan entier.

```etape
Survole une carte : le plan défile sous la souris. Glisse sur la barre du bas de la carte pour choisir la partie à garder.
```

```etape
La souris sur la carte, appuie sur `E` : **Add at the end**. La partie choisie se pose à la fin de la timeline.
```

```etape
Fais de même avec tes deux autres médias. Tu as trois plans à la suite sur V1.
```

| Geste sur une carte | Effet |
|---|---|
| `E` | **Add at the end** : ajoute à la fin |
| `Maj+E` | **Insert at the playhead** : insère à la tête de lecture, la suite recule |
| Glisser vers la timeline | Pose le plan où tu le lâches |
| Clic droit | **Whole media**, **Open in the source**, **Remove from project** |

## Lire

```etape
Appuie sur `Origine` pour revenir au début, puis sur `Espace` pour lire.
```

| Touche | Effet |
|---|---|
| `Espace` | Lire, arrêter |
| `J`, `K`, `L` | Arrière, arrêt, lecture |
| `←` `→` | Une image |
| `↑` `↓` | Coupe précédente, suivante |
| `Origine`, `Fin` | Début, fin |
| `=`, `-` | Zoomer, dézoomer dans le temps |
| `\` | Tout voir |

```attention
Pas d'image dans le moniteur ? Il manque ffmpeg. Aryadne propose de l'installer. **Install sound (ffplay)** ajoute la lecture du son.
```

## La timeline magnétique

Le bouton **Magnetic timeline** change la façon dont les plans se tiennent.

- Active : la piste V1 raconte l'histoire, sans trous. Retirer un plan resserre la suite. Titres et sons restent accrochés à leur plan.
- Désactivée : la timeline est libre. Les clips restent où tu les poses.

Garde-la active pour cette leçon.

## Raccourcir un plan

```etape
Avec l'outil de sélection (`V`), glisse le bord droit du premier plan vers la gauche. Le plan raccourcit et la suite se resserre.
```

## Couper

```etape
Appuie sur `C` pour le cutter. Clique au milieu du deuxième plan : il est coupé en deux. Reviens à la sélection avec `V`.
```

```etape
Clique la deuxième moitié et appuie sur `Suppr`. Elle disparaît et la suite se rapproche.
```

```attention
Le montage annonce `Ctrl+K` pour couper à la tête de lecture. Mais `Ctrl+K` est aussi le raccourci global de la palette de commandes, et la palette peut passer devant. Le cutter (`C`) marche toujours. Si tu tiens à `Ctrl+K` pour couper, donne un autre raccourci à la palette dans **Edit › Keyboard shortcuts…**.
```

## Rogner d'une touche

Deux touches rognent jusqu'à la tête de lecture, sans laisser de trou.

| Touche | Effet |
|---|---|
| `Q` | Retire le début du plan, jusqu'à la tête de lecture |
| `W` | Retire la fin du plan, depuis la tête de lecture |

```etape
Lis le troisième plan. Arrête-toi quand l'action commence vraiment. Appuie sur `Q`.
```

C'est la façon la plus rapide de dégrossir : tu lis, tu t'arrêtes, tu rognes.

## Changer l'ordre

```etape
Glisse le troisième plan devant le premier. Les autres lui font place.
```

## Annuler

```raccourci
`Ctrl+Z` annule, `Ctrl+Maj+Z` rétablit. Au montage, c'est bien `Ctrl+Maj+Z`, pas `Ctrl+Y`.
```

## Enregistrer

```etape
Enregistre avec `Ctrl+S`.
```

Le fichier `.aryseq` est petit : il ne contient que la recette du montage. Si tu déplaces tes vidéos, le montage ne les trouvera plus.

```quiz
Que contient un fichier de montage `.aryseq` ?
- [ ] Une copie de toutes tes vidéos.
- [x] Le chemin des médias et ce que tu en fais.
- [ ] Une vidéo déjà exportée.
explication: Les médias restent à leur place. Le montage est une recette.
```

```quiz
Timeline magnétique active, tu supprimes un plan au milieu. Que se passe-t-il ?
- [ ] Un trou reste à sa place.
- [x] La suite se resserre.
- [ ] Tout le montage est effacé.
explication: La piste V1 reste sans trous. Désactive la timeline magnétique pour garder les trous.
```

```quiz
Que fait la touche Q ?
- [ ] Elle quitte le montage.
- [x] Elle rogne le début du plan jusqu'à la tête de lecture.
- [ ] Elle prend l'outil de sélection.
explication: Q rogne le début, W rogne la fin. La suite se resserre.
```

```defi
Monte une séquence de trente secondes avec cinq plans, sans un seul temps mort.
indice: Pose tout avec `E`, puis lis et rogne avec `Q` et `W`. Regarde le total en bas de la timeline.
```

## Récapitulatif

- Glisse sur la barre d'une carte pour choisir une partie. `E` l'ajoute à la fin.
- Le cutter (`C`) coupe, `Q` et `W` rognent, `Suppr` retire.
- La timeline magnétique garde V1 sans trous.

Leçon suivante : [Affiner : outils, titres, transitions, couleur](02-affiner-le-montage.md)
