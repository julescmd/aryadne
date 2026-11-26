# Modélisation

Deux outils se complètent :

- le **mode édition** change le maillage lui-même, sommet par sommet ;
- la **pile de modificateurs** transforme le maillage sans le toucher, de façon réversible.

## Mode édition

### Entrer et sortir

1. Dans une scène, choisis un objet à maillage.
2. `Tab`, ou `Edit mode` dans la barre.
3. `Tab` de nouveau pour sortir.

Seuls les objets à maillage se modifient. Lumières, caméras et groupes vides n'ont pas de mode édition.

**Ce qui arrive au fichier.** En entrant en mode édition, le maillage devient un fichier OBJ à côté de la scène. Le modèle d'origine n'est pas touché. Les quads et les n-gones sont gardés tels quels.

### Sommets, arêtes, faces

`1`, `2`, `3` choisissent le mode de sélection : `Vertices (1)`, `Edges (2)`, `Faces (3)`.

### Sélectionner

| Geste | Effet |
|---|---|
| Clic | Choisir |
| `Maj+clic` | Ajouter |
| Glisser | Rectangle. `Maj` ajoute, `Ctrl` retire |
| `Ctrl+clic` | Chemin le plus court depuis la dernière sélection |
| `Alt+clic` | Boucle d'arêtes |
| `A` | Tout choisir |
| `Alt+A` | Ne rien choisir |
| `Ctrl+I` | Inverser |
| `Ctrl +` | Agrandir la sélection |
| `Ctrl −` | Rétrécir la sélection |
| `L` | La pièce reliée sous le pointeur |
| `Ctrl+L` | Tout ce qui touche la sélection |
| `B` | Sélection au rectangle |

### Déplacer, tourner, redimensionner

`G`, `R`, `S`, puis `X`, `Y` ou `Z` pour un axe. Clic valide, `Échap` annule.

`G` puis `G` : **Slide**. Les sommets glissent le long de leurs arêtes.

Pendant un déplacement, `Ctrl` aimante par crans.

### Aides de la barre

| Bouton | Raccourci | Effet |
|---|---|---|
| `X-ray` | `Alt+Z` | Voir et choisir aussi les sommets cachés derrière le maillage |
| `Proportional editing` | `O` | Les sommets voisins suivent. La molette pendant `G`, `R`, `S` règle le rayon |
| `Snap to vertices` | `Maj+Tab` | Pendant `G`, le sommet choisi le plus proche se pose sur le sommet visé |
| `X mirror` | | En déplaçant un sommet, son reflet de l'autre côté de l'objet suit |
| `Pivot point` | `.` | `Median point`, `3D cursor`, `Individual origins`, `Active element` |

### Les outils

Clic droit dans la vue ouvre le menu du mode édition. Il est rangé par famille.

**Vertices**

| Outil | Raccourci | Effet |
|---|---|---|
| `Merge at center` | `M` | Fusionne les sommets choisis en un seul, au centre |
| `Merge by distance` | | Fusionne les sommets plus proches qu'une distance. Réglage : `Distance` |
| `Vertex bevel` | `Ctrl+Maj+B` | Remplace un sommet par une petite face. Réglage : `Width` |
| `Smooth vertices` | | Adoucit. Réglages : `Factor`, `Repeat` |

**Edges**

| Outil | Raccourci | Effet |
|---|---|---|
| `Bevel` | `Ctrl+B` | Biseaute les arêtes choisies. Réglages : `Width`, `Segments` (molette) |
| `Loop cut` | `Ctrl+R` | Ajoute une boucle d'arêtes |
| `Bridge edge loops` | | Relie deux boucles par des faces |

**Faces**

| Outil | Raccourci | Effet |
|---|---|---|
| `Extrude` | `E` | Sort les faces choisies. Il faut au moins une face dont tous les sommets sont choisis |
| `Inset` | `I` | Incruste une face plus petite dans chaque face. Réglage : `Thickness` |
| `Subdivide` | | Coupe chaque face. Réglage : `Cuts` |
| `Fill` | `F` | Crée une face entre les sommets choisis |
| `Grid fill` | | Remplit une boucle de bord par une grille de quads. Il faut une seule boucle, avec un nombre pair de sommets. Réglages : `Span`, `Offset` |

**Mesh**

| Outil | Raccourci | Effet |
|---|---|---|
| `Knife` | `K` | Coupe à main levée |
| `Rip` | `V` | Déchire le maillage aux sommets choisis |
| `Split` | `Y` | Détache les faces choisies du reste, sans les déplacer |
| `Separate selection` | `P` | Les faces choisies deviennent un nouvel objet de la scène |
| `Delete or dissolve…` | `X` | Ouvre le menu de suppression |

**Delete** : `Delete vertices` (`Suppr`), `Delete edges`, `Delete faces`.

**Dissolve** : `Dissolve vertices`, `Dissolve edges`, `Dissolve faces`. Dissoudre retire l'élément sans laisser de trou. Pour les faces, choisis au moins deux faces voisines.

### Outils à réglage en direct

`Inset`, `Bevel`, `Vertex bevel`, `Subdivide`, `Smooth vertices`, `Merge by distance` et `Grid fill` s'utilisent ainsi :

1. Lance l'outil.
2. Bouge la souris : la valeur change. Elle s'affiche près du curseur.
3. Molette : le second réglage (segments, coupes).
4. `Ctrl` : arrondir la valeur.
5. Clic valide. Clic droit remet la valeur par défaut. `Échap` annule.

Pour `Inset`, `I` pendant l'outil bascule entre `Whole region` et `Individual faces`.

### Coupe en boucle, pas à pas

1. `Ctrl+R`.
2. Survole une arête : l'aperçu de la boucle s'affiche.
3. Molette, ou `1` à `9` : nombre de coupes.
4. Clic : la coupe est faite, puis elle glisse avec la souris.
5. Clic pour poser. Clic droit : au milieu.

### Couteau, pas à pas

1. `K`.
2. Clic sur un sommet ou une arête pour poser un point.
3. Continue. Chaque segment doit traverser une face, d'un bord à l'autre.
4. `Retour arrière` enlève le dernier point.
5. `Entrée` ou clic droit : couper. `Échap` : annuler.

### Annuler

`Ctrl+Z` annule, `Ctrl+Maj+Z` ou `Ctrl+Y` rétablit. L'historique est propre au mode édition.

### Limites

- Pas de sélection au lasso ni au cercle.
- L'aimantation ne se fait que sur les sommets (pas sur les arêtes, la grille ou les surfaces).
- Le miroir d'édition existe sur X seulement.
- Le couteau ne coupe qu'à travers des faces entières, d'un bord à l'autre.

## Modificateurs

Un modificateur transforme le maillage du précédent, sans toucher au fichier d'origine. Ils s'appliquent de haut en bas.

### Ouvrir la pile

Bouton `Modifiers` de la barre, ou onglet `Modifiers` des propriétés.

### Gérer la pile

| Geste | Effet |
|---|---|
| `＋ Add ▾` | Ajoute un modificateur. Un champ de recherche filtre la liste |
| Glisser la poignée | Changer l'ordre |
| Œil | Activer ou désactiver |
| `Duplicate`, `Delete` | Sur chaque modificateur |
| `Apply` | Le résultat devient le maillage de base, avec les modificateurs au-dessus |

Options de la pile : `Expand all`, `Collapse all`, `Enable all`, `Disable all`, `Clear the stack`, `Apply all` (le résultat devient le maillage de base et la pile se vide), `Restore the file's mesh` (oublie ce qui a été appliqué).

### Influence et zone

Chaque déformation a un bloc **Influence and area** :

| Réglage | Effet |
|---|---|
| `Influence` | Dose l'effet, de 0 à 1 |
| `Limit to a zone` | N'agit que sur une partie du modèle, le long d'un axe |
| `Start`, `End` | Bornes de la zone |
| `Falloff` | Largeur du fondu aux bornes |
| `Invert the zone` | Agit partout sauf dans la zone |

### La liste

**Generate (générer)**

| Modificateur | Effet | Réglages principaux |
|---|---|---|
| `Mirror` | Symétrie sur un ou plusieurs axes | Axes, `Weld in the middle`, `Cut first (bisect)` |
| `Array` | Copies avec décalage, rotation et échelle par copie | `Copies`, `Offset`, `Rotation °`, `Scale` |
| `Subdivision` | Lisse et densifie | `Levels`, `Catmull-Clark` ou `Simple`, `Optimal display` |
| `Solidify` | Donne une épaisseur | `Thickness`, `Offset`, `Close the borders` |
| `Wireframe` | Change les arêtes en tubes | `Thickness`, `Keep the mesh` |
| `Screw` | Copies en spirale autour d'un axe | Axe, `Steps`, `Height`, angle |
| `Boolean (shape)` | Ajoute, creuse ou garde l'intersection avec une forme | `Operation` (`Union`, `Difference`, `Intersection`), `Shape` (`Cube`, `Sphere`, `Cylinder`), `Center`, `Half size`, `Voxel size`, `Smoothing` |
| `Scatter (instances)` | Sème des copies d'une petite forme sur la surface | Forme, quantité, taille, `Seed` |

**Deform (déformer)**

| Modificateur | Effet | Réglages principaux |
|---|---|---|
| `Smooth` | Adoucit les formes | `Passes`, `Strength`, `Preserve volume (Taubin)` |
| `Twist` | Tord autour d'un axe | Axe, angle |
| `Bend` | Plie le long d'un axe | Axe, angle |
| `Taper` | Rétrécit vers un bout | Axe, `Factor` |
| `Cast` | Tend vers une sphère, un cylindre ou un cube | Forme, `Factor` |
| `Stretch` | Allonge en gardant le volume | Axe, `Factor` |
| `Shear` | Penche le long d'un axe | `Factor`, `Moves`, `Along` |
| `Warp` | Tire une zone d'un point à un autre | De, `To`, `Radius` |
| `Bulge` | Gonfle autour d'un point | `Strength`, `Center`, `Radius` |
| `Inflate` | Pousse le long des normales | `Distance` |
| `Axial bulge (lattice)` | Renfle ou pince le milieu | Axe, `Factor` |

**Organic (organique)**

| Modificateur | Effet | Réglages principaux |
|---|---|---|
| `Displace (noise)` | Relief procédural fractal | `Strength`, `Scale`, `Octaves`, `Roughness`, `Seed`, `Direction` (`Normal` ou un axe) |
| `Wave` | Vagues linéaires ou circulaires | `Amplitude`, longueur d'onde, `Circular` |
| `Ripple` | Ronds dans l'eau autour du centre | `Amplitude`, `Damping` |
| `Random` | Secoue les sommets | `Amount`, `Seed`, `Along normals` |
| `Melt` | Affaisse vers le sol | `Strength` |
| `Crumple` | Plis aléatoires | `Strength`, `Scale`, `Seed` |

**Topology (topologie)**

| Modificateur | Effet | Réglages principaux |
|---|---|---|
| `Decimate` | Réduit le nombre de triangles | Taux |
| `Weld` | Fusionne les sommets proches | `Distance` |
| `Triangulate` | Montre tous les triangles | Aucun |
| `Remesh (voxels)` | Refait la surface en quads réguliers, pour sculpter | `Voxel size`, `Smoothing`. Plus petit : plus de détails et plus de faces |

**Normals (normales)**

| Modificateur | Effet |
|---|---|
| `Edge split` | Facettes nettes au-delà d'un angle |
| `Flip normals` | Retourne toutes les faces |

**Effects (effets)**

| Modificateur | Effet | Réglages principaux |
|---|---|---|
| `Explode` | Écarte les faces le long de leurs normales | `Distance` |
| `Voxelize` | Aligne les sommets sur une grille (effet pixel) | `Scale` |
| `Build` | Ne garde qu'une partie des faces | Part gardée, `Random order`, `Seed` |

**Editing (édition)**

| Modificateur | Effet | Réglages principaux |
|---|---|---|
| `Transform` | Échelle, rotation, décalage | `Scale`, `Rotation °`, `Offset` |
| `Slice` | Supprime ce qui dépasse d'un plan | Axe, `Location`, `Keep the other side` |

### Subdivision

- `Catmull-Clark` : comme dans Blender, calculée sur les vrais quads et n-gones.
- `Simple` : densifie sans lisser.
- `Optimal display` : le fil de fer ne montre que les arêtes de la cage d'origine.
- La subdivision est limitée : au-delà d'environ 2,5 millions de triangles affichés, le nombre de niveaux est plafonné et un message l'indique.

### Limites

- Le **booléen** agit avec une forme simple (cube, sphère, cylindre), pas avec un autre objet de la scène. Il passe par des voxels : le résultat est un maillage régulier, pas une coupe nette.
- Pas de modificateur de déformation par courbe, ni d'armature dans la pile (le squelette se règle dans l'onglet `Object`).
- Le **décimer** et le **remaillage** changent le nombre de sommets : les UV posées avant sont perdues.

## Exporter le résultat

Dans la fenêtre des modificateurs, **Export the result** écrit le maillage modifié. La fenêtre d'export habituelle inclut aussi les modificateurs actifs. Voir [Scène 3D](3d-scene.md#enregistrer-et-exporter).
