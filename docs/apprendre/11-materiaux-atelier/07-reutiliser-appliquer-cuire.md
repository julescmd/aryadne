---
titre: Réutiliser, appliquer, cuire
niveau: intermédiaire
duree: 20 min
prerequis: 11-materiaux-atelier/03-les-masques, 07-3d-bases/01-ta-premiere-scene
objectifs: Enregistrer une pile comme matériau intelligent ; Appliquer un matériau à un objet de la scène ; Le voir se mettre à jour en direct ; Cuire ses textures pour l'exporter
---

## Ce que tu vas réussir

Tu vas ranger une pile entière dans ta bibliothèque. Tu vas poser un matériau sur un objet. Puis tu vas en tirer des images de texture, lisibles par d'autres logiciels.

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

## Partie 3 : cuire les textures

Un `.arymat` ne se lit que dans Aryadne. Pour un moteur de jeu ou un autre logiciel, il faut des images : c'est la **cuisson**.

Dans le code actuel, la cuisson ne part pas de l'Atelier. Elle part de l'objet, et elle lit le matériau de chaque **ID de matériau**. Un ID est une étiquette posée sur des faces. Un objet neuf n'en a qu'un : l'ID 0, qui couvre tout.

```etape
Dans la scène, choisis le cube. Dans la barre de la vue, passe en mode **UV & materials**.
```

```etape
Ouvre l'onglet **Materials**. Sur la ligne de l'ID 0, clique **＋ Nodes**. Un matériau neuf est créé pour cet ID, et il s'ouvre dans l'Atelier.
verifier: fichier.ouvert = arymat
```

C'est ici que ton matériau intelligent rend service.

```etape
Dans la bibliothèque, filtre **Smart materials** et clique « metal-use ». Ta pile est reposée dans le matériau de l'ID.
verifier: atelier.calques >= 3
```

```etape
Reviens à la scène. Ouvre l'onglet **UV**. Clique **Auto (seams + UVs)**. Le cube est déplié.
```

```etape
Choisis la taille des images, puis clique **Bake textures**.
```

Cinq images sont écrites dans le dossier `textures` du coffre : couleur, rugosité, métal, normale et courbure.

| Réglage | Effet |
|---|---|
| Occlusion | **No occlusion**, ou un nombre de rayons, de 8 à 128 |
| Taille | 512, 1024, 2048 ou 4096 pixels |
| **High-poly** | Le chemin d'un sculpt, pour reporter ses détails dans la normale |
| **Re-bake when the model changes** | Recuit tout seul quand la forme, les UV ou le matériau changent |

```etape
Reviens à la scène avec **Back to the scene**, ou Échap.
```

Le dépliage et la cuisson ont leur parcours : [UV et textures](../10-uv-textures/index.md).

```attention
Sans UV, pas de cuisson : les images n'ont nulle part où se poser. Déplie toujours avant.
```

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
- [x] En cuisant depuis l'onglet UV de l'objet, avec Bake textures.
- [ ] En enregistrant un matériau intelligent.
explication: La cuisson part des UV de l'objet et du matériau de chaque ID. Elle écrit les images dans le dossier textures.
```

```defi
Pose deux matériaux différents sur un même cube : le dessus et le reste.
indice: En mode UV & materials, onglet Materials : ajoute un ID avec le bouton Material, choisis les faces du dessus, clique Assign sur ce nouvel ID, puis ＋ Nodes.
```

## En trois lignes

- Les étincelles enregistrent la pile en matériau intelligent, à reposer d'un clic.
- L'onglet **Material** d'un objet lui donne un `.arymat`. Il se met à jour en direct.
- **Bake textures**, dans l'onglet **UV**, écrit les images pour l'export.

[Suite : Projet, un métal peint, usé et sale](08-projet-metal-peint.md)
