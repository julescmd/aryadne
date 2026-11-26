---
titre: Squelette et rig automatique
niveau: avancé
duree: 20 min
prerequis: 07-3d-bases/03-ajouter-et-organiser, 15-animation-rendu/01-cles-lissages-et-rythme
objectifs: Construire une chaîne d'os ; Lier un modèle à un squelette ; Plier un membre par cinématique inverse ; Générer un squelette de bipède à partir de repères
---

## Ce que tu vas réussir

Tu vas construire un bras de trois os, le plier en déplaçant une seule cible, puis générer un squelette complet de bipède en plaçant quelques repères.

```essayer
nouveau: aryscene
nom: squelette
```

## Un os

Un os est un objet de la scène. Il a une position, une rotation, et une longueur le long de son axe Y.

```etape
Ouvre **Add** et choisis **Bone**. Un os apparaît.
verifier: scene.objets >= 4
```

```etape
L'os choisi, ouvre encore **Add**, puis **Bone**. Le nouvel os est ajouté au bout du premier, et devient son enfant.
verifier: scene.objets >= 5
```

```etape
Recommence une fois. Tu as une chaîne de trois os : bras, avant-bras, main.
verifier: scene.objets >= 6
```

Une chaîne d'os reliés par la parenté forme un squelette. L'os du haut est l'os **racine**.

Dans l'onglet **Bone** des propriétés : **Length**, et le bouton **Add a bone at the tip of this one**.

## Tourner la chaîne

```etape
Choisis le premier os et tourne-le avec R. Toute la chaîne suit.
```

```etape
Choisis le deuxième os et tourne-le. Seuls lui et le troisième bougent.
```

C'est la cinématique directe : tu tournes chaque os, de la racine vers le bout. Précis, mais lent pour poser une main quelque part.

## La cinématique inverse

Avec la cinématique inverse, tu déplaces une **cible**. La chaîne se plie toute seule pour l'atteindre.

```etape
Ajoute un **Empty** par le menu **Add**. Place-le au bout du deuxième os, là où serait le poignet.
verifier: scene.objets >= 7
```

```etape
Choisis le deuxième os, l'avant-bras. Dans l'onglet **Bone**, section **Inverse kinematics**, règle **Target** sur ton **Empty**, et **Bones in chain** sur 2.
```

```etape
Choisis l'**Empty** et déplace-le avec G. Le coude se plie pour suivre.
```

| Réglage | Sens |
|---|---|
| **Target** | L'objet que le bout de l'os cherche à toucher |
| **Bones in chain** | Le nombre d'os de la chaîne, cet os compris. 2 pour un bras |

Pour animer, tu poses des clés sur la cible. Deux clés suffisent pour un geste entier.

```attention
Il n'y a pas de réglage pour la direction du coude. Si le bras se plie du mauvais côté, tourne un peu les os à la main avant de déplacer la cible.
```

## Lier un modèle à un squelette

Un squelette sert à déformer un modèle.

1. Place le squelette dans le modèle, en pose de repos.
2. Choisis le modèle. Onglet **Object**, section **Skeleton** : choisis l'os racine.
3. La liaison se fait avec des poids automatiques. La pose actuelle du squelette devient sa pose de repos.

**Bone reach** règle la portée de l'influence des os. Plus grand : chaque os entraîne la peau plus loin, et les plis sont plus doux.

Dans l'onglet **Bone**, section **Skeleton** : **Save the current pose as the skeleton's rest pose** et **Back to the rest pose**.

## Le rig automatique d'un bipède

Construire un squelette complet à la main est long. Le rig bipède le fait à partir de quelques repères.

```essayer
nouveau: aryscene
nom: rig-bipede
```

```etape
Supprime le cube. Ouvre **Add**, puis **Biped rig**, puis **Place the markers**.
verifier: scene.objets >= 3
```

Des repères apparaissent, du **côté gauche seulement** : bassin, poitrine, cou, tête, épaule, coude, poignet, hanche, genou, cheville, orteils. Sans personnage choisi, ils sont posés à taille humaine.

Avec un vrai personnage, tu le choisis d'abord. Il doit être debout sur l'axe Y et tourné vers +Z. Les repères se placent alors à leur endroit probable.

```etape
Choisis le repère du coude et déplace-le légèrement vers l'arrière. Fais de même pour le genou, vers l'avant.
verifier: scene.selection
```

Sur un personnage, tu amènes chaque repère sur son articulation. Un coude et un genou légèrement pliés aident la cinématique inverse à choisir le bon sens.

```etape
Ouvre **Add**, puis **Biped rig**, puis **Generate the skeleton**.
```

Le squelette est construit des deux côtés. Des cibles de cinématique inverse sont créées aux mains et aux pieds. Si un personnage était choisi, il est lié au squelette.

```etape
Choisis la cible d'une main et déplace-la. Le bras suit. Fais de même avec un pied.
verifier: scene.selection
```

## Ce qui n'existe pas encore

- Bipèdes seulement. Pas de doigts, pas de visage, pas de queue.
- Les poids sont automatiques. Pas d'outil pour les peindre à la main.
- Un coude très plié perd du volume : la déformation est linéaire.

```etape
Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```quiz
Avec la cinématique inverse, que déplaces-tu ?
- [ ] Chaque os, l'un après l'autre.
- [x] Une cible : la chaîne se plie pour l'atteindre.
- [ ] Le modèle entier.
explication: Tu poses la main, et le coude se place tout seul. C'est l'inverse de tourner chaque os.
```

```quiz
De quel côté poses-tu les repères du rig bipède ?
- [x] Du côté gauche seulement.
- [ ] Des deux côtés.
- [ ] Au centre.
explication: Le squelette est ensuite construit des deux côtés. Les repères supposent un personnage symétrique.
```

```defi
Fais faire un signe de la main à ton squelette, en quatre clés.
indice: Des clés sur la cible de la main seulement. Copy pose et Paste pose pour revenir à la pose de départ.
```

## En trois lignes

- **Add**, **Bone**, sur un os choisi : une chaîne.
- **Target** et **Bones in chain** : une cible plie le membre.
- **Biped rig** : des repères à gauche, un squelette complet des deux côtés.

[Suite : Mocap et suivi de caméra](06-mocap-et-suivi-de-camera.md)
