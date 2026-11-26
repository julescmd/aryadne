---
titre: Prototype : survol, liens et présentation
niveau: intermédiaire
duree: 15 min
prerequis: 04-vectoriel/05-composants-et-bibliotheque
objectifs: Relier un bouton à un autre plan de travail ; Donner un état de survol à une forme ; Présenter le prototype ; Commenter une maquette
---

## Ce que tu vas réussir à faire

Tu vas transformer deux écrans dessinés en prototype cliquable. Un bouton réagira au survol de la souris, et un clic mènera au deuxième écran.

```essayer
nouveau: aryvec
nom: prototype
```

```etape
Clique **Try it yourself**.
verifier: fichier.ouvert = aryvec
```

## Deux écrans

```etape
Trace un plan de travail avec `F`, ou choisis celui qui existe. Clique **Size…** et choisis le format iPhone. Dans la section **Artboard** des propriétés, tape son nom : `Accueil`.
```

```etape
Trace un deuxième plan à côté, au même format. Nomme-le `Détail`.
```

```etape
Sur `Accueil`, pose un titre et un bouton : un rectangle arrondi avec un texte. Sur `Détail`, pose un autre titre et un bouton « Retour ».
verifier: calques >= 4
```

```astuce
Montre tes écrans dans un cadre de téléphone : sur le plan de travail, règle **Device**. Le cadre et sa barre d'état s'affichent, et partent aussi dans l'export SVG.
```

## Relier un bouton à un écran

```etape
Choisis le rectangle du bouton de `Accueil`. Dans ses propriétés, trouve le réglage **Prototype: on click, go to this artboard**. Choisis `Détail`.
```

```etape
Fais de même pour le bouton « Retour » : il mène à `Accueil`.
```

## Un état de survol

En présentation, une forme peut changer quand la souris passe dessus. C'est le **Hover state**.

```etape
Choisis le rectangle du bouton de `Accueil`. Active **Hover state**.
```

| Réglage | Effet au survol |
|---|---|
| **Color** | La forme prend cette couleur |
| **Opacity** | La forme change d'opacité |
| **Lifts** | Une ombre apparaît sous la forme : elle se soulève |

```etape
Choisis une **Color** de survol un peu plus claire que celle du bouton. Coche **Lifts**.
```

L'état de survol ne se voit pas pendant que tu dessines. Il ne joue qu'en présentation.

## Présenter

```etape
Clique le bouton **Present the prototype**. Le premier plan de travail s'affiche seul.
```

```etape
Passe la souris sur le bouton : il s'éclaircit et se soulève. Clique : tu arrives sur `Détail`. Clique « Retour ».
```

| Touche | Effet |
|---|---|
| Clic sur une forme liée | Va à son plan de travail |
| Flèches | Change de plan |
| `Échap` | Sort de la présentation |

```etape
Sors avec `Échap`.
```

## Commenter

L'outil **Comment** (`C`) épingle une note sur la toile. Les commentaires ne sont jamais exportés.

```etape
Appuie sur `C`. Clique près du bouton et écris une remarque : « couleur à valider ».
```

Un clic sur la bulle ouvre le commentaire. Tu peux le marquer **Resolved** ou le retirer.

## Comparer deux versions de la maquette

**File › Version history** marche aussi pour un dessin vectoriel. La comparaison compte les formes ajoutées, retirées et modifiées, et montre les différences en rose.

```etape
Enregistre avec `Ctrl+S`. Change la couleur d'un bouton, enregistre encore. Ouvre **File › Version history** et compare avec la version d'avant.
```

## Limites du prototype

- Un lien va d'une forme vers un plan de travail, au clic.
- L'état de survol change la couleur, l'opacité et l'ombre.
- Il n'y a pas de transition animée entre deux plans en présentation, ni de défilement.

Pour une vraie animation entre deux écrans, regarde la leçon suivante.

```quiz
Quand voit-on l'état de survol d'une forme ?
- [ ] Tout le temps.
- [x] En présentation, quand la souris passe sur la forme.
- [ ] À l'export PNG.
explication: Le Hover state ne joue que dans Present the prototype.
```

```quiz
Comment sortir de la présentation ?
- [x] Avec Échap.
- [ ] Avec Ctrl+W.
- [ ] En cliquant le fond.
explication: Échap ramène à la toile. Les flèches changent de plan sans passer par les liens.
```

```defi
Ajoute un troisième écran « Réglages » et une barre de navigation à trois boutons, présente sur chaque écran, qui mène aux trois.
indice: Fais de la barre un composant avec `Ctrl+Alt+K`, puis pose une instance sur chaque plan.
```

## Récapitulatif

- **Prototype: on click, go to this artboard** relie une forme à un plan de travail.
- **Hover state** change couleur, opacité et ombre au survol, en présentation seulement.
- **Present the prototype** joue la maquette. `Échap` en sort.

Leçon suivante : [Mode Dev et export](07-mode-dev-et-export.md)
