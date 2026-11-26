---
titre: Tous les blocs en une leçon
niveau: débutant
duree: 5 min
prerequis:
objectifs: Ouvrir un point de départ ; Cocher une étape en faisant le geste ; Répondre à une question
---

Cette leçon montre ce que le panneau sait faire. Lis à droite, agis à gauche.

## Un point de départ

Le bouton crée une copie dans le dossier `Apprendre/` du coffre. L'original n'est jamais touché.

```essayer
exemple: tissu sur une boule
```

## Des étapes

```etape
Ouvre l'exemple avec le bouton ci-dessus. Tu arrives dans l'outil **Simulation** de l'espace **3D**.
verifier: fichier.ouvert = arygeo
```

```etape
Regarde le graphe : il contient déjà un node **Solver** (solveur).
verifier: graphe.contient = solveur
```

```etape
Lance la lecture avec **Espace** et laisse-la dépasser l'image 20.
verifier: lecture.image >= 20
```

```etape
Observe le tissu qui tombe sur la boule. Coche cette case quand tu l'as vu.
```

```astuce
Tu peux glisser l'en-tête du panneau pour le détacher dans sa propre fenêtre.
```

```attention
Changer le solveur vide le cache : la lecture repart de la première image.
```

```raccourci
**Espace** : lecture ou pause. **Maj+A** : ajouter un node.
```

## Une question

```quiz
Où vont les fichiers créés par le bouton « Essaie par toi-même » ?
- [ ] À la racine du coffre.
- [x] Dans le dossier Apprendre du coffre.
- [ ] Dans un dossier temporaire, effacé à la fermeture.
explication: Chaque essai est une copie rangée dans `Apprendre/`. Tu peux la garder, la renommer ou la jeter.
```

## Un défi

```defi
Ajoute un deuxième node dans le graphe et relie-le.
indice: **Maj+A** ouvre la recherche de nodes sous la souris.
```
