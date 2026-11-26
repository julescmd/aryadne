---
titre: Outils, poses et calques d'animation
niveau: avancé
duree: 15 min
prerequis: 15-animation-rendu/02-editeur-de-courbes
objectifs: Copier, coller et inverser une pose ; Ranger des poses dans une bibliothèque ; Ajouter un mouvement par-dessus un autre avec un calque ; Doser et couper un calque
---

## Ce que tu vas réussir

Tu vas réutiliser une pose au lieu de la refaire. Puis tu ajouteras un mouvement par-dessus une animation finie, sans toucher à une seule de ses clés.

```essayer
nouveau: aryscene
nom: poses-et-calques
```

## Une animation de base

```etape
Choisis le cube. Pose une clé à l'image 1. À l'image 48, déplace-le de quelques unités sur X et pose une clé.
verifier: lecture.image >= 48
```

Le cube traverse la scène. C'est ton animation de base.

## Les outils d'animation

```etape
Dans la timeline, clique la baguette. La barre des outils s'ouvre.
```

### Le Tweener

```etape
Va à l'image 24. Bouge le curseur du **Tweener** vers 20 %. Une pose proche de celle de départ est posée.
verifier: lecture.image >= 24
```

Il faut une clé avant et une clé après l'image courante. Au-delà de 0 % ou de 100 %, tu obtiens un dépassement : l'élan avant un geste, ou le rebond après.

### Copier et coller une pose

```etape
Va à l'image 1. Clique **Copy pose**.
```

```etape
Va à l'image 72. Clique **Paste pose (and key it)**. Le cube revient à sa pose de départ, et une clé est posée.
verifier: lecture.image >= 72
```

| Outil | Effet |
|---|---|
| **Copy pose** | Copie la pose des objets choisis |
| **Paste pose (and key it)** | La colle et pose une clé |
| **Paste mirrored pose** | La colle en miroir gauche-droite |
| **Mirror the pose left-right (and key it)** | Inverse la pose en place |

Le miroir sert aux cycles de marche. Tu poses le pas gauche, tu colles son miroir : tu as le pas droit.

### Alléger et partager

Deux autres boutons de la barre rendent service. Survole-les pour lire leur rôle.

- L'un retire les clés que les voisines redonnent déjà. L'animation ne change pas, elle est seulement plus légère.
- L'autre donne à chaque objet choisi une clé partout où l'un d'eux en a une. Tes objets restent ainsi calés sur les mêmes images.

## La bibliothèque de poses

```etape
Ouvre **Pose library**. Donne un nom, par exemple « repos », puis clique **Save the pose of the selected objects**.
```

Sur chaque pose enregistrée :

- **Apply to the selected objects (and set a key)** ;
- **Apply mirrored** ;
- **Delete the pose**.

**Strength** applique en partie : à 1 la pose est remplacée, à 0,5 tu vas à mi-chemin. C'est un moyen rapide de pousser une pose vers une autre.

## Les calques d'animation

Ton cube traverse la scène. Tu veux maintenant qu'il tremble en avançant. Tu pourrais ajouter vingt clés à l'animation de base, et la rendre impossible à retoucher.

Un calque d'animation ajoute un mouvement **par-dessus**, sans toucher à la base.

```etape
Dans **Properties**, ouvre l'onglet **Scene**. Dans la section **Animation layers**, clique **New animation layer**.
```

```etape
Clique le point du nouveau calque. Les prochaines clés iront dans ce calque.
```

```etape
À l'image 1, pose une clé avec I. À l'image 12, monte un peu le cube, clé. À l'image 24, redescends-le, clé. Continue jusqu'à 72.
verifier: lecture.image >= 72
```

```etape
Lis. Le cube traverse la scène **et** monte et descend.
```

Les clés d'un calque sont des **décalages** par rapport à ce qu'il y a dessous.

### Doser et couper

```etape
Baisse **Layer weight** à 0,5. Le tremblement est deux fois plus faible.
```

```etape
Clique **Mute** sur le calque. Il ne compte plus : l'animation de base revient, intacte. Rallume-le.
```

```etape
Clique **Base**. Les nouvelles clés iront de nouveau dans l'animation de base.
```

```attention
Regarde toujours quel calque est actif avant de poser une clé. Une clé posée dans le mauvais calque est la première cause de confusion.
```

### À quoi ça sert

- Corriger une capture de mouvement sans toucher à ses centaines de clés.
- Ajouter une respiration sur un cycle de marche.
- Essayer une idée : si elle ne plaît pas, tu supprimes le calque.

```etape
Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```quiz
Que contiennent les clés d'un calque d'animation ?
- [ ] Des poses complètes qui remplacent la base.
- [x] Des décalages par rapport à ce qu'il y a dessous.
- [ ] Des copies des clés de base.
explication: Un calque s'ajoute à l'animation de base. Coupé, il rend la base intacte.
```

```quiz
À quoi sert Paste mirrored pose ?
- [x] À obtenir la pose opposée, gauche contre droite.
- [ ] À retourner l'objet la tête en bas.
- [ ] À inverser l'ordre des clés.
explication: Dans un cycle de marche, le pas droit est le miroir du pas gauche.
```

```defi
Ajoute un second calque qui fait tourner le cube sur lui-même, et dose les deux calques séparément.
indice: New animation layer, clique son point, pose des clés de rotation. Chaque calque a son Layer weight.
```

## En trois lignes

- **Copy pose**, **Paste pose**, **Paste mirrored pose** : une pose se réutilise.
- Un calque d'animation ajoute des décalages par-dessus la base.
- **Layer weight** dose, **Mute** coupe, **Base** revient à l'animation de base.

[Suite : Le montage d'animation](04-montage-d-animation.md)
