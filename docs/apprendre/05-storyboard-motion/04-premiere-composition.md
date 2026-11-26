---
titre: Ta première composition
niveau: débutant
duree: 15 min
prerequis: 01-demarrer/03-creer-et-enregistrer
objectifs: Créer une composition ; Ajouter des calques de texte et de forme ; Poser des clés pour animer ; Lire la timeline ; Appliquer un préréglage d'animation
---

## Ce que tu vas réussir à faire

Tu vas animer un carton de titre : un rectangle glisse en place, un texte apparaît, le tout en deux secondes.

Une composition a une taille, une cadence, une durée et une pile de calques. Tu animes chaque calque avec des **clés**.

```essayer
nouveau: arymotion
nom: titre
```

```etape
Clique **Try it yourself**. Une composition s'ouvre dans l'étape Motion.
verifier: onglet = motion
```

Une composition neuve n'est pas vide. Elle contient un titre qui entre lettre par lettre, souligné d'un trait qui s'étire. Elle dure 6 secondes.

```etape
Appuie sur `Espace` pour la lire. Appuie encore pour arrêter.
```

Tu vas refaire toi-même une animation de ce genre. Repars d'abord de zéro.

```etape
Choisis le calque du titre dans la timeline. Ouvre le menu **Layer** et clique **Delete**. Fais de même pour le trait.
```

## L'écran

| Panneau | Contenu |
|---|---|
| **Project (motion)** | Les compositions, **Add a layer**, les préréglages d'animation |
| **View (motion)** | La composition, avec les poignées du calque choisi |
| **Inspector (motion)** | Deux onglets : **Properties** et **Composition** |
| **Timeline (motion)** | Les calques et leurs clés |

## Ajouter des calques

Les outils de la vue créent un calque en le traçant.

| Outil | Touche |
|---|---|
| **Select** | `V` |
| **Rectangle** | `Q` |
| **Ellipse** | `E` |
| **Text** | `T` |
| **Freehand path** | `P` |

```etape
Appuie sur `Q`. Trace un rectangle large et bas au milieu de la vue : le bandeau du titre.
```

```etape
Appuie sur `T`. Clique sur le bandeau et tape ton titre.
```

Le premier calque de la liste est au-dessus. Le texte doit être au-dessus du bandeau.

```etape
Si le texte est caché par le bandeau, choisis-le dans la timeline, puis menu **Layer**, **Move up**.
```

```astuce
Le menu **Layer** crée aussi un texte, un rectangle, une ellipse ou une image. Le panneau **Project (motion)** ajoute les autres genres : bruit fractal, objet nul, précomposition, import d'un SVG. Survole ses boutons pour lire leur nom.
```

## Les quatre propriétés de base

Chaque calque a quatre propriétés : **Location**, **Scale**, **Rotation**, **Opacity**.

Sans clé, la valeur est fixe. Avec des clés, elle passe de l'une à l'autre.

## Poser deux clés

Tu vas faire entrer le bandeau par la gauche.

```etape
Reviens à l'outil de sélection avec `V`. Choisis le bandeau. Appuie sur `Origine` : la tête de lecture va au début.
```

```etape
Dans l'inspecteur, clique le losange à côté de **Location** : **Add a key at this time**. Une clé apparaît dans la timeline.
```

```etape
Glisse le bandeau hors de la vue, à gauche. C'est sa position de départ.
```

```etape
Avance la tête de lecture d'une vingtaine d'images. Glisse le bandeau au centre. Une deuxième clé se pose toute seule.
```

Une fois qu'un calque a des clés, le glisser dans la vue pose une clé à l'instant courant.

```raccourci
`K` pose une clé de position. `←` et `→` avancent d'une image.
```

## Lire

```etape
Appuie sur `Origine`, puis sur `Espace`. Le bandeau entre par la gauche.
```

| Geste dans la timeline | Effet |
|---|---|
| `Espace` | Lecture |
| `←` `→` | Image par image |
| `Origine` | Début |
| `Ctrl+molette` | Zoom dans le temps |
| Glisser une clé | La déplacer dans le temps |
| `Suppr` | Supprimer la clé, ou le calque choisi |

## Adoucir le mouvement

Le passage entre deux clés suit le lissage de la clé de départ.

```etape
Fais un clic droit sur la première clé du bandeau, dans la timeline. Le menu des lissages s'ouvre. Essaie-en deux et relis à chaque fois.
```

Un mouvement qui ralentit en arrivant paraît naturel. Un mouvement à vitesse constante paraît mécanique.

## Faire apparaître le texte

```etape
Choisis le calque de texte. Place la tête de lecture là où le bandeau s'arrête.
```

```etape
Dans la barre de la composition, ouvre le menu **Animation** et clique **Fade in**.
```

Le préréglage pose les clés pour toi, à partir de la tête de lecture.

| Préréglage | Effet |
|---|---|
| **Letter by letter in** | Le texte entre lettre par lettre |
| **Bounce** | Un rebond |
| **Soft slide** | Un glissement doux |
| **Fade in**, **Fade out** | Un fondu d'entrée, de sortie |
| **Full turn** | Un tour complet |

```etape
Relis depuis le début avec `Origine`, puis `Espace`.
```

## Le début et la fin d'un calque

Dans la timeline, chaque calque est une barre. Glisse ses bords pour régler son **Start** et son **End** : le calque n'existe qu'entre les deux.

```etape
Glisse le bord gauche de la barre du texte jusqu'à la fin du mouvement du bandeau.
```

## Régler la composition

L'onglet **Composition** de l'inspecteur règle le **Format**, le **Time** (cadence et **Duration**) et le **Background**.

```etape
Ouvre l'onglet **Composition**. Règle **Duration** sur 3 secondes.
```

```etape
Enregistre avec `Ctrl+S`.
```

```quiz
Combien de clés faut-il au minimum pour qu'une propriété bouge ?
- [ ] Une
- [x] Deux
- [ ] Quatre
explication: Une clé fixe une valeur à un instant. Il en faut deux pour passer de l'une à l'autre.
```

```quiz
Un calque a déjà des clés. Tu le glisses dans la vue. Que se passe-t-il ?
- [ ] Toute l'animation est décalée.
- [x] Une clé se pose à l'instant courant.
- [ ] Rien, il faut d'abord cliquer le losange.
explication: Dès qu'un calque est animé, le déplacer pose une clé là où se trouve la tête de lecture.
```

```defi
Fais sortir le titre : à la fin, le texte disparaît en fondu et le bandeau repart vers la droite.
indice: Place la tête de lecture avant la fin, applique **Fade out** au texte, puis pose deux clés de **Location** sur le bandeau.
```

## Récapitulatif

- `Q`, `E`, `T`, `P` créent des calques. Le premier de la liste est au-dessus.
- Le losange d'une propriété pose une clé. Deux clés font un mouvement.
- Le menu **Animation** pose des clés toutes prêtes à partir de la tête de lecture.

Leçon suivante : [Animer du texte et des tracés](05-texte-et-traces.md)
