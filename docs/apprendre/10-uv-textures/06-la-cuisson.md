---
titre: La cuisson
niveau: avancé
duree: 15 min
prerequis: 10-uv-textures/03-rangement-densite-udim, 11-materiaux-atelier/02-calques-de-remplissage
objectifs: Donner un matériau à un ID, neuf ou déjà dans le coffre ; Cuire les cinq textures ; Régler taille et occlusion ; Reporter les détails d'un sculpt ; Recuire automatiquement
---

## Ce que tu vas réussir

Tu vas transformer un matériau d'Aryadne en images que n'importe quel logiciel sait lire. Puis tu sauras reporter les détails d'un sculpt sur un modèle léger.

```essayer
nouveau: aryscene
nom: cuisson
```

## Ce que la cuisson écrit

**Bake textures** écrit cinq images dans le dossier `textures` du coffre.

| Image | Contenu |
|---|---|
| Couleur | La couleur de base |
| Rugosité | Lisse ou mat |
| Métal | Métallique ou non |
| Normale | Le relief fin, pour l'éclairage |
| Courbure | Arêtes et creux du modèle |

Le calcul part de deux choses : les **UV** du modèle, et le **matériau de chaque ID**.

## Les IDs de matériau

Un ID de matériau est une étiquette posée sur des faces. Chaque ID peut recevoir son propre matériau. Un objet neuf a un seul ID, qui couvre tout.

```etape
Choisis le cube. Passe en mode **UV & materials** et ouvre l'onglet **Materials**.
verifier: scene.selection
```

Chaque ID a une couleur, un nom et quelques boutons.

| Bouton | Effet |
|---|---|
| **Assign** | Donne cet ID aux faces choisies |
| **Choose** | Sélectionne les faces de cet ID |
| **＋ Nodes** | Crée un matériau pour cet ID et l'ouvre |
| Le dossier ouvert | Donne à cet ID un `.arymat` qui existe déjà dans le coffre |
| **Nodes ✎** | Ouvre le matériau de cet ID |
| ⌫ | Retire le matériau de cet ID |

### Deux IDs sur un cube

```etape
Clique le bouton **Material**, sous la liste. Un second ID est créé.
```

```etape
Dans la vue, clique la face du dessus du cube. Sur le second ID, clique **Assign**. La face prend sa couleur.
```

| Geste | Effet |
|---|---|
| Clic | Choisir une face |
| Maj+clic | Ajouter |
| Ctrl+clic | Retirer |
| **Grow** | Ajouter les faces voisines d'orientation proche |
| **Island** | Les faces reliées, jusqu'aux coutures |

### Un matériau par ID

```etape
Sur le premier ID, clique **＋ Nodes**. Un matériau est créé et s'ouvre dans l'Atelier.
verifier: fichier.ouvert = arymat
```

```etape
Pose le préréglage **Scratched brushed metal** depuis la bibliothèque. Reviens à la scène.
verifier: atelier.calques >= 2
```

```etape
Sur le second ID, clique **＋ Nodes**. Dans ce matériau, clique **+** et donne au calque un rouge vif. Reviens à la scène.
verifier: sous_onglet = scene
```

Ton cube est en métal, avec un couvercle rouge.

### Un matériau déjà fait

Le matériau existe déjà dans le coffre ? Inutile d'en créer un autre.

```etape
Sur la ligne d'un ID, clique le bouton au dossier ouvert. Au survol : **Pick a material from the vault**. La liste montre les `.arymat` du coffre.
```

Un clic sur un nom, et l'ID le porte. Son nom s'affiche sous la ligne, avec un maillon. La cuisson le lit comme les autres, qu'il soit fait de calques ou de nodes. Le même matériau peut servir à plusieurs IDs et à plusieurs objets.

## Déplie

```etape
Ouvre l'onglet **UV**. Clique **Auto (seams + UVs)**.
```

Le bouton **Bake textures** n'est actif qu'une fois le modèle déplié.

## Cuis

À côté du bouton, deux listes.

| Réglage | Choix |
|---|---|
| Occlusion | **No occlusion**, ou 8, 24, 64, 128 rayons |
| Taille | 512, 1024, 2048 ou 4096 pixels |

```etape
Choisis 1024 pixels et **No occlusion**. Clique **Bake textures**.
```

Un message donne le nombre de textures écrites, leur taille et le temps passé. Les images sont dans le dossier `textures` du coffre.

```etape
Ouvre une des images depuis le panneau des fichiers. Tu reconnais le patron UV, rempli par le matériau.
```

### L'occlusion

L'occlusion assombrit les creux et les endroits où deux surfaces se rapprochent. Plus il y a de rayons, plus elle est lisse, et plus le calcul est long. 8 rayons suffisent pour un brouillon.

## Reporter un sculpt

Le champ **High-poly** reçoit le chemin d'un sculpt. Ses détails sont reportés dans la carte de normales du modèle léger.

Deux conditions :

- le modèle léger est déplié ;
- le sculpt occupe la même place que lui dans l'espace.

C'est la dernière étape du trajet d'un sculpt : sculpter, retopologiser, déplier, cuire.

```astuce
Après une retopologie appliquée avec **Apply**, le champ **High-poly** est déjà rempli : il pointe l'ancien maillage. Ce que tu y écris est gardé avec le modèle.
```

## Recuire tout seul

```etape
Coche **Re-bake when the model changes**.
```

Les textures sont alors recuites en arrière-plan dès que la forme, les UV ou les matériaux changent. Tu retouches un calque dans l'Atelier, et les images suivent.

## Avec des UDIM

Si tu as réparti les îlots sur plusieurs tuiles, chaque tuile reçoit ses propres images.

## Des images nommées pour un moteur

**Bake textures** donne des noms simples, et une image par carte. Pour Unreal, Unity, Blender ou glTF, l'Atelier a son propre export : **Export textures**, en haut de son aperçu. Il nomme les fichiers comme le logiciel les attend, range occlusion, rugosité et métal dans une seule image, et écrit en PNG 16 bits ou en EXR. Voir [Réutiliser, appliquer, cuire](../11-materiaux-atelier/07-reutiliser-appliquer-cuire.md).

| Tu veux | Passe par |
|---|---|
| Tous les IDs d'un objet, avec courbure, occlusion et épaisseur | **Bake textures**, ici |
| Un matériau, nommé pour un moteur | **Export textures**, dans l'Atelier |

## Exporter le modèle

Les images sont prêtes. Le modèle s'exporte avec Ctrl+Maj+S, ou **Export…**.

| Format | Ce qui est gardé |
|---|---|
| OBJ | Quads, UV, un matériau par ID |
| FBX binaire | Quads, UV, matériaux, os, animation |
| GLB | UV, un matériau par ID, os, animation |

```etape
Reviens à la scène et enregistre.
verifier: fichier.enregistre
```

```quiz
De quoi part la cuisson ?
- [x] Des UV du modèle et du matériau de chaque ID.
- [ ] De la peinture seulement.
- [ ] De l'image rendue.
explication: Chaque ID de matériau a son matériau. La cuisson le calcule sur le patron UV.
```

```quiz
Le bouton Bake textures est grisé. Pourquoi ?
- [ ] Il manque une lumière.
- [x] Le modèle n'a pas d'UV.
- [ ] Il manque un sculpt.
explication: Sans UV, les images n'ont pas de patron. Déplie d'abord.
```

```defi
Cuis le même cube en 512 puis en 4096 pixels, avec 64 rayons d'occlusion. Compare les images et le temps.
indice: Change seulement la taille et l'occlusion. Regarde les bords des îlots et le grain de l'occlusion.
```

## En trois lignes

- Chaque ID de matériau porte son matériau : un neuf avec **＋ Nodes**, ou un `.arymat` du coffre avec le bouton au dossier ouvert.
- **Bake textures** écrit couleur, rugosité, métal, normale et courbure.
- **High-poly** reporte un sculpt. **Re-bake when the model changes** tient les images à jour.

Parcours terminé. Pour fabriquer les matériaux que tu cuis, va voir [Matériaux : l'Atelier](../11-materiaux-atelier/index.md).
