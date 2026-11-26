---
titre: Clés, lissages et rythme
niveau: intermédiaire
duree: 15 min
prerequis: 07-3d-bases/06-la-timeline
objectifs: Animer un rebond avec peu de clés ; Choisir le lissage de chaque passage ; Écraser et étirer ; Régler le rythme en déplaçant les clés
---

## Ce que tu vas réussir

Tu vas animer une balle qui rebondit trois fois et s'arrête. C'est l'exercice de base de tout animateur : il contient le poids, le rythme et l'élan.

```essayer
nouveau: aryscene
nom: balle-rebond
```

## Prépare

```etape
Ajoute une **Sphere**. Supprime le cube. Dans l'onglet **Scene**, mets **End** à 72 : trois secondes à 24 images par seconde.
verifier: scene.objets >= 3
```

```etape
Choisis la sphère. Passe en vue de face avec 1.
verifier: scene.selection
```

## Les poses clés

Une balle qui rebondit n'a que deux sortes de poses : en haut, et au sol.

```etape
À l'image 1, monte la balle bien haut. Appuie sur I.
```

```etape
À l'image 12, pose la balle au sol. Appuie sur I.
verifier: lecture.image >= 12
```

```etape
À l'image 24, remonte-la, moins haut que la première fois. Appuie sur I.
verifier: lecture.image >= 24
```

```etape
Lis avec Espace. La balle descend et remonte, mais sans poids : elle flotte.
```

## Le lissage donne le poids

Par défaut, chaque passage est **Soft** : départ et arrivée en douceur. Une balle ne ralentit pas avant de toucher le sol. Elle accélère.

Le lissage d'une clé règle le passage **vers la clé suivante**.

```etape
Clic droit sur la clé de l'image 1. Choisis **Ease in** : la balle quitte le sommet doucement, puis accélère jusqu'au sol.
```

```etape
Clic droit sur la clé de l'image 12. Choisis **Ease out** : la balle repart vite, puis ralentit en montant.
```

```etape
Relis. La balle a du poids.
```

| Passage | Lissage | Pourquoi |
|---|---|---|
| Du sommet vers le sol | **Ease in** | La chute accélère |
| Du sol vers le sommet | **Ease out** | La montée ralentit |

## Les rebonds suivants

Chaque rebond est plus bas et plus court que le précédent.

```etape
Pose la balle au sol à l'image 34, puis à mi-hauteur à l'image 42, au sol à l'image 49, à peine décollée à l'image 54, au sol à l'image 58.
verifier: lecture.image >= 58
```

```etape
Donne **Ease in** aux clés hautes et **Ease out** aux clés du sol.
```

```astuce
Maj+flèche gauche et Maj+flèche droite sautent de clé en clé. C'est le moyen le plus rapide de passer tes clés en revue.
```

## Écraser et étirer

Une balle souple s'écrase au contact et s'étire en vol. Une clé enregistre aussi la taille.

```etape
Va à l'image 12, où la balle touche le sol. Écrase-la un peu avec S puis l'axe vertical. Repose la clé avec I.
```

```etape
Va à l'image 10, juste avant le contact. Étire-la un peu en hauteur. Pose une clé.
```

```etape
Relis. Le rebond a de l'élasticité.
```

Reste discret : un écrasement léger se sent sans se voir.

## Le rythme

Le rythme, c'est l'écart entre les clés. Rapprocher deux clés accélère le passage. Les écarter le ralentit.

Les outils d'animation décalent les clés sans les reposer.

```etape
Dans la timeline, clique la baguette. La barre des outils d'animation s'ouvre.
```

| Geste | Raccourci |
|---|---|
| Avancer les clés de l'image courante | Alt+← |
| Les reculer | Alt+→ |
| Retirer une image après l'image courante | Alt+Maj+← |
| Ajouter une image après l'image courante | Alt+Maj+→ |

```etape
Va sur une clé du sol. Ajoute deux images après elle avec Alt+Maj+→. Toute la suite s'éloigne : ce rebond dure plus longtemps.
```

Annule avec Ctrl+Z si le rythme d'avant était meilleur.

## Tenir une pose

**Hold** garde la valeur jusqu'à la clé suivante, puis saute. C'est le lissage des poses tenues, et de l'animation saccadée façon stop motion.

```etape
Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```quiz
Quel lissage donner à la clé du sommet, pour une chute qui a du poids ?
- [x] Ease in : départ doux, puis accélération.
- [ ] Ease out : arrivée douce.
- [ ] Linear.
explication: Le lissage règle le trajet vers la clé suivante. Du sommet au sol, la balle accélère.
```

```quiz
Comment ralentir un passage entre deux clés ?
- [ ] En changeant la couleur de la clé.
- [x] En écartant les deux clés dans le temps.
- [ ] En ajoutant une lumière.
explication: Le rythme vient de l'écart entre les clés. Plus d'images entre deux poses, mouvement plus lent.
```

```defi
Anime une boule de pétanque, puis une balle de ping-pong, avec la même hauteur de départ.
indice: La boule lourde rebondit une fois, très bas, sans s'écraser. La balle légère rebondit haut, longtemps, à un rythme serré.
```

## En trois lignes

- Peu de clés, bien placées : en haut et au sol.
- **Ease in** en quittant le sommet, **Ease out** en quittant le sol.
- Le rythme est l'écart entre les clés. Alt et les flèches les décalent.

[Suite : L'éditeur de courbes](02-editeur-de-courbes.md)
