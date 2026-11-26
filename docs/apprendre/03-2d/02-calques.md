---
titre: Les calques
niveau: débutant
duree: 15 min
prerequis: 03-2d/01-premiere-image
objectifs: Créer, nommer et ordonner des calques ; Régler l'opacité et le mode de fusion ; Verrouiller un calque ; Grouper et fusionner
---

## Ce que tu vas réussir à faire

Tu vas séparer un dessin en trois calques : le trait, la couleur, l'ombre. Tu pourras changer l'un sans toucher aux autres.

Un calque est une feuille transparente posée sur l'image. Ce que tu peins sur un calque ne touche pas ceux du dessous.

```essayer
nouveau: aryimg
nom: trois-calques
```

```etape
Clique **Essaie par toi-même**.
verifier: fichier.ouvert = aryimg
```

## Créer un calque

```etape
Appuie sur `Ctrl+Maj+N`. Un nouveau calque apparaît dans le panneau **Layers**, au-dessus du premier.
verifier: calques >= 2
```

Le même geste est dans le menu **Layer ▾ › New layer**, et dans les boutons en bas du panneau **Layers**.

Le calque actif est celui qui est surligné. Tu peins toujours sur le calque actif.

```etape
Sur ce calque, dessine un rond au pinceau noir : ce sera ton trait.
```

```etape
Double-clique le nom du calque. Nomme-le `trait`.
```

## Un calque de couleur, sous le trait

```etape
Crée un autre calque avec `Ctrl+Maj+N`. Nomme-le `couleur`.
verifier: calques >= 3
```

```etape
Dans le panneau **Layers**, glisse le calque `couleur` sous le calque `trait`.
```

Le panneau se lit du haut vers le bas. Le calque du haut cache ceux du dessous.

```etape
Sur le calque `couleur`, peins l'intérieur du rond d'une couleur vive. Tu peux déborder : le trait reste par-dessus.
```

## L'œil, l'opacité, le fond

Chaque calque a ses réglages.

| Réglage | Effet |
|---|---|
| L'œil | Montre ou cache le calque |
| **Opacity** | La transparence de tout le calque |
| **Fill** | La transparence du contenu seul, sans ses effets |
| Le mode de fusion | Comment le calque se mélange avec le dessous |
| **Lock** | Deux verrous : les pixels transparents, ou tout le calque |

```etape
Clique l'œil du calque `couleur` pour le cacher. Clique encore pour le montrer.
```

```etape
Baisse **Opacity** du calque `couleur` à 60 % environ.
```

## Verrouiller la transparence

**Lock transparent pixels** protège les zones vides du calque. Tu ne peux plus peindre que là où il y a déjà de la peinture.

```etape
Active **Lock transparent pixels** sur le calque `couleur`. Peins avec une autre couleur en travers du rond : la peinture ne sort pas de la forme.
```

C'est le moyen le plus simple de recolorer ou d'ombrer une forme sans déborder.

## Les modes de fusion

Le mode de fusion change le mélange entre un calque et ce qui est dessous.

| Groupe | Modes | Usage courant |
|---|---|---|
| Assombrir | **Darken**, **Multiply**, **Color Burn**, **Linear Burn** | Les ombres |
| Éclaircir | **Lighten**, **Screen**, **Color Dodge**, **Linear Dodge (Add)** | Les lumières |
| Contraste | **Overlay**, **Soft Light**, **Hard Light** et autres | Renforcer une image |
| Comparaison | **Difference**, **Exclusion**, **Subtract**, **Divide** | Comparer deux images |
| Couleur | **Hue**, **Saturation**, **Color**, **Luminosity** | Recolorer |

```etape
Crée un calque `ombre` entre `couleur` et `trait`. Passe son mode de fusion de **Normal** à **Multiply**.
verifier: calques >= 4
```

```etape
Sur le calque `ombre`, peins un côté du rond avec un gris moyen. La couleur du dessous s'assombrit sans disparaître.
```

```astuce
**Multiply** assombrit toujours. **Screen** éclaircit toujours. Avec ces deux modes, tu fais la plupart des ombres et des lumières.
```

## Dupliquer, fusionner, grouper

| Action | Raccourci |
|---|---|
| **Duplicate layer** | `Ctrl+J` |
| **Merge down** : fusionner avec le calque du dessous | `Ctrl+E` |
| **Group layer** : mettre le calque dans un groupe | `Ctrl+G` |

```etape
Choisis le calque `ombre`. Groupe-le avec `Ctrl+G`.
```

Un groupe est un dossier de calques. Il a sa propre opacité, son mode de fusion et son masque. Son mode par défaut est **Pass Through** : les calques du groupe se mélangent directement avec le dessous.

```attention
**Merge down** est définitif une fois le fichier fermé. Garde tes calques séparés tant que tu travailles. Fusionne seulement quand tu es sûr.
```

## Un calque depuis une image

**Layer ▾ › Layer from an image…** ajoute une image de ton ordinateur comme nouveau calque. Tu peux aussi ouvrir un `.psd` : ses calques, ses groupes, ses masques et ses modes de fusion sont repris.

```quiz
Tu veux ombrer une forme sans dépasser de son contour. Quel réglage ?
- [ ] Baisser Opacity.
- [x] Activer Lock transparent pixels.
- [ ] Passer en mode Screen.
explication: Le verrou des pixels transparents limite la peinture à ce qui existe déjà sur le calque.
```

```quiz
Quel mode de fusion convient pour une ombre ?
- [x] Multiply
- [ ] Screen
- [ ] Linear Dodge (Add)
explication: Multiply assombrit ce qui est dessous. Screen et Linear Dodge éclaircissent.
```

```defi
Ajoute un calque de lumière : un reflet blanc sur le rond, en mode **Screen**, à 50 % d'opacité.
indice: Place ce calque sous `trait` et au-dessus de `ombre`.
```

## Récapitulatif

- `Ctrl+Maj+N` crée un calque. Tu peins sur le calque actif.
- L'ordre, l'opacité et le mode de fusion décident du mélange.
- **Lock transparent pixels** garde la peinture dans la forme. `Ctrl+G` groupe.

Leçon suivante : [Sélectionner, recadrer, transformer](03-selections-et-transformations.md)
