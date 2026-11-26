---
titre: Le montage d'animation
niveau: avancé
duree: 15 min
prerequis: 15-animation-rendu/02-editeur-de-courbes
objectifs: Changer des clés en action ; Poser une action en bande ; Répéter, étirer et fondre une bande ; Faire avancer un cycle ; Comprendre l'ordre du calcul
---

## Ce que tu vas réussir

Tu vas animer un seul pas, puis en tirer dix sans poser une clé de plus. Le montage d'animation range les mouvements en blocs réutilisables.

```essayer
nouveau: aryscene
nom: montage-animation
```

## Deux mots

- Une **action** est un morceau d'animation réutilisable : un saut, un pas, un geste.
- Une **bande** est une action posée sur la timeline, à un endroit, avec ses réglages.

Une même action peut être posée en plusieurs bandes.

## Anime un cycle court

```etape
Choisis le cube. À l'image 1, pose une clé. À l'image 12, monte-le et avance-le un peu sur X, clé. À l'image 24, repose-le au sol, encore un peu plus loin sur X, clé.
verifier: lecture.image >= 24
```

Tu as un saut vers l'avant de 24 images.

## Fais-en une action

```etape
Dans la timeline, clique le bouton en forme de bandes. Le panneau **Animation mixer** s'ouvre.
```

```etape
Le cube choisi, clique **Make an action from the selection's keys**.
```

Les clés du cube sont devenues une action, posée en bande au même endroit. L'animation à l'écran n'a pas changé.

Si tu avais choisi un personnage, les clés de ses os seraient parties dans l'action aussi : elle prend la sélection et ses enfants.

## Répète

```etape
Choisis la bande. Mets **Repeat** à 4. Dans l'onglet **Scene**, mets **End** à 96.
```

```etape
Lis. Le cube saute quatre fois… en revenant chaque fois à son point de départ.
verifier: lecture.image >= 48
```

```etape
Coche **Travels each cycle**. Relis. Chaque saut repart d'où le précédent est arrivé : le cube avance.
```

C'est ainsi qu'on fait marcher un personnage : un cycle de marche, répété, qui avance à chaque cycle.

**Repeat** accepte les valeurs à virgule : 2,5 donne deux cycles et demi.

## Étire et dose

| Réglage | Effet |
|---|---|
| **Start**, **End** | Où la bande commence et finit |
| **Repeat** | Le nombre de répétitions |
| **Duration ×** | Étire l'action. 2 : deux fois plus lente |
| **Weight** | Dose la bande |
| **Fade in**, **Fade out** | Fondus d'entrée et de sortie, en images |
| **Hold the last pose** | Après la fin, la dernière pose tient |
| **Muted** | La bande ne compte plus |

```etape
Mets **Duration ×** à 2. Les sauts sont deux fois plus lents. Remets 1.
```

```etape
Coche **Hold the last pose**. Après le dernier saut, le cube reste où il a atterri.
```

## Une seconde bande

```etape
Va à l'image 97. Clique **Place a strip at the current frame** et choisis ton action. Dans l'onglet **Scene**, mets **End** à 144.
verifier: lecture.image >= 97
```

```etape
Sur cette bande, donne 8 images à **Fade in**. Elle entre en fondu.
```

**Duplicate after** copie une bande à la suite d'elle-même.

## Additive : ajouter au lieu de remplacer

Par défaut, une bande remplace ce qu'il y a dessous. Avec **Additive**, elle s'y ajoute. Un hochement de tête en bande additive se pose sur n'importe quelle marche.

## L'ordre du calcul

Pour chaque objet, à chaque image, Aryadne calcule dans cet ordre :

1. les clés de base de l'objet ;
2. les bandes, rangée après rangée ;
3. les calques d'animation additifs de la scène.

Un calque d'animation peut donc encore corriger le résultat d'un montage.

## Nettoyer

- **Remove the strip** retire une bande. L'action reste disponible.
- **Remove the action and all its strips** retire l'action et tout ce qui s'en sert.

```etape
Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```quiz
Quelle différence entre une action et une bande ?
- [x] L'action est le mouvement. La bande est ce mouvement posé à un endroit de la timeline.
- [ ] Aucune.
- [ ] La bande contient les clés, l'action les réglages.
explication: Une action se pose en autant de bandes qu'on veut, chacune avec sa répétition, sa durée, son poids.
```

```quiz
Ton cycle de marche se répète sur place. Que coches-tu ?
- [ ] Additive
- [ ] Hold the last pose
- [x] Travels each cycle
explication: Chaque répétition repart alors d'où la précédente est arrivée.
```

```defi
Enchaîne trois sauts normaux, puis deux sauts au ralenti, avec un fondu entre les deux.
indice: Deux bandes de la même action. La seconde avec Duration × à 2 et un Fade in.
```

## En trois lignes

- **Make an action from the selection's keys** change des clés en action.
- **Repeat** et **Travels each cycle** font avancer un cycle.
- Ordre du calcul : clés de base, bandes, puis calques d'animation.

[Suite : Squelette et rig automatique](05-squelette-et-rig.md)
