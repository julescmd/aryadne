# Retopologie

La retopologie refait un maillage en **quads propres** à partir d'un modèle dense ou irrégulier (un sculpt, un scan). Elle est automatique. Tu la guides par des traits, de la densité peinte et quelques réglages.

## Où la trouver

1. Dans une scène, choisis l'objet.
2. Mode `UV & materials` (ou `UVs, materials & retopology`).
3. Onglet `Retopo`.

## Lancer, pas à pas

1. Règle `Target faces` (faces visées) : le nombre de quads voulu.
2. Coche `Symmetry` si le modèle est symétrique, et choisis l'axe.
3. `▶ Run retopology`.
4. L'avancement s'affiche : `Preparing mesh`, `Computing flow`, `Placing poles`, `Building quads`.
5. Le résultat apparaît en fil de fer bleu par-dessus le modèle, avec le nombre de pôles.
6. Applique-le à l'objet (« Retopology applied to the object »), ou exporte-le en OBJ.

`Cancel` arrête un calcul en cours.

## Guider le résultat

### Guides dessinés

Glisse sur le modèle pour dessiner un trait : les boucles de quads suivront sa direction. `Maj` ou le clic droit tournent la vue pendant ce temps.

- Pour une boucle autour de la bouche, d'un œil, d'un genou ou d'un coude : dessine un trait **fermé** tout autour.
- `Undo last` retire le dernier guide. `Clear guides` les retire tous.
- `Guide reach` (portée des guides) : jusqu'où un trait influence le flux autour de lui.

### Pinceau de densité

`Density brush` : peins sur le modèle là où il faut plus de quads (visage, mains, articulations) ou moins (dos, crâne). Réglages : `Radius`, `Density`. `Clear density` efface la peinture.

### Boucles automatiques

`Automatic loops` : Aryadne cherche les rebords qui font le tour d'un détail (yeux, bouche, narines, base d'une corne) et y met des boucles, comme si tu avais dessiné un guide.

## Réglages

| Réglage | Plage | Effet |
|---|---|---|
| `Target faces` | 50 à 10 millions | Nombre de quads visé. Le résultat s'en approche, sans l'atteindre exactement |
| `Symmetry` | X, Y ou Z | Le maillage est calculé sur une moitié puis recopié en miroir, avec une boucle sur le plan |
| `Follow curvature` | 0 à 2 | 0 : le flux le plus régulier possible. Plus haut : les boucles suivent les formes (membres, plis), au prix de quelques pôles en plus |
| `Adaptive size` | 0 à 1 | Les quads rétrécissent là où les boucles se resserrent, au lieu de se déformer |
| `Hard edges` | 0 à 90° | Au-delà de cet angle, une arête devient une boucle nette du maillage. 0 : désactivé. Les bords ouverts sont toujours suivis |
| `Guide reach` | 0,5 à 4 | Portée des guides dessinés |
| `Separate objects` | `Same density` ou `By detail` | Chaque morceau non relié est retopologisé à part |
| `Strength` | 0,1 à 1,5 | Avec `By detail` : plus la force est haute, plus un morceau chargé de détails reçoit de quads par rapport à un morceau lisse |

## Ce qu'il faut savoir sur les pôles

Un pôle est un sommet où se rejoignent 3 ou 5 arêtes (ou plus) au lieu de 4. Un maillage de quads en a forcément, sauf sur un tore. L'outil les place un à un. Un cube en donne 8, un personnage en a aux articulations.

Moins de pôles donne un maillage plus régulier, mais des boucles qui suivent moins bien les formes. Le réglage `Follow curvature` décide de ce compromis.

## Conseils

- **Modèle très lourd.** Un sculpt de plusieurs millions de triangles est simplifié avant le calcul. C'est normal.
- **Échec.** Le message « Retopology failed on this model » propose deux pistes : viser moins de faces, ou mettre `Hard edges` à 0.
- **Formes mécaniques.** Monte `Hard edges` (par exemple 30°) pour que les arêtes franches restent des boucles nettes.
- **Formes organiques.** Laisse `Hard edges` bas, coche `Automatic loops`, et dessine un guide fermé autour de chaque œil et de la bouche.

## Après la retopologie

Le nouveau maillage n'a pas d'UV. Passe à l'onglet `UV` (voir [UV et UDIM](3d-uv.md)). Pour reporter les détails du sculpt sur le maillage propre, indique le chemin du sculpt dans `High-poly` avant la cuisson.

## Limites

- Le résultat est 100 % quads et fermé, d'après les tests du code. Sur un modèle abîmé (trous, faces qui se croisent), la qualité baisse. Un passage par `Remesh (voxels)` avant aide.
- Le nombre de faces obtenu n'est pas exactement celui demandé.
- Il n'y a pas d'outil de retopologie manuelle (poser les quads un par un).
- **Incertain.** Le code contient deux descriptions de la méthode : l'en-tête de `retopo.rs` parle d'une approche inspirée d'Instant Meshes, le README d'un placement de pôles façon Mixed-Integer Quadrangulation. Le comportement réel doit être jugé à l'écran.
