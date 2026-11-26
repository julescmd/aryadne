---
titre: Projet : un drapeau, puis une nappe
niveau: avancé
duree: 30 min
prerequis: 14-simulation/06-vent-lecture-et-cache
objectifs: Monter un drapeau au vent de bout en bout ; Poser une nappe sur une table ; Figer le résultat ; Faire monter une simulation par l'Assistant
---

## Ce que tu vas réussir

Deux simulations finies. Un drapeau accroché à son mât, qui flotte dix secondes. Une nappe qui tombe sur une table et pend sur les côtés. À la fin, tu refais le drapeau en une phrase, avec l'Assistant.

## Projet 1 : le drapeau au vent

```essayer
exemple: tissu sur une boule
```

Tu pars du tissu sur une boule. Tu vas le transformer.

### 1. Dresse le tissu

```etape
Sur **Grid**, choisis **Plane** : **wall (XY)**. Le tissu est maintenant debout.
```

```etape
Toujours sur **Grid**, mets **Width** à 3, **Depth** à 2, **Points across** à 48 et **Points deep** à 32. Vérifie qu'**Edges** vaut **with diagonals**.
```

Une grille en mur de largeur 3 va de x = -1,5 à x = 1,5. Le node **Move, rotate** qui suit la monte de 1,6 : le drapeau flotte au-dessus du sol.

### 2. Retire la boule

```etape
Sur **Solver**, mets **Ball radius** à 0. L'obstacle est retiré.
```

```etape
En bas du graphe, supprime les trois nodes qui montraient la boule : **Points on a sphere**, **Move, rotate** et **Set an attribute**. Choisis-les, puis Suppr.
```

### 3. Accroche le bord au mât

Le mât est le bord gauche : les points dont X est le plus petit.

```etape
Dans le cadre bleu, relie la sortie **X** de **Separate XYZ** à l'entrée **A** de **Compare**. Ce lien remplace celui qui venait de **Z**.
```

```etape
Sur **Compare**, garde **less than**. Mets **B** à -1,49.
```

```etape
Clique le node **Set an attribute** qui écrit `pin`. Dans **ATTRIBUTES**, vérifie que `pin` va de 0,00 à 1,00.
```

### 4. Fais souffler

```etape
Sur **Forces**, mets **Wind** à (14, 0, 3) et **Air drag** à 0,3.
```

Le vent pousse surtout le long du drapeau, avec un peu de travers. Sans ce travers, le drapeau resterait dans son plan.

### 5. Choisis le solveur

```etape
Sur **Solver**, choisis **Solver** : **cloth (graphics card)**. Mets **Floor** sur **not**.
verifier: graphe.contient = sim_solver
```

### 6. Allonge et lis

```etape
Ouvre la roue dentée de la barre de lecture. Mets **Last frame** à 240.
```

```etape
Reviens à la première image et lis jusqu'à l'image 120.
verifier: lecture.image >= 120
```

Le drapeau se déploie puis flotte.

### 7. Règle la matière

Ouvre le menu du tissu, dans le coin haut droit de l'aperçu.

```etape
Le drapeau s'étire près du mât ? Monte **Iterations** à 32.
```

```etape
Tu veux une étoffe légère ? Baisse **Bending** vers 0,02. Les plis deviennent fins et nerveux.
```

```etape
Les plis frémissent trop ? Monte **Fold damping** vers 0,5.
```

### 8. Fige

```etape
Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```etape
Clique **Simulation end**. Dans la carte **SIMULATION CACHE**, clique **Bake to disk**. Relis : plus aucun calcul.
```

```astuce
Pour un vent vivant, reprends la leçon 6 : une **Expression** qui lit **Time**, reliée à **Wind** par un **Combine XYZ**.
```

## Projet 2 : la nappe sur la table

```essayer
exemple: tissu sur la carte graphique
```

Cet exemple est déjà une nappe sans point fixe, calculée par le solveur **Cloth (graphics card)**. Il te reste à construire la table.

### 1. La table dans le Solver

```etape
Sur **Solver**, mets **Ball radius** à 0. Plus de boule.
```

```etape
Mets **Box center** à (0, 0,4, 0) et **Box size** à (1,2, 0,8, 0,8). La boîte est posée sur le sol. Son dessus est à 0,8 de haut.
```

```etape
Vérifie que **Floor** vaut **yes**.
```

### 2. Montre la table

Le **Solver** connaît la table, mais tu ne la vois pas. Tu vas la remplir de points.

```etape
Supprime le node **Points on a sphere**, en bas du graphe.
```

```etape
Ajoute un node **Point cloud**. Mets **Points** à 2000 et **Scale** à (1,2, 0,8, 0,8).
verifier: graphe.contient = geo_scatter
```

```etape
Relie **Point cloud** au node **Move, rotate** qui portait la boule. Mets la **Translation** de ce node à (0, 0,4, 0).
```

Le nuage de points occupe exactement le volume de la boîte.

### 3. Lâche la nappe

```etape
Reviens à la première image et lis jusqu'à l'image 80.
verifier: lecture.image >= 80
```

La nappe tombe, se pose sur le dessus et pend autour.

### 4. Soigne les plis

```etape
Ouvre le menu du tissu. Active **Self-collision**. Les pans qui pendent ne se traversent plus dans les coins.
```

```etape
Mets **Friction** à 0,6. La nappe glisse moins sur la table.
```

```etape
Les coins traversent la table ? Monte **Substeps** à 12.
```

### 5. Fige

```etape
Enregistre avec Ctrl+S, puis fige avec **Bake to disk** sur **Simulation end**.
verifier: fichier.enregistre
```

## Bonus : demande à l'Assistant

L'Assistant sait monter ce graphe à ta place. Il lui faut un modèle branché : voir [Assistant IA](../../utilisateur/assistant-ia.md).

```essayer
nouveau: arygeo
nom: drapeau-assistant
```

```etape
Ouvre l'Assistant avec Ctrl+L. Sous le titre du panneau, vérifie la ligne « Sees and edits: the graph ».
```

```etape
Écris : « fais un drapeau qui flotte au vent, en simulation de tissu ». Envoie avec Entrée.
```

Sous la réponse, une carte **Apply to the graph** résume les changements : les nodes posés, les réglages, le nombre de liens.

```etape
Clique **All the details** pour lire la liste complète. Puis clique **Apply**.
verifier: graphe.contient = sim_solver
```

Rien n'est modifié avant ton clic. Tout est appliqué en une seule étape : un Ctrl+Z dans le graphe annule l'ensemble.

```etape
Lis la simulation. Compare avec ton drapeau fait à la main.
verifier: lecture.image >= 60
```

```attention
Si la carte devient **Change refused**, rien n'a été appliqué. Elle liste les erreurs. Clique **Ask for a fix** : l'Assistant corrige sa proposition. Un petit modèle local se trompe plus souvent qu'un modèle en ligne.
```

Tu peux ensuite demander des retouches : « ajoute du vent », « rends le tissu plus souple ». Et tu sais maintenant lire ce qu'il a posé.

```quiz
Pourquoi le drapeau a-t-il besoin d'un vent avec un peu de Z ?
- [ ] Pour aller plus vite.
- [x] Sans travers, il resterait dans son plan.
- [ ] Le Solver refuse un vent sur un seul axe.
explication: Un vent exactement dans le plan du drapeau ne le fait pas sortir de ce plan. Un léger travers lance les ondulations.
```

```quiz
Tu cliques Apply sur la carte de l'Assistant, et le résultat ne te plaît pas. Que fais-tu ?
- [ ] Tu fermes le fichier sans enregistrer.
- [x] Ctrl+Z dans le graphe : tout le changement est annulé d'un coup.
- [ ] Tu supprimes les nodes un par un.
explication: Le changement de l'Assistant compte pour une seule étape d'annulation.
```

```defi
Fais une nappe ronde sur la table.
indice: Avant la zone, supprime les points trop loin du centre : Location, Vector math réglé sur length, Compare, puis Delete points.
```

## En trois lignes

- Un drapeau : grille en mur, bord épinglé, vent, solveur sur carte graphique.
- Une nappe : pas de point fixe, une boîte et le sol dans le **Solver**, l'auto-collision.
- L'Assistant propose, tu appliques d'un clic, et Ctrl+Z annule tout.

Parcours terminé. Pour mélanger la simulation avec le reste, va voir les [projets guidés](../16-projets/index.md).
