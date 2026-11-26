# Rig, mocap et suivi de caméra

Cette page couvre quatre outils liés : les squelettes, le rig automatique d'un bipède, la capture de mouvement (BVH) et le suivi de caméra.

## Squelettes

### Les os

Un os est un objet de la scène. Il a une position, une rotation, et une longueur le long de son axe Y.

- `Add › Bone` ajoute un os. Si un os est choisi, le nouveau est ajouté à son bout et devient son enfant.
- Dans l'onglet `Bone` : `Length`, et `Add a bone at the tip of this one`.

Une chaîne d'os reliés par la parenté forme un squelette. L'os du haut est l'os racine.

### Lier un modèle à un squelette

1. Place le squelette dans le modèle, en pose de repos (souvent une pose en T).
2. Choisis le modèle. Onglet `Object`, section `Skeleton` : choisis l'os racine.
3. La liaison se fait avec des **poids automatiques**. La pose actuelle du squelette devient sa pose de repos.

| Réglage | Effet |
|---|---|
| `Skeleton` | L'os racine du squelette qui déforme le modèle. `None` : aucun |
| `Bone reach` | Portée de l'influence des os. Plus grand : chaque os entraîne la peau plus loin, les plis sont plus doux |

Ensuite, tourner un os déforme le modèle (skinning linéaire).

### Pose de repos

Onglet `Bone`, section **Skeleton** :

- `Save the current pose as the skeleton's rest pose` ;
- `Back to the rest pose`.

Sans modèle lié, il n'y a pas encore de pose de repos : un message le dit.

### Cinématique inverse (IK)

Avec l'IK, tu déplaces une **cible** (la main, le pied) et la chaîne d'os se plie toute seule pour l'atteindre.

Onglet `Bone`, section **Inverse kinematics** :

| Réglage | Effet |
|---|---|
| `Target` | L'objet que le bout de l'os cherche à toucher. Souvent un groupe vide |
| `Bones in chain` | Nombre d'os de la chaîne, cet os compris. 2 pour un bras : avant-bras et bras |

**Exemple.** Pour un bras : choisis l'os de l'avant-bras, mets comme cible un groupe vide placé au poignet, chaîne de 2. Déplace le groupe vide : le coude se plie.

### Limites des squelettes

- Les poids sont automatiques. Il n'y a pas d'outil pour peindre les poids à la main.
- Le skinning est linéaire : un coude très plié perd du volume.
- L'IK utilise la méthode CCD. Elle n'a pas de réglage de direction du coude (pas de « pole target »).

## Rig automatique d'un bipède

Le rig bipède construit un squelette complet à partir de quelques repères posés sur le personnage.

### Pas à pas

1. Choisis le personnage. Il doit être debout sur l'axe Y et tourné vers +Z.
2. `Add › Biped rig › Place the markers`.
3. Des repères apparaissent à leur place probable, **côté gauche seulement** : bassin, poitrine, cou, tête, épaule, coude, poignet, hanche, genou, cheville, orteils. Ils sont rangés dans un groupe « repères de rig ».
4. Déplace chaque repère sur son articulation.
5. `Add › Biped rig › Generate the skeleton`.

Le squelette est construit des deux côtés. Des cibles de cinématique inverse sont créées aux mains et aux pieds. Le personnage est lié au squelette.

Si la boîte du personnage est large, les repères des bras partent en pose en T. Sinon ils longent le corps. Sans personnage choisi, les repères sont posés à taille humaine.

### Limites

- Bipèdes seulement. Pas de doigts, pas de visage, pas de queue.
- Les repères supposent un personnage symétrique.

## Capture de mouvement (BVH)

Un fichier BVH contient un squelette et son mouvement, image par image.

`Add › Motion capture (BVH)…`, puis choisis le fichier.

Deux cas :

| Situation | Résultat |
|---|---|
| Aucun os choisi | Un nouveau squelette est créé : un os par articulation du BVH. Message : « bones imported » |
| Un os choisi | Le mouvement va sur le squelette de cet os. Les os sont retrouvés **par leur nom** (Hips, Spine, LeftArm…). Message : « bones animated » |

Dans les deux cas, une clé est posée par image, ramenée à la cadence de la scène.

Si aucun os du squelette ne porte le nom d'une articulation du BVH, un message le signale.

### Conseils

- Pour corriger un mouvement de mocap sans toucher à ses clés, utilise un [calque d'animation](3d-animation.md#calques-danimation).
- Pour en faire un cycle réutilisable, fais-en une action dans le [montage d'animation](3d-animation.md#montage-danimation-nla).
- Beaucoup de clés : `Simplify` dans l'éditeur de courbes retire celles qui ne servent pas.

### Limites

- Pas de retargeting : le mouvement ne s'adapte pas à un squelette de proportions ou de noms différents. Les noms doivent correspondre.
- Seul le format BVH est lu.

## Suivi de caméra (matchmove)

Le suivi de caméra retrouve le mouvement de la caméra d'une vidéo, et pose dans la scène une caméra animée, des repères de la scène filmée, et la vidéo en fond. Tu peux alors placer des objets 3D qui tiennent dans le plan.

### Pas à pas

1. `Add › Tracked camera from a video…`, choisis la vidéo.
2. Dans la fenêtre **Camera tracking** :
   - laisse `Estimate the lens field of view` coché, ou décoche et donne le champ horizontal de l'objectif s'il est connu. Le résultat est plus juste ;
   - `Track`.
3. À la fin, un message donne le nombre d'images suivies et l'erreur moyenne en pixels.
4. La scène contient une `Tracked camera`, des repères, et la vidéo en fond. `0` regarde par la caméra.

### Comment ça marche

1. Des coins bien marqués sont détectés dans l'image et suivis d'image en image.
2. À partir de ces pistes, Aryadne calcule la position de la caméra à chaque image et un nuage de points de la scène.
3. Le tout est posé dans la scène. La première image résolue donne l'orientation du monde. La scène est mise à l'échelle pour que les points soient à environ cinq unités de la caméra.

### Conditions

- **La caméra doit se déplacer** dans le plan. Un simple panoramique sur trépied ne donne pas de relief : le calcul échoue ou donne n'importe quoi.
- Seules les premières secondes de la vidéo sont suivies.
- Il faut des détails nets dans l'image : textures, coins, objets fixes. Un ciel uni ou une image floue ne se suivent pas.
- ffmpeg est nécessaire pour lire la vidéo.

### Lire l'erreur

L'erreur moyenne est la distance, en pixels, entre où un point devrait apparaître et où il apparaît vraiment. En dessous d'un pixel, le suivi est bon. Au-dessus de 2 ou 3, les objets ajoutés glisseront.

### Limites

- Pas de réglage manuel des points suivis, pas de masque pour exclure une zone qui bouge (un personnage, une voiture).
- L'échelle et l'orientation du sol sont arbitraires : à corriger à la main en déplaçant et tournant le groupe.
- Pas de correction de la distorsion de l'objectif.
- Le suivi d'objet (un objet qui bouge devant une caméra fixe) n'existe pas.

Le suivi d'un simple point en 2D, pour accrocher un titre à un élément d'une vidéo, se trouve dans le [montage](montage.md#suivi-et-stabilisation).
