---
titre: Extruder, incruster, biseauter
niveau: débutant
duree: 20 min
prerequis: 08-modelisation/01-le-mode-edition
objectifs: Sortir de la matière avec Extrude ; Incruster une face avec Inset ; Adoucir une arête avec Bevel ; Régler un outil en direct
---

## Ce que tu vas réussir

Tu vas construire une maison à partir d'un cube : un toit, une porte en creux, une cheminée, des arêtes adoucies. Trois outils suffisent.

```essayer
nouveau: aryscene
nom: petite-maison
```

```etape
Choisis le cube et entre en mode édition avec Tab. Passe en mode **Faces** avec 3.
verifier: scene.selection
```

## Extrude : sortir de la matière

**Extrude** sort les faces choisies et crée les côtés qui manquent.

```etape
Clique la face du dessus. Appuie sur E. Monte la souris : un étage sort du cube. Valide d'un clic.
```

```raccourci
E : extruder les faces choisies.
```

### Le toit

```etape
La face du dessus est encore choisie. Appuie sur S et resserre-la presque jusqu'à un point. Valide.
```

L'étage est devenu un toit à quatre pans.

```astuce
Extruder puis redimensionner : c'est le geste le plus courant de la modélisation. Une tête, un bras, un goulot de bouteille se font ainsi.
```

## Inset : incruster

**Inset** dessine une face plus petite à l'intérieur d'une face.

```etape
Clique une face d'un mur. Appuie sur I. Bouge la souris : une face plus petite apparaît dedans. Valide.
```

```raccourci
I : incruster. En mode édition seulement : en mode objet, I pose une clé.
```

### La porte en creux

```etape
La face intérieure est choisie. Appuie sur E et pousse vers l'intérieur du mur. Valide. Tu as une porte en retrait.
```

## Les outils à réglage en direct

**Inset**, **Bevel**, **Subdivide** et quelques autres se règlent pendant que tu les utilises.

1. Lance l'outil.
2. Bouge la souris : la valeur change. Elle s'affiche près du curseur.
3. La molette règle le second paramètre : segments, coupes.
4. Ctrl arrondit la valeur.
5. Clic valide. Clic droit remet la valeur par défaut. Échap annule.

```etape
Clique une autre face de mur. Appuie sur I, bouge la souris, puis tiens Ctrl : la valeur saute par crans. Valide.
```

Pendant **Inset**, I bascule entre **Whole region** et **Individual faces**. Avec plusieurs faces choisies, le premier fait un seul contour, le second un contour par face.

## La cheminée

```etape
Clique un des pans du toit. Appuie sur I pour y incruster une petite face. Valide.
```

```etape
Appuie sur E et sors la cheminée. Valide.
```

## Bevel : adoucir les arêtes

Dans la réalité, aucune arête n'est parfaitement vive. **Bevel** remplace une arête par un chanfrein.

```etape
Passe en mode **Edges** avec 2. Fais un Alt+clic sur une arête du bas des murs, pour prendre la boucle.
```

```etape
Appuie sur Ctrl+B. Bouge la souris pour régler **Width**. Tourne la molette : **Segments** augmente et le chanfrein s'arrondit. Valide.
```

| Réglage | Geste | Effet |
|---|---|---|
| **Width** | Souris | Largeur du biseau |
| **Segments** | Molette | 1 : un pan coupé. Plus : un arrondi |

```astuce
Un biseau fin, d'un ou deux segments, suffit à accrocher la lumière. C'est ce qui fait la différence entre un objet de synthèse et un objet crédible.
```

### Biseauter un sommet

**Vertex bevel**, avec Ctrl+Maj+B, remplace un sommet par une petite face. Essaie-le sur la pointe du toit.

## Subdivide : plus de faces

```etape
Passe en mode **Faces**. Clique un mur. Fais un clic droit dans la vue et choisis **Subdivide**. Tourne la molette pour régler **Cuts**. Valide.
```

Le mur est découpé en faces plus petites. Tu peux maintenant y incruster des fenêtres.

## Le menu des outils

Le clic droit dans la vue ouvre le menu du mode édition. Il range les outils par famille : **Vertices**, **Edges**, **Faces**, **Mesh**.

```etape
Sors du mode édition avec Tab. Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```quiz
Tu veux une porte en creux dans un mur. Quel enchaînement ?
- [x] Inset, puis Extrude vers l'intérieur.
- [ ] Bevel, puis Subdivide.
- [ ] Extrude deux fois.
explication: Inset dessine le contour de la porte. Extrude pousse la face intérieure dans le mur.
```

```quiz
Pendant un Bevel, que règle la molette ?
- [ ] La largeur.
- [x] Le nombre de segments.
- [ ] La couleur.
explication: La souris règle la largeur. La molette règle le second paramètre : ici les segments.
```

```defi
Ajoute deux fenêtres en creux sur un mur.
indice: Subdivide le mur, choisis deux faces, Inset avec I pressé une seconde fois pour Individual faces, puis Extrude vers l'intérieur.
```

## En trois lignes

- E sort de la matière. I incruste une face plus petite.
- Ctrl+B adoucit une arête. La molette règle ses segments.
- Souris, molette, Ctrl, clic : tous les outils en direct se règlent pareil.

[Suite : Couper, souder, nettoyer](03-couper-souder-nettoyer.md)
