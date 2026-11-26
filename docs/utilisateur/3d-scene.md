# Scène 3D

Étape **3D**, outil `Scene`. Fichier : `.aryscene`.

Une scène réunit des objets : modèles, lumières, caméras, os, groupes vides, images de référence. La scène ne contient pas les maillages. Elle pointe vers des fichiers de modèles, qui restent à leur place sur le disque.

## Modèle seul ou scène

- Ouvrir un fichier de modèle (`.obj`, `.fbx`, `.glb`…) l'affiche dans une **visionneuse**. On peut tourner autour, changer l'éclairage, capturer une image. On ne peut pas le modifier.
- Le bouton **Open in a scene** le place dans une scène. C'est là que l'on modélise, sculpte, peint, anime.

## Créer une scène

`File › New › 3D scene`. Deux départs : `Cube, light and camera`, ou `Empty scene`.

## Naviguer dans la vue

Mode Blender (par défaut). Le mode Maya se choisit dans [Réglages](reglages.md#navigation-3d).

| Geste | Effet |
|---|---|
| Molette enfoncée + glisser | Tourner |
| `Maj` + molette enfoncée | Déplacer la vue |
| Molette, ou `Ctrl` + molette enfoncée | Zoom |
| `Alt` + clic gauche glissé | Tourner |
| `1`, `3`, `7` | Vues de face, de droite, de dessus |
| `Maj+1`, `Maj+3`, `Maj+7` | Vues opposées : arrière, gauche, dessous |
| `5` | Perspective / orthographique |
| `0` | Regarder par la caméra de la scène. `0` de nouveau, ou tourner, pour sortir |
| `Origine` | Tout cadrer |
| `Maj+F` | Cadrer la sélection |
| `.` | Menu circulaire du point de pivot |
| `Maj+S` | Menu circulaire du curseur 3D |

Dans la visionneuse d'un modèle seul, glisser avec le bouton gauche tourne, `Maj` + glisser déplace, et `.` cadre.

Menu **View** de la barre : les six vues, `Frame all (Home)`, `Frame selected (Shift+F)`, `Orbit around selection`, `Z axis up`, `Orthographic view (5)`, `Turntable` (rotation automatique).

## La barre de la vue

De gauche à droite :

| Élément | Rôle |
|---|---|
| Mode | `Object mode`, `Edit mode`, `Sculpt`, `Paint`, `UV & materials` |
| `View` | Vues et cadrage |
| `Select` | `All (A)`, `None (Alt+A)`, `Box (B)` |
| `Add` | Formes, lumières, caméra, os, mocap, texte 3D |
| `Object` | Dupliquer, supprimer, cacher, parenter, poser une clé |
| Outils | `Move`, `Rotate`, `Scale` |
| `Render ▾` | Moteur de la vue et réglages du rendu |
| `Nodes ▾` | Matériaux en nœuds |
| `Modifiers` | La pile de modificateurs |
| Compteur | Nombre d'objets et de sommets affichés |

## Sélectionner

| Geste | Effet |
|---|---|
| Clic | Choisir un objet |
| `Maj+clic` | Ajouter à la sélection. Le dernier cliqué est l'objet actif |
| Glisser dans le vide | Sélection au rectangle |
| `A` | Tout |
| `Alt+A` | Rien |
| `B` | Rectangle |
| `Ctrl+I` | Inverser |

La liste des objets (panneau `Scene`) sert aussi à choisir. L'œil affiche ou cache. Clic droit : `Duplicate`, `Remove from scene`.

## Déplacer, tourner, redimensionner

Deux façons, comme dans Blender.

**Avec les poignées.** Choisis l'outil `Move`, `Rotate` ou `Scale`. Des flèches, des anneaux ou des cubes apparaissent sur l'objet. Tire une flèche pour un axe, un petit plan pour deux axes, le centre pour un déplacement libre.

**Au clavier.**

1. `G` (déplacer), `R` (tourner) ou `S` (taille).
2. Bouge la souris.
3. `X`, `Y` ou `Z` impose un axe.
4. Clic ou `Entrée` valide. Clic droit ou `Échap` annule.

Le panneau `Properties › Object › Transform` donne les valeurs exactes : `Location`, `Rotation` (degrés), `Scale`. Un clic sur le nom d'une ligne la remet à zéro. `Uniform` lie les trois axes de la taille.

### Point de pivot

Menu `Pivot point`, ou `.` : `Median point`, `3D cursor`, `Individual origins`, `Active element`.

### Curseur 3D

`Maj+S` ouvre un menu circulaire : `Cursor to selected`, `Selection to cursor`, `Cursor to world origin`, `Cursor to active`, `Cursor to grid`.

## Ajouter des objets

Menu `Add`, ou `Maj+A` dans la vue (le même menu sous le curseur).

| Entrée | Ce qu'elle ajoute |
|---|---|
| Shapes | `Cube`, `Sphere`, `Plane`, `Cylinder`, `Cone`, `Torus` |
| `Sun light` | Une direction. Tourne-la pour changer d'où vient la lumière |
| `Point light` | Éclaire autour d'elle. Déplace-la |
| `Spot` | Un cône, vers le bas de son axe Y. Réglages : `Angle °`, `Falloff` |
| `Area light` | Un panneau lumineux, ombres douces. Réglage : `Scale` (largeur, hauteur) |
| `Camera` | Un point de vue enregistré |
| `Empty` | Un objet sans forme, pour déplacer plusieurs objets ensemble |
| `Bone` | Un os. Ajouté au bout de l'os choisi. Voir [Rig](3d-rig-mocap.md) |
| `Motion capture (BVH)…` | Un squelette animé |
| `Node geometry` | Une géométrie de nodes (`.arygeo`) : un tissu simulé, des particules. `New simulated flag` en crée une à côté de la scène ; `FROM THE VAULT` liste celles du coffre |
| `3D text…` | Du texte extrudé en volume, enregistré en `.obj` à côté de la scène |
| `Tracked camera from a video…` | Suivi de caméra |
| `Biped rig` | Rig automatique d'un personnage |
| `Models in the vault` | N'importe quel modèle du coffre |
| `Pick a file…` | Un modèle ou une image du disque |

Tu peux aussi glisser des modèles sur la fenêtre, ou un `.arygeo` depuis le panneau des fichiers.

### Géométrie de nodes

Une géométrie de nodes posée dans la scène suit la timeline : l'image de la scène pilote l'image de sa simulation. Elle a une transformation, reçoit un matériau (onglet `Material`) et sort au rendu.

Onglet `Object`, section `Simulation` :

| Réglage | Effet |
|---|---|
| Icône du graphe | Ouvre le graphe dans l'onglet **Simulation**. Ce qu'on y enregistre se voit aussitôt dans la scène |
| Icône de téléchargement | Exporte la simulation en suite d'OBJ, un fichier par image, dans un dossier à côté de la scène |
| `Start (frames)` | Décalage de départ. 10 : la simulation part à la dixième image. -10 : elle a dix images d'avance |
| `Simulation frame` | L'image de la simulation montrée, et sa durée |

Elle ne se modifie pas en mode édition : elle se modifie dans son graphe. Détail : [Géométrie et simulation en nœuds](noeuds-geometrie.md#dans-une-scène-3d).

Une **image** ajoutée devient un plan de référence dans la scène, avec une `Opacity` réglable.

### Texte 3D

`Add › 3D text…` : le texte (`Entrée` pour une nouvelle ligne), `Line height`, `Thickness`, `Edge rounding`. Le texte est tracé avec la police Inter Bold puis extrudé par voxels.

**Limite.** Une seule police. Le maillage vient d'une grille de voxels : il est dense et régulier, pas optimisé.

## Opérations sur les objets

| Raccourci | Menu `Object` | Effet |
|---|---|---|
| `Maj+D` | `Duplicate` | Dupliquer |
| `X` ou `Suppr` | `Delete` | Retirer de la scène (le fichier du modèle reste) |
| `H` | `Hide` | Cacher |
| `Alt+H` | `Show all` | Tout afficher |
| `P` | `Parent to active` | Les objets choisis deviennent enfants de l'objet actif |
| `Alt+P` | `Clear parent` | Retirer le parent |
| `Ctrl+J` | `Join into active` | Joindre plusieurs maillages en un. Choisis au moins deux objets à maillage, l'actif en dernier |
| `I` | `Insert key` | Poser une clé d'animation |
| `Tab` | `Edit mode` | Entrer en mode édition |

Clic droit sur un objet : le même genre de menu, plus `Reset (location, rotation, scale)` et `Remove material`.

## Le panneau des propriétés

Six onglets.

| Onglet | Contenu |
|---|---|
| `Object` | Transformation, parent, contraintes, opacité, couleur, squelette, formes |
| `Modifiers` | La pile de modificateurs de l'objet. Voir [Modélisation](3d-modelisation.md#modificateurs) |
| `Material` | Le matériau en nœuds de l'objet. Voir [Matériaux en nœuds](noeuds-materiaux.md) |
| Données | `Light`, `Camera` ou `Bone` selon l'objet |
| `Scene` | `fps`, `Start`, `End`, calques d'animation, superpositions |
| `World` | Environnement et éclairage de la vue |

### Contraintes (onglet Object)

| Contrainte | Effet |
|---|---|
| `Parent` | L'enfant suit les déplacements, rotations et changements de taille du parent |
| `Aim at` | L'objet se tourne toujours vers la cible, même animée. Sa rotation X et Y ne se règle plus à la main |
| `Copy location` | L'objet se place toujours sur l'origine de la cible |
| `Copy rotation` | L'objet prend la rotation de la cible (objets sans parent) |
| `Limit location` | Borne la position entre un `Min` et un `Max` par axe |

### Lumière

`Strength`, la forme (`Sun`, `Point`, `Spot`, `Rectangle`), `Range` pour une ponctuelle. La couleur se règle dans l'onglet `Object`.

### Caméra

| Réglage | Effet |
|---|---|
| `Look through` | Aligne la vue sur cette caméra |
| `Set from view` | Enregistre la vue actuelle dans cette caméra |
| `Field of view` | Champ de vision sur le petit côté de l'image. `Same as the viewport` : celui de la vue |

## Affichage et éclairage de la vue

Onglet `World`, ou menus `Lighting` et `Overlays` dans la visionneuse.

| Réglage | Effet |
|---|---|
| Matériau de la vue | `Clay`, `Gris`, `White`, `Chrome`, `Normals`, `Couleurs du fichier` (gris et couleurs du fichier n'ont pas encore de traduction anglaise) |
| `Smooth shading` | Ombrage lisse au lieu de facettes |
| `Wireframe` › `Opacity` | Fil de fer par-dessus. Couleur : `Black`, `White`, accent |
| `Wireframe only` | Seulement le fil de fer |
| `Cast shadows` | Chaque lumière projette l'ombre des objets. Plus lent sur les gros modèles |
| `Softness` | 0 : bord net. Plus haut : pénombre large, calculée avec plusieurs rayons |
| `Environment (HDRI)` | `Studio`, `Studio doux`, `Outdoor`, `Coucher de soleil`, `Nuit`, `Neutre` (quatre noms restent en français), ou les `.hdr` / `.exr` du coffre |
| `Load a .hdr / .exr…` | Ton propre environnement |
| `Rotation (H + drag)` | Tourne l'environnement |
| `Intensity`, `Exposure (EV)` | Force de l'environnement et exposition |
| `Show HDRI as background` | L'environnement en fond |
| `Directional light (L + drag)` | Lumière de la vue : `Intensity`, `Direction`, `Height`, `Color` |
| `Reset lighting` | Revient aux valeurs de départ |

Dans la vue : maintenir `L` et glisser déplace la lumière. Maintenir `H` et glisser tourne l'environnement.

Dès qu'une scène contient ses propres lumières, ce sont elles qui éclairent.

## Capturer

Menu `Capture` :

- `PNG image (2400 px)` ;
- `Animation as video (MP4)…` : chaque image de la scène, du début à la fin, vue par sa caméra (ou par la vue actuelle s'il n'y en a pas). Demande ffmpeg ;
- `Turntable GIF (48 frames)`.

Les fichiers sont enregistrés à côté du modèle. Pour une image finale de qualité, voir [Rendu](3d-rendu.md).

## Enregistrer et exporter

- `Ctrl+S` enregistre la scène.
- `Ctrl+Maj+S` ou `Export…` ouvre **Export model**.

| Format | Ce qui est gardé |
|---|---|
| OBJ | Quads et n-gones, UV (`vt`), matériaux par ID (`usemtl` + `.mtl`) |
| FBX binaire | Quads, UV, matériaux. Une scène garde ses os et l'animation. Lu par Maya, 3ds Max, Blender, Unity, Unreal |
| GLB (glTF binaire) | UV, un matériau par ID. Une scène garde ses os, ses poids et l'animation |
| STL binaire | La forme seule, pour l'impression 3D |
| PLY | La forme, pour les scans et MeshLab |

Les modificateurs actifs sont inclus dans le fichier exporté.

## Unités

Les fichiers sont en mètres. L'unité affichée (m, cm, mm, km, in, ft) se choisit dans les réglages.

## Limites

- **Taille des modèles.** Au-delà de 2 millions de triangles, un modèle est simplifié pour l'affichage (regroupement de sommets sur une grille). La forme reste complète, avec moins de triangles.
- **Affichage sur le processeur.** La vue est calculée par le processeur, sur tous les cœurs, et recalculée seulement quand quelque chose change. Voir [Moteur d'affichage 3D](../technique/moteur-3d.md).
- **Courbes et surfaces.** Pas de courbes de Bézier ni de NURBS en 3D.
- **FBX ASCII** : non lu. Seul le FBX binaire (versions 7.x) l'est.
