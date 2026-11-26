---
titre: Le remaillage dynamique et la suite
niveau: avancé
duree: 15 min
prerequis: 09-sculpture/03-la-multiresolution
objectifs: Ébaucher avec le remaillage dynamique ; Régler la taille du détail ; Choisir entre les trois façons d'avoir des sommets ; Enchaîner vers retopologie, UV et cuisson
---

## Ce que tu vas réussir

Tu vas tirer des cornes et des membres d'une simple boule, sans jamais manquer de sommets. Puis tu sauras quoi faire de ton sculpt une fois fini.

```essayer
nouveau: aryscene
nom: ebauche-dynamique
```

```etape
Ajoute une **Sphere**, choisis-la et passe en mode **Sculpt**.
verifier: scene.selection
```

## Le problème

```etape
Prends **Grab** et tire une longue corne hors de la sphère.
```

La corne est faite de quelques grandes faces étirées. Impossible d'y sculpter un détail : il n'y a pas de sommets.

```etape
Annule avec Ctrl+Z.
```

## Le remaillage dynamique

```etape
Dans la barre, clique l'icône de grille : **Dynamic topology**. Un champ en pourcentage apparaît à côté.
```

La surface se subdivise maintenant sous le pinceau, pendant que tu sculptes.

```etape
Prends **Draw** et sculpte une bosse. Regarde le maillage se densifier là où tu passes, et seulement là.
```

```etape
Tire de nouveau une corne, par petites touches de **Draw** et de **Clay**. Elle reçoit des sommets à mesure qu'elle pousse.
```

## La taille du détail

Le champ en pourcentage règle **Detail size** : la taille du détail, en part du rayon du pinceau. Plus petit : plus fin.

```etape
Baisse la valeur vers 10 %. Sculpte : le maillage devient très fin sous le pinceau.
```

```etape
Remonte vers 50 %. Sculpte ailleurs : le maillage reste grossier.
```

Comme le détail suit le rayon, un petit pinceau donne un maillage fin, un grand pinceau un maillage large. Tu doses la densité sans y penser.

```attention
Le remaillage dynamique **remplace** les niveaux de multirésolution. Les deux ne se combinent pas.
```

## Quand s'en servir

Le remaillage dynamique sert à **ébaucher**. Tu cherches une forme, tu ajoutes un bras, tu changes d'avis. La topologie ne t'arrête jamais.

Son défaut : le maillage obtenu est irrégulier. Il ne se déplie pas bien et ne s'anime pas bien.

| Méthode | Pour | Limite |
|---|---|---|
| **Remesh (voxels)** | Repartir d'une surface saine | Le détail déjà sculpté est adouci |
| Multirésolution | Détailler une base propre | Demande une bonne base |
| **Dynamic topology** | Chercher une forme | Maillage irrégulier |

## Le trajet complet d'un sculpt

Un sculpt fini n'est pas un modèle utilisable. Il faut encore trois étapes.

1. **Retopologie.** Un maillage propre, en quads, avec peu de faces. Voir [La retopologie](../08-modelisation/06-la-retopologie.md).
2. **UV.** Le maillage propre est déplié à plat. Voir [Déplier les UV](../10-uv-textures/01-deplier-les-uv.md).
3. **Cuisson.** Les détails du sculpt passent dans une carte de normales, posée sur le maillage propre. Voir [La cuisson](../10-uv-textures/06-la-cuisson.md).

Le maillage léger porte alors l'apparence du sculpt lourd.

```astuce
Garde toujours le fichier du sculpt dense. À la cuisson, tu donnes son chemin dans le champ **High-poly**.
```

## Un flux de travail type

1. **Add**, **Sphere**.
2. **Dynamic topology** : l'ébauche, avec **Grab** et **Clay**.
3. Modificateur **Remesh (voxels)**, **Apply** : une surface égale.
4. Retopologie : une base propre.
5. Multirésolution sur cette base : les détails.
6. UV, puis cuisson.

```etape
Sors avec **Leave sculpting** et enregistre.
verifier: fichier.enregistre
```

## Ce qui n'existe pas encore

- Sept pinceaux. Pas de masques, pas de pinceaux à texture, pas de couches de sculpt.
- La symétrie n'existe que sur X.

```quiz
À quoi sert surtout le remaillage dynamique ?
- [x] À ébaucher une forme sans se soucier de la topologie.
- [ ] À produire un maillage prêt pour l'animation.
- [ ] À ajouter des niveaux de multirésolution.
explication: Il donne des sommets là où tu sculptes. Le maillage reste irrégulier : il faudra retopologiser.
```

```quiz
Peut-on combiner remaillage dynamique et multirésolution ?
- [ ] Oui, ils s'additionnent.
- [x] Non : le remaillage dynamique remplace les niveaux.
- [ ] Seulement au niveau 0.
explication: Le remaillage change la topologie à chaque trait. Les niveaux, eux, supposent une topologie stable.
```

```defi
Ébauche une petite créature en dix minutes : un corps, quatre pattes, une queue, deux cornes.
indice: Dynamic topology, Grab et Clay, symétrie X. Ne zoome pas : reste sur la silhouette.
```

## En trois lignes

- **Dynamic topology** ajoute des sommets sous le pinceau.
- **Detail size** suit le rayon : petit pinceau, maillage fin.
- Après le sculpt : retopologie, UV, cuisson.

Parcours terminé. Continue avec [UV et textures](../10-uv-textures/index.md).
