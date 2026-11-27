---
titre: Réutiliser, appliquer, cuire
niveau: intermédiaire
duree: 25 min
prerequis: 11-materiaux-atelier/03-les-masques, 07-3d-bases/01-ta-premiere-scene
objectifs: Enregistrer une pile comme matériau intelligent ; Appliquer un matériau à un objet de la scène ; Le voir se mettre à jour en direct ; Exporter ses textures pour un autre logiciel ; Donner un matériau du coffre à un ID
---

## Ce que tu vas réussir

Tu vas ranger une pile entière dans ta bibliothèque. Tu vas poser un matériau sur un objet. Puis tu vas en tirer des images de texture, nommées pour le logiciel de ton choix.

## Partie 1 : un matériau intelligent

Un **matériau intelligent** est une pile enregistrée : tous ses calques, avec leurs masques. Tu la reposes sur un autre matériau d'un clic.

```essayer
nouveau: arymat
nom: metal-use
```

```etape
Pose le préréglage **Scratched brushed metal**. Ajoute ensuite un calque de remplissage sombre, avec le masque **Cavity dirt**.
verifier: atelier.calques >= 3
```

```etape
Enregistre le matériau avec Ctrl+S.
verifier: fichier.enregistre
```

```etape
Dans l'en-tête de la pile, clique les étincelles. Un message confirme : « Smart material saved: ».
```

Le fichier porte le nom du matériau et l'extension `.arysmart`. Il va dans le dossier `matériaux intelligents` du coffre.

```etape
Dans la bibliothèque, clique le filtre **Smart materials**. Ta pile y figure, avec un badge d'étincelles.
```

```etape
Clique-la. Tous ses calques s'ajoutent d'un coup à la pile, avec leurs masques.
verifier: atelier.calques >= 5
```

Annule avec Ctrl+Z : c'était pour voir.

| Dans la bibliothèque | Au clic |
|---|---|
| Matériau maître du coffre | Un calque lié |
| Matériau intelligent | Tous ses calques, avec leurs masques |
| Préréglage | Un calque avec son propre graphe |

## Partie 2 : sur un objet

Un matériau ne sert qu'une fois posé sur un modèle.

```essayer
nouveau: aryscene
nom: essai-materiau
```

```etape
Clique le cube pour le choisir.
verifier: scene.selection
```

```etape
Dans le panneau **Properties**, ouvre l'onglet **Material**.
```

Deux chemins s'offrent à toi.

- Choisir un `.arymat` du coffre dans la liste : par exemple « metal-use ».
- Le petit bouton **＋** : au survol, **Create a node material for this object**. Il crée un matériau neuf, l'applique et l'ouvre.

```etape
Sur la ligne **Material**, ouvre la liste, qui affiche **None**. Choisis « metal-use ». Le cube le prend.
```

```etape
Clique le petit crayon à côté de la liste : **Open this material in the node editor**. Le matériau s'ouvre dans l'Atelier.
verifier: fichier.ouvert = arymat
```

```etape
Change la couleur d'un calque. Reviens à la scène : le cube a suivi.
```

Un matériau appliqué se met à jour en direct dans la vue 3D.

```astuce
Depuis la vue 3D, **Nodes**, puis **New material…** fait tout d'un geste : il crée le matériau, l'applique à l'objet choisi et l'ouvre à côté du modèle.
```

### Juger au tracé de chemins

```etape
Dans la barre de la vue, ouvre **Render** et choisis le moteur **Path tracing (realistic)**. Laisse l'image s'affiner.
```

L'aperçu rapide et le tracé de chemins lisent le même matériau. Le second montre les vrais reflets.

## Partie 3 : exporter les textures

Un `.arymat` ne se lit que dans Aryadne. Pour un moteur de jeu ou un autre logiciel, il faut des images. L'Atelier les écrit lui-même : il **cuit** la pile de calques sur un modèle.

```etape
Reviens au matériau « metal-use », dans l'Atelier.
verifier: fichier.ouvert = arymat
```

```etape
En haut à droite de l'aperçu, clique le bouton à l'image fléchée. Au survol : **Export textures**. Une fenêtre s'ouvre.
```

La fenêtre tient en quatre cartes.

| Carte | Ce que tu y choisis |
|---|---|
| **Model to bake** | Le modèle qui reçoit le matériau |
| **Images** | La taille et le format des images |
| **Naming** | Le logiciel visé, et le nom des fichiers |
| **Output folder** | Le dossier d'arrivée |

### Sur quoi cuire

```etape
Ouvre la liste **Bake on**. Le cube de ta scène est en tête, avec un maillon : il porte ce matériau. Laisse-le choisi.
```

| Dans la liste | Ce qui est cuit |
|---|---|
| Un objet d'une scène | L'objet, à sa place dans la scène. Ceux qui portent le matériau viennent d'abord |
| **Flat tile** | Un carré à plat : une texture à répéter sur n'importe quel modèle |
| **Preview sphere**, **Preview cube** | La forme montrée dans l'aperçu |
| **Preview model** | Le modèle 3D que tu as déposé dans l'aperçu |

Un objet déplié dans l'onglet **UV** est cuit sur ses UV. Un objet sans UV en reçoit d'automatiques : tu n'as rien à déplier pour essayer. Si ses UV occupent plusieurs tuiles UDIM, chaque tuile a ses images, numérotées 1001, 1002…

```astuce
**Model occlusion** ajoute l'ombre que le modèle se fait à lui-même, dans ses creux. C'est plus long. Sur un carreau plat, laisse la case vide.
```

### Taille, format, nommage

```etape
Dans **Images**, choisis **1K** et **PNG 8-bit**.
```

| Format | Pour quoi |
|---|---|
| **PNG 8-bit** | Le plus léger, lu partout |
| **PNG 16-bit** | La hauteur et la normale, sans marches visibles |
| **EXR 32-bit** | Le compositing et le film. Couleur en linéaire, fichiers lourds |

```etape
Dans **Naming**, clique **glTF**. La liste des fichiers change sous le champ **Name**.
```

Chaque logiciel attend ses noms et ses canaux. Les préréglages s'en chargent.

| Préréglage | Ce qu'il écrit |
|---|---|
| **Aryadne** | Une image par carte : `_couleur`, `_rugosite`, `_metal`, `_normale` |
| **Unreal** | `T_nom_BaseColor`, `T_nom_Normal`, `T_nom_ORM` |
| **Unity** | `nom_Albedo`, `nom_Normal`, `nom_MetallicSmoothness` |
| **Blender** | `nom_Base_Color`, `nom_Roughness`, `nom_Metallic`, `nom_Normal` |
| **glTF** | `nom_baseColor`, `nom_normal`, `nom_occlusionRoughnessMetallic` |

Une image **ORM** range trois cartes grises dans une seule : l'occlusion dans le rouge, la rugosité dans le vert, le métal dans le bleu. Le moteur charge une image au lieu de trois.

La hauteur, l'occlusion et l'émission ne s'écrivent que si le matériau les donne. La normale est tirée du relief du matériau. Pour Unreal, son vert est inversé : c'est ce que ce moteur attend.

### Écrire les images

Le dossier proposé s'appelle `textures`. Il est à côté du matériau. Le bouton au dossier en choisit un autre.

```etape
Clique **Export**. Une barre avance, puis un message confirme : « 3 images written to ».
```

```etape
Clique le petit dossier à gauche du message : le dossier s'ouvre. Tes images y sont.
```

```attention
Les masques **Hand-painted** ne sont pas cuits par cet export : le calque qui en dépend n'apparaît pas dans les images.
```

## Partie 4 : plusieurs matériaux sur un objet

Un objet peut porter plusieurs matériaux, un par **ID de matériau**. Un ID est une étiquette posée sur des faces. Cette cuisson-là part de l'objet, et elle lit le matériau de chaque ID.

```etape
Dans la scène, choisis le cube. Dans la barre de la vue, passe en mode **UV & materials**.
```

```etape
Ouvre l'onglet **Materials**. Sur la ligne de l'ID 0, clique le bouton au dossier ouvert. Au survol : **Pick a material from the vault**. Choisis « metal-use ».
```

Le nom du matériau s'affiche sous la ligne, avec un maillon. Le cube le montre.

| Bouton de la ligne | Effet |
|---|---|
| **＋ Nodes** | Crée un matériau neuf pour cet ID et l'ouvre |
| Le dossier ouvert | Donne à l'ID un `.arymat` qui existe déjà dans le coffre |
| **Nodes ✎** | Ouvre le matériau de l'ID |
| ⌫ | Retire le matériau de l'ID |

```etape
Ouvre l'onglet **UV**. Clique **Auto (seams + UVs)**. Le cube est déplié.
```

```etape
Choisis la taille des images, puis clique **Bake textures**.
```

Les images vont dans le dossier `textures` du coffre : couleur, rugosité, métal, normale et courbure, pour tous les IDs à la fois.

| Réglage | Effet |
|---|---|
| Occlusion | **No occlusion**, ou un nombre de rayons, de 8 à 128 |
| Taille | 512, 1024, 2048 ou 4096 pixels |
| **High-poly** | Le chemin d'un sculpt, pour reporter ses détails dans la normale |
| **Re-bake when the model changes** | Recuit tout seul quand la forme, les UV ou le matériau changent |

```etape
Reviens à la scène avec **Back to the scene**, ou Échap.
```

Le dépliage et cette cuisson ont leur parcours : [UV et textures](../10-uv-textures/index.md).

| Tu veux | Passe par |
|---|---|
| Les textures d'un matériau, nommées pour un moteur | **Export textures**, dans l'Atelier |
| Les textures d'un objet à plusieurs IDs, avec courbure et épaisseur | **Bake textures**, dans l'onglet **UV** |

```quiz
Que contient un matériau intelligent ?
- [ ] Un seul calque, lié à un maître.
- [x] Une pile entière : ses calques et leurs masques.
- [ ] Des images de texture.
explication: C'est une pile enregistrée. Un clic dans la bibliothèque repose tous ses calques.
```

```quiz
Comment obtenir des images de texture à partir d'un matériau ?
- [ ] Avec Ctrl+S dans l'Atelier.
- [x] Avec le bouton Export textures, en haut de l'aperçu de l'Atelier.
- [ ] En enregistrant un matériau intelligent.
explication: L'export cuit la pile sur le modèle choisi et écrit les images dans le dossier textures, à côté du matériau.
```

```quiz
Que contient une image ORM ?
- [ ] La couleur, en trois canaux.
- [x] L'occlusion, la rugosité et le métal, un par canal.
- [ ] La normale, pour Unreal.
explication: Rouge : occlusion. Vert : rugosité. Bleu : métal. Les préréglages Unreal et glTF l'écrivent.
```

```defi
Pose deux matériaux différents sur un même cube : le dessus et le reste.
indice: En mode UV & materials, onglet Materials : ajoute un ID avec le bouton Material, choisis les faces du dessus, clique Assign sur ce nouvel ID, puis donne-lui un matériau du coffre avec le bouton au dossier ouvert.
```

## En trois lignes

- Les étincelles enregistrent la pile en matériau intelligent, à reposer d'un clic.
- L'onglet **Material** d'un objet lui donne un `.arymat`. Il se met à jour en direct.
- **Export textures**, en haut de l'aperçu de l'Atelier, écrit les images pour Unreal, Unity, Blender ou glTF. Pour un objet à plusieurs IDs, chaque ID reçoit un `.arymat` du coffre, et **Bake textures** les cuit tous.

[Suite : Projet, un métal peint, usé et sale](08-projet-metal-peint.md)
