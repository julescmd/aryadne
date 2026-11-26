---
titre: Découvrir l'Atelier
niveau: débutant
duree: 10 min
prerequis:
objectifs: Reconnaître les quatre zones de l'Atelier ; Ajouter un préréglage comme calque ; Changer la forme et la lumière de l'aperçu ; Cacher et supprimer un calque
---

## Ce que tu vas réussir

Tu vas ouvrir un matériau neuf. Tu vas y poser un métal tout fait, le regarder sur un cube, puis nettoyer la pile.

```essayer
nouveau: arymat
nom: premier-materiau
```

```etape
Vérifie que le matériau est ouvert dans l'outil **Materials** de l'étape **3D**.
verifier: sous_onglet = materiaux
```

## Les quatre zones

| Zone | Ce que tu y trouves |
|---|---|
| À gauche | **Materials** : la bibliothèque |
| Au centre | L'aperçu du matériau sur une forme |
| À droite, en haut | La pile de calques |
| À droite, en bas | Les propriétés du calque choisi |

La pile a déjà un calque, nommé **Graph**. C'est le matériau de départ. Le calque du dessus est en haut de la liste, comme dans un logiciel de retouche.

## La bibliothèque

En haut de la bibliothèque, un champ de recherche. Dessous, quatre icônes filtrent la liste. Survole-les pour lire leur nom.

| Filtre | Ce qu'il montre |
|---|---|
| **All** | Tout |
| **Vault master materials** | Les matériaux de ton coffre |
| **Smart materials** | Les piles que tu as enregistrées |
| **Presets** | Les matériaux fournis |

```etape
Clique le filtre **Presets**.
```

```etape
Trouve la pastille **Scratched brushed metal** et clique-la. Un calque s'ajoute en haut de la pile.
verifier: atelier.calques >= 2
```

Un préréglage devient un calque à part entière, avec son propre graphe de nodes. Rien n'est écrit ailleurs dans le coffre.

```astuce
Le nom d'une pastille est coupé s'il est long. Survole-la pour le lire en entier.
```

```attention
La recherche de la bibliothèque lit le nom français des préréglages, même quand l'interface est en anglais. Pour ce métal, tape « métal ».
```

## L'aperçu

```etape
Au-dessus de l'aperçu, clique **Cube**. Le matériau s'affiche sur un cube aux arêtes adoucies.
```

Le cube sert à juger l'usure des arêtes et la saleté des creux. La sphère montre mieux les reflets.

```etape
Ouvre **Light** et choisis **Outdoor**. Regarde les reflets changer. Reviens sur **Studio**.
```

Tu peux aussi déposer un modèle 3D de ton coffre sur l'aperçu. Le matériau s'affiche alors dessus.

## Montrer, cacher, supprimer

```etape
Clique l'œil du calque de métal. Le calque **Graph** réapparaît dessous. Reclique l'œil.
```

Un calque opaque et posé partout cache tout ce qui est sous lui.

```etape
Clique le calque **Graph** pour le choisir. Dans l'en-tête de la pile, clique la corbeille : **Delete layer**.
```

Il reste un seul calque : ton métal.

```etape
Annule avec Ctrl+Z. Le calque revient. Rétablis avec Ctrl+Maj+Z.
```

## L'en-tête de la pile

Six icônes. Survole-les pour lire leur rôle.

| Icône | Rôle |
|---|---|
| **+** | Nouveau calque de remplissage |
| Étincelles | Enregistrer la pile comme matériau intelligent |
| Flèche vers le haut | **Move up** |
| Flèche vers le bas | **Move down** |
| Copie | **Duplicate layer** |
| Corbeille | **Delete layer** |

```quiz
Où se trouve le calque du dessus dans la pile ?
- [x] En haut de la liste.
- [ ] En bas de la liste.
- [ ] Dans la bibliothèque.
explication: Comme dans un logiciel de retouche : ce qui est en haut de la liste recouvre le reste.
```

```quiz
Que se passe-t-il quand tu cliques un préréglage ?
- [ ] Il remplace tout le matériau.
- [x] Il devient un nouveau calque, avec son propre graphe.
- [ ] Il crée un fichier dans le coffre.
explication: Le préréglage s'ajoute en haut de la pile. Le reste du matériau ne change pas.
```

```defi
Compose un sol : pose le préréglage **Embossed tiles**, puis regarde-le sur le cube en lumière Outdoor.
indice: Filtre Presets, puis tape « carrelage » dans la recherche.
```

## En trois lignes

- Un matériau est une pile de calques. Le dessus est en haut.
- La bibliothèque, à gauche, ajoute des calques d'un clic.
- **Sphere**, **Cube** et **Light** changent seulement l'aperçu.

[Suite : Les calques de remplissage](02-calques-de-remplissage.md)
