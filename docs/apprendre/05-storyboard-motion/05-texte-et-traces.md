---
titre: Animer du texte et des tracés
niveau: intermédiaire
duree: 15 min
prerequis: 05-storyboard-motion/04-premiere-composition
objectifs: Faire entrer un texte morceau par morceau ; Faire se dessiner un tracé ; Transformer une forme avec des clés de forme ; Lire les clés dans l'éditeur de courbes ; Importer un logo SVG
---

## Ce que tu vas réussir à faire

Tu vas faire entrer un titre mot par mot, puis faire se dessiner un trait sous lui, comme tracé à la main.

```essayer
nouveau: arymotion
nom: titre-anime
```

```etape
Clique **Try it yourself**. Supprime les deux calques de départ : choisis-les dans la timeline, puis menu **Layer**, **Delete**. Pose un texte de trois ou quatre mots avec `T`.
verifier: onglet = motion
```

## Régler le texte

Choisis le calque de texte. L'onglet **Properties** montre ses réglages.

| Réglage | Effet |
|---|---|
| La police | Une police installée, ou **Font file (.ttf, .otf)…** |
| **Size** | La taille |
| **Spacing** | L'espace entre les lettres |
| L'alignement | **Left**, **Centered**, **Right** |

```etape
Choisis une police. Monte **Size** pour que le titre occupe la moitié de la largeur.
```

Une police variable montre ses axes : **Weight**, **Width**, **Slant**, **Italic**, **Optical size**. Chacun s'anime par clés. Un titre peut ainsi passer du maigre au gras.

## Le Text animator

Le **Text animator** fait entrer le texte morceau par morceau.

```etape
Place la tête de lecture au début avec `Origine`. Dans les propriétés du texte, clique **Animate in piece by piece**.
```

```etape
Choisis le découpage par **Word**. Relis avec `Espace` : les mots arrivent l'un après l'autre.
```

| Réglage | Effet |
|---|---|
| **Letter**, **Word**, **Line** | La taille des morceaux |
| **Offset** | Le délai entre deux morceaux |
| **Start x**, **Start y** | D'où part chaque morceau |
| **Start size** | La taille de départ |
| **Duration** | La durée de l'entrée d'un morceau |
| **Order** | **In order**, **Reversed**, **From the center out**, **Random** |

```etape
Mets **Start y** à une valeur positive : les mots montent en place. Baisse **Offset** pour une entrée plus serrée.
```

```etape
Passe **Order** sur **From the center out**, par **Letter**. Relis.
```

```astuce
Une entrée courte et un petit décalage font un titre vif. Une entrée longue et un grand décalage font un titre posé. Règle les deux ensemble.
```

## Un tracé qui se dessine

```etape
Appuie sur `P` pour **Freehand path**. Trace un trait souple sous le titre, d'un seul geste.
```

Un tracé a des réglages de forme : **Thickness**, **Closed**, une pointe de flèche au bout, **Fill the inside**.

Le **Trim path** règle la partie visible du tracé : un **Start** et un **End**, tous deux animables.

```etape
Place la tête de lecture là où le dernier mot est arrivé. Choisis le tracé et clique **Animate drawing**.
```

```etape
Relis. Le trait se dessine en une seconde, à partir de la tête de lecture.
```

**Erase at the end** fait l'inverse : le tracé s'efface pendant la dernière seconde du calque.

```etape
Monte **Thickness** et coche la pointe de flèche : ton trait devient une flèche qui se trace.
```

## Les clés de forme

Une forme peut se transformer en une autre. Tu poses une clé de forme, puis tu déplaces ses points à un autre instant.

```etape
Trace un rectangle avec `Q`. Au début de la timeline, pose une clé de forme sur ce calque.
```

```etape
Avance d'une seconde. Déplace les points du rectangle pour en faire un losange. Relis : la forme se transforme.
```

## L'éditeur de courbes

Le bouton **Graph editor** de la timeline montre toutes les clés du calque, avec leurs tangentes. Tu y vois la vitesse : une pente raide est un mouvement rapide.

```etape
Choisis un calque animé. Ouvre **Graph editor**. Repère où la courbe est raide et où elle est plate.
```

**Smooth keys** adoucit les clés : **The chosen key, toward the next one**, ou **All keys of the layer**.

## Pelure d'oignon et flou de mouvement

| Bouton de la timeline | Effet |
|---|---|
| **Onion skin** | Le calque choisi aux images d'avant, en rouge, et d'après, en vert |
| **Motion blur** | Un flou de mouvement. Clic droit : **Shutter angle** et **Samples** |

```attention
Le flou de mouvement se calcule sur le processeur. Avec beaucoup d'échantillons, la lecture et l'export ralentissent. Active-le à la fin.
```

## Importer un logo

**Import an SVG** change un logo en calques de forme. Chaque tracé du SVG devient un calque que tu animes : **Animate drawing** marche dessus.

Depuis le dessin vectoriel d'Aryadne, le bouton **Animate in Motion** fait le même trajet pour un plan de travail entier.

```quiz
Que règle Offset dans le Text animator ?
- [ ] La position du texte.
- [x] Le délai entre l'entrée de deux morceaux.
- [ ] La taille des lettres.
explication: Plus Offset est grand, plus les morceaux entrent espacés dans le temps.
```

```quiz
Que fait Animate drawing ?
- [x] Le tracé se dessine en une seconde à partir de la tête de lecture.
- [ ] Il transforme le texte en tracé.
- [ ] Il efface le tracé.
explication: Le bouton pose les clés du Trim path pour toi. Erase at the end fait l'effacement.
```

```defi
Fais une signature : écris ton prénom à main levée en plusieurs tracés, et fais-les se dessiner l'un après l'autre.
indice: Un tracé par lettre ou par geste. Applique **Animate drawing** à chacun, en avançant la tête de lecture d'une demi-seconde entre deux.
```

## Récapitulatif

- **Animate in piece by piece** fait entrer un texte par lettre, mot ou ligne.
- **Animate drawing** fait se dessiner un tracé. Le **Trim path** s'anime aussi à la main.
- **Graph editor** montre la vitesse du mouvement entre les clés.

Leçon suivante : [Parenté, caches, masques et effets](06-parente-masques-effets.md)
