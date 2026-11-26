# Animation 3D

L'animation se fait par **clés**. Une clé enregistre la pose complète d'un objet (position, rotation, taille) à une image donnée. Entre deux clés, Aryadne calcule le passage.

## Réglages de la scène

Onglet `Scene` des propriétés : `fps` (24 par défaut), `Start`, `End`.

## La timeline 3D

Panneau `3D timeline`, sous la vue.

| Élément | Effet |
|---|---|
| Lecture | `Play / stop (Space)` |
| `Back to start` | Revient à la première image |
| `Frame` | L'image courante. Glisse ou tape une valeur |
| ◇ / ◆ | `Set a key for the selected object at this frame (K)`. Plein : il y a déjà une clé ici |
| ✕ | `Remove this frame's key` |
| Lissage | Le passage vers la clé suivante |
| Point rouge | Clé automatique : chaque déplacement pose une clé à l'image courante |
| Calques | Pelure d'oignon : l'objet choisi aux clés d'avant (rouge) et d'après (vert) |
| Baguette | Outils d'animation |
| Courbe | Éditeur de courbes |
| Fenêtre | Éditeur de courbes dans sa propre fenêtre, à poser sur un autre écran |
| Bandes | Montage d'animation (NLA) |

### Naviguer dans le temps

Ces gestes sont les mêmes dans toutes les timelines d'Aryadne.

| Geste | Effet |
|---|---|
| `Espace` | Lecture |
| `←` `→` | Une image |
| `Maj+←` `Maj+→` | Clé précédente, suivante |
| `Origine`, `Fin` | Début, fin |
| Molette | Défiler dans le temps |
| `Ctrl+molette` | Zoomer dans le temps, autour du pointeur |
| `Maj+molette` | Défiler en hauteur |
| `Alt+molette` | Zoomer en hauteur |
| Clic molette glissé | Se déplacer dans les deux sens |
| `F` | Tout voir |
| `=` et `−` | Zoomer, dézoomer |

## Poser des clés

1. Choisis un objet.
2. Va à une image.
3. Place l'objet.
4. `I` dans la vue (ou `K`, ou le losange de la timeline).
5. Va à une autre image, déplace l'objet, repose une clé.

Avec la **clé automatique** (point rouge), l'étape 4 est inutile : chaque déplacement pose sa clé.

Dans la timeline : clic droit sur une clé ouvre le menu des lissages. `Suppr` retire la clé survolée.

## Lissages

Le lissage d'une clé décrit le passage **vers la clé suivante**. La forme du losange le rappelle.

| Lissage | Forme | Mouvement |
|---|---|---|
| `Soft` | Losange | Départ et arrivée en douceur |
| `Linear` | Carré | Vitesse constante |
| `Ease in` | Sablier | Départ en douceur |
| `Ease out` | Sablier | Arrivée en douceur |
| `Bounce` | Rond | Rebondit à l'arrivée |
| `Spring` | Rond | Dépasse puis revient, comme un ressort |
| `Hold` | Carré coupé | La valeur tient jusqu'à la clé suivante |
| `Bézier (handles)` | | Poignées libres, réglées dans l'éditeur de courbes |

Ces lissages sont les mêmes dans la 3D, le Motion et le montage.

## Éditeur de courbes

`Maj+F6`, ou le bouton courbe. Une courbe par canal : position, rotation et taille, chacune en X, Y et Z.

### Gestes

| Geste | Effet |
|---|---|
| Clic, rectangle | Choisir des clés |
| Glisser une clé | La déplacer en temps et en valeur |
| Glisser une poignée | Changer la tangente |
| `A` | Tout choisir |
| `F` | Cadrer |
| `I` | Poser une clé à la tête de lecture sur les courbes affichées |
| `Suppr` | Supprimer les clés choisies |
| `Ctrl+C`, `Ctrl+V` | Copier, coller les clés à la tête de lecture |

### Barre d'outils

| Outil | Effet |
|---|---|
| Tangentes | `Auto tangents (flat at peaks)`, `Spline tangents (smooth)`, `Linear tangents`, `Flat tangents`, tangentes en escalier |
| `Unify tangents` | Les deux côtés alignés |
| `App easings` | Applique un des lissages de la liste ci-dessus |
| `Snap the selected keys to whole frames` | Cale les clés sur des images entières |
| `Tweener` | Place les clés choisies entre leur voisine d'avant (0 %) et d'après (100 %) |
| Avancer, reculer | Décale les clés choisies d'une image |
| `Smooth the selected keys` | Moyenne avec les voisines |
| `Simplify` | Retire les clés que les voisines redonnent déjà |
| `Euler filter` | Rotations sans sauts de 360° |
| `Buffer curves` | Garde une copie des courbes pour comparer. `Swap with the buffer curves` échange |
| `Normalize` | Affiche chaque courbe entre -1 et 1 |
| `Lock tangent weights` | Les poignées gardent leur longueur, seule la pente change |
| `Before the animation`, `After the animation` | `Constant`, `Cycle`, `Cycle with offset`, `Oscillate`. `Loop both sides` applique des deux côtés |
| `Key frame`, `Key value` | Valeurs exactes de la clé choisie |

## Outils d'animation

Le bouton baguette ouvre une barre d'outils, dans l'esprit d'Animbot.

| Outil | Raccourci | Effet |
|---|---|---|
| Tweener | | Un curseur entre la clé d'avant (0 %) et celle d'après (100 %). Au-delà : dépassement. Il faut une clé avant et une clé après l'image courante |
| Avancer les clés | `Alt+←` | Les clés de l'image courante avancent d'une image |
| Reculer les clés | `Alt+→` | Elles reculent d'une image |
| Retirer une image | `Alt+Maj+←` | Retire une image après l'image courante. La suite se rapproche |
| Ajouter une image | `Alt+Maj+→` | Ajoute une image après l'image courante. La suite s'éloigne |
| `Copy pose` | | Copie la pose des objets choisis |
| `Paste pose (and key it)` | | La colle et pose une clé |
| `Paste mirrored pose` | | La colle en miroir gauche-droite |
| `Mirror the pose left-right (and key it)` | | Miroir de la pose en place |
| Pose de repos | | Sans rotation, taille 1. Avec `Maj` : aussi à l'origine |
| Partager les clés | | Chaque objet choisi reçoit une clé partout où l'un d'eux en a une |
| Alléger | | Retire les clés que les voisines redonnent déjà. L'animation ne change pas |
| Décaler | | Glisser décale toute l'animation des objets choisis |

### Bibliothèque de poses

`Pose library` : donne un nom, `Save the pose of the selected objects`. Ensuite, sur chaque pose : `Apply to the selected objects (and set a key)`, `Apply mirrored`, `Delete the pose`. `Strength` applique en partie : 1 remplace la pose, 0,5 va à mi-chemin.

## Calques d'animation

Un calque d'animation ajoute un mouvement **par-dessus** l'animation de base, sans la toucher. Utile pour corriger une animation de mocap, ou ajouter une respiration sur un cycle.

Onglet `Scene`, section **Animation layers** :

| Élément | Effet |
|---|---|
| `New animation layer` | Crée un calque |
| `Base` | Les nouvelles clés vont dans l'animation de base |
| Point d'un calque | Les nouvelles clés (`I`) vont dans ce calque |
| `Layer weight` | Dose l'effet du calque |
| `Mute` | Le calque ne compte plus |
| Supprimer | Supprime le calque et ses clés |

Les clés d'un calque sont des **décalages** par rapport à ce qu'il y a dessous.

## Montage d'animation (NLA)

Le montage d'animation range des **actions** réutilisables en **bandes** sur la timeline. Exemple : un cycle de marche de 24 images, répété dix fois, qui avance à chaque cycle.

### Créer une action

1. Choisis les objets animés (un personnage et ses os, par exemple).
2. Ouvre le panneau **Animation mixer** (bouton bandes).
3. `Make an action from the selection's keys`. Les clés de la sélection et de ses enfants deviennent une action, posée en bande au même endroit.

### Poser et régler une bande

`Place a strip at the current frame` pose une bande d'une action existante.

| Réglage | Effet |
|---|---|
| `Start`, `End` | Où la bande commence et finit |
| `Repeat` | Nombre de répétitions. 2,5 : deux cycles et demi |
| `Duration ×` | Étire l'action dans le temps. 2 : deux fois plus lente |
| `Weight` | Dose la bande |
| `Fade in`, `Fade out` | Fondus d'entrée et de sortie, en images |
| `Additive` | Ajoute le mouvement de l'action à ce qu'il y a dessous, au lieu de le remplacer |
| `Travels each cycle` | Chaque répétition repart d'où la précédente est arrivée : un cycle de marche qui avance |
| `Hold the last pose` | Après la fin, la dernière pose tient |
| `Muted` | La bande ne compte plus |
| `Duplicate after` | Copie la bande à la suite |
| `Remove the strip` | Retire la bande |
| `Remove the action and all its strips` | Retire l'action |

### Ordre du calcul

Pour chaque objet, à chaque image :

1. les clés de base de l'objet ;
2. les bandes, rangée après rangée ;
3. les calques d'animation additifs de la scène.

## Formes (blendshapes)

Une forme est une autre version du même maillage : un sourire, un clignement. Un curseur la mélange à la forme de base, et ce curseur s'anime.

Onglet `Object`, section **Shapes** :

| Bouton | Effet |
|---|---|
| Ajouter depuis un fichier | Une forme depuis un OBJ. Il doit avoir le même nombre de sommets |
| Garder l'édition en cours | Le maillage revient à son état de départ, et ce que tu viens de modeler devient une forme. Ne marche que si tu n'as ni ajouté ni retiré de sommets |
| Curseur | La part de la forme dans le mélange |
| Losange | Clé à l'image actuelle. Clic droit : retirer toutes les clés |
| Retirer | Retire la forme. Le fichier reste |

Entre deux clés de forme, la valeur passe en ligne droite.

## Exporter l'animation

- FBX et GLB gardent les os et leur animation. Voir [Scène 3D](3d-scene.md#enregistrer-et-exporter).
- `Capture › Animation as video (MP4)…` pour une vidéo.

## Limites

- Une clé enregistre toujours la pose complète (position, rotation et taille). On ne peut pas poser une clé sur la position seule. L'éditeur de courbes permet ensuite de régler chaque canal.
- Les rotations sont en angles d'Euler (degrés, X puis Y puis Z). Le filtre d'Euler corrige les sauts, mais le blocage de cardan reste possible.
- Pas de pilotes (drivers) : une propriété ne peut pas en commander une autre par une formule en 3D. Les expressions existent dans le [Motion](motion.md#expressions).
- Les propriétés des lumières, des caméras (champ) et des matériaux ne s'animent pas par clés.
