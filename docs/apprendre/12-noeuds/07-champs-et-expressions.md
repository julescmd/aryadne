---
titre: Champs et expressions
niveau: avancé
duree: 20 min
prerequis: 12-noeuds/06-math-et-logique
objectifs: Comprendre ce qu'est un champ ; Déplacer des points avec un champ ; Écrire une formule dans un node Expression ; Lire le temps ; Découvrir le node Custom shader
---

## Ce que tu vas réussir

Tu vas déformer une grille de points avec un bruit, puis avec une formule que tu écris toi-même. Tu vas la faire bouger dans le temps.

```essayer
exemple: vagues animées
```

## Qu'est-ce qu'un champ

Dans un graphe de géométrie, une entrée de nombre, de vecteur ou de couleur n'attend pas **une** valeur. Elle attend une valeur **par point**. C'est un champ.

Tous les nodes de calcul que tu connais servent à construire des champs : **Math**, **Compare**, **Noise**, **Random**, **Expression**. Branchés sur une géométrie, ils sont calculés pour chaque point.

```etape
Lis la simulation quelques instants avec Espace, souris sur l'aperçu. Puis mets en pause.
verifier: lecture.image >= 10
```

## Un champ simple : le bruit

```etape
Trouve le node **Set point positions**. Son entrée **Offset** reçoit un lien. Coupe-le avec Ctrl+clic. La grille redevient plate.
```

```etape
Ajoute un node **Noise**. Mets **Scale** à 2.
verifier: graphe.contient = noise
```

```etape
Ajoute un node **Combine XYZ**. Relie la sortie **Value** de **Noise** à son entrée **Y**. Relie **Combine XYZ** à **Offset**.
verifier: graphe.liens >= 9
```

Chaque point monte d'une hauteur différente, lue dans le bruit à sa position. La grille devient un relief.

```astuce
Un node de champ le dit dans sa miniature : « Field: one value per point ». Appuie sur P, le node choisi, pour l'afficher.
```

## Le node Expression

Au lieu d'enchaîner dix nodes **Math**, tu peux écrire une formule.

```etape
Clique le node **Expression** déjà présent dans le graphe. Lis sa formule.
```

Une **Expression** a quatre entrées, `a`, `b`, `c`, `d`, et une sortie. Dans la formule, tu disposes de :

| Variable | Sens |
|---|---|
| `a`, `b`, `c`, `d` | Les quatre entrées |
| `x`, `y`, `z` | La position du point |
| `u`, `v` | Les coordonnées de texture |
| `pi` | 3,14159… |

```etape
Rebranche la sortie de **Expression** sur l'entrée **Y** de **Combine XYZ**, à la place du bruit.
```

```etape
Remplace toute la formule par : `sin(x * 6) * 0.2`
verifier: graphe.contient = expr
```

Des vagues droites, immobiles. `x` est lu pour chaque point.

## Ajoute le temps

L'entrée `a` reçoit déjà le node **Time**. Sa sortie **Frame** donne l'image courante, **Seconds** le temps en secondes.

```etape
Change la formule : `sin(x * 6 + a * 0.2) * 0.2`
```

```etape
Lance la lecture. Les vagues avancent.
verifier: lecture.image >= 30
```

## Des conditions dans une formule

```etape
Change la formule : `if(z > 0, sin(x * 6 + a * 0.2) * 0.2, 0)`
```

Une moitié de la grille ondule, l'autre reste plate. `if` prend une condition, la valeur si elle est vraie, la valeur sinon.

## Les fonctions

| Fonction | Sens |
|---|---|
| `sin`, `cos`, `tan`, `atan2(y, x)` | Trigonométrie |
| `abs`, `floor`, `ceil`, `fract`, `sqrt`, `sign` | Une valeur |
| `pow(a, b)`, `min`, `max`, `mod` | Deux valeurs |
| `clamp(x, a, b)` | Borne `x` entre `a` et `b` |
| `lerp(a, b, t)` ou `mix(a, b, t)` | Mélange de `a` vers `b` |
| `step(seuil, x)` | 0 sous le seuil, 1 au-dessus |
| `smoothstep(a, b, x)` | Passage doux de 0 à 1 |
| `length(x, y)` | Longueur |
| `noise(x, y, z)`, `fbm(x, y, z)` | Bruit lisse, bruit fractal |
| `rand(x, y)` | Hasard stable pour un même point |
| `if(condition, si vrai, si faux)` | Choix |

Opérateurs : `+`, `-`, `*`, `/`, les parenthèses, et `>` et `<` dans une condition.

```etape
Essaie un relief qui respire : `fbm(x, a * 0.02, z) * 0.5`
```

```attention
Si une formule est refusée, le node est marqué en erreur et le message donne la raison. Corrige la formule : le graphe reprend aussitôt.
```

## Le node Custom shader

Dans un matériau ou un compositing, le node **Custom shader** va plus loin : plusieurs lignes, exécutées pour chaque pixel. Tu écris la couleur de sortie dans `r`, `g`, `b`, `a`.

```
d = length(x - 0.5, y - 0.5) * 2
r = lerp(ar, br, d)
g = lerp(ag, bg, d)
b = lerp(ab, bb, d)
```

Cet exemple mélange les entrées A et B par un dégradé radial. `ar`, `ag`, `ab` sont les canaux de l'entrée A. Le node fournit des exemples prêts à lire. Son bouton **AI** écrit le code d'après une description, si un modèle est branché.

```quiz
Dans un graphe de géométrie, que reçoit l'entrée Offset de Set point positions ?
- [ ] Une seule valeur pour toute la grille.
- [x] Un champ : une valeur par point.
- [ ] Une géométrie.
explication: Une entrée de vecteur d'un node de géométrie est un champ. Chaque point reçoit sa propre valeur.
```

```quiz
Dans une Expression, que valent x, y et z ?
- [ ] Les trois premières entrées.
- [x] La position du point en cours de calcul.
- [ ] Toujours zéro.
explication: Les entrées sont a, b, c et d. x, y et z sont la position, lue pour chaque point.
```

```defi
Écris des ronds dans l'eau qui s'éloignent du centre et s'affaiblissent avec la distance.
indice: La distance au centre est `length(x, z)`. Divise l'amplitude par `1 + length(x, z) * 3`.
```

## En trois lignes

- Un champ est une valeur calculée pour chaque point.
- **Expression** remplace une chaîne de **Math** par une formule.
- **Time** fait varier une formule d'une image à l'autre.

[Suite : Aperçu, cache et erreurs](08-apercu-cache-erreurs.md)
