# Peinture 3D

Peindre directement sur le modèle, dans l'esprit de Substance Painter. Chaque touche du pinceau est déposée dans une texture posée sur les UV de l'objet.

## Avant de peindre

L'objet doit avoir des **UV**. Sans UV, la peinture n'a nulle part où aller. Voir [UV et UDIM](3d-uv.md) : le bouton `Auto (seams + UVs)` suffit pour commencer.

## Entrer en peinture

1. Dans une scène, choisis un objet.
2. Mode `Paint` dans la barre.
3. `Leave painting` pour sortir.

## Peindre

| Geste | Effet |
|---|---|
| Glisser | Peindre |
| `Alt` + glisser, ou molette enfoncée | Tourner autour du modèle |
| `Maj` + molette enfoncée | Déplacer la vue |
| `[` et `]` | Taille du pinceau |
| `E` | Gomme |
| `Ctrl+Z` | Annuler |

## Réglages du pinceau

| Réglage | Effet |
|---|---|
| Couleur | La couleur déposée |
| `Radius ([ and ])` | Rayon du pinceau à l'écran |
| `Opacity` | Opacité d'une touche |
| `Edge hardness` | 0 : bord très doux. 1 : bord net |
| `Eraser (E)` | Retire la peinture au lieu d'en poser |
| `Clear the selected object's paint` | Efface toute la peinture de l'objet |

## Calques de peinture

Le panneau **Paint layers** liste les calques de l'objet choisi, du bas vers le haut. Chaque calque est une image PNG du coffre.

| Bouton | Effet |
|---|---|
| `Add a layer` | Nouveau calque. Tu choisis son canal |
| Œil | Montrer ou cacher le calque |
| Pinceau | Peindre sur ce calque |
| Curseur | Opacité du calque |
| Retirer | Retire le calque. Son image reste dans le coffre |

S'il n'y a pas encore de calque, le message « No layer yet: paint, or add one » s'affiche. Peindre en crée un.

### Les canaux

Un calque appartient à un canal. Le canal dit ce que la peinture change.

| Canal | Ce que tu peins |
|---|---|
| `Color` | La couleur de la surface |
| `Roughness` | La rugosité. 0 : lisse. 1 : rugueux |
| `Metal` | Le caractère métallique. 0 : non métallique. 1 : métal |
| `Mask 1 (Atelier)` à `Mask 4 (Atelier)` | Un masque peint à la main, lu par un calque de l'[Atelier](3d-atelier.md) |

Pour la rugosité, le métal et les masques, tu ne peins pas une couleur mais une **valeur** de 0 à 1.

**Exemple : salir le bas d'un mur.**

1. Dans l'Atelier, pose un calque « saleté » sur ton matériau. Mets son masque sur `Hand-painted`, canal 1.
2. Dans la scène, mode `Paint`. `Add a layer`, canal `Mask 1 (Atelier)`.
3. Peins le bas du mur : la saleté apparaît là où tu peins.

## Décalques

Le bouton **Decal** projette une image sur l'objet à l'endroit où tu cliques : un logo, une étiquette.

1. Clique `Decal` et choisis une image.
2. Clique sur le modèle. L'image est posée, à la taille du pinceau.
3. Clique de nouveau sur `Decal` pour le retirer du pinceau.

## Où va la peinture

- Les textures sont des PNG de 2048 × 2048 pixels, dans le dossier `textures/` du coffre.
- La scène garde le chemin de chaque calque dans l'objet. Voir [Formats](../technique/formats.md#scène-3d-aryscene).
- Dans la vue et au rendu, la peinture se mélange au matériau de l'objet.

## Limites

- La taille de texture est fixe : 2048 pixels de côté.
- Un seul pinceau rond. Pas de pinceaux à texture, pas de projection au pochoir, pas de peinture par symétrie.
- La peinture suit les UV : si tu refais les UV après avoir peint, la peinture ne correspond plus.
- Aux coutures UV, un trait peut laisser une fine ligne. Place les coutures dans les endroits peu visibles.
- **Incertain.** Le comportement sur un objet à plusieurs tuiles UDIM n'a pas été vérifié.
