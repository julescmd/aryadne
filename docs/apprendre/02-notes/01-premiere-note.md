---
titre: Ta première note
niveau: débutant
duree: 15 min
prerequis: 01-demarrer/03-creer-et-enregistrer
objectifs: Écrire en Markdown ; Mettre en forme titres, listes et cases à cocher ; Passer de l'écriture à la lecture
---

## Ce que tu vas réussir à faire

Tu vas écrire la fiche d'un projet : un titre, un texte mis en forme, une liste et des cases à cocher. Tu la liras ensuite en mode lecture, comme une page propre.

## Une note, c'est du texte

Une note est un fichier `.md`. Tu écris du texte simple. Quelques signes donnent la mise en forme : c'est le Markdown.

```essayer
nouveau: md
nom: mon-projet
```

```etape
Clique **Essaie par toi-même**. Une note vide s'ouvre dans l'étape Notes.
verifier: onglet = notes
```

## Un titre

Un `#` suivi d'un espace fait un grand titre. Deux `#` font un titre plus petit.

```etape
Tape sur la première ligne : `# Mon projet`
verifier: texte.contient = projet
```

```etape
Saute une ligne. Tape : `## Idée`
verifier: texte.contient = Idée
```

## Du texte mis en forme

| Tu écris | Tu obtiens |
|---|---|
| `**gras**` | du gras |
| `*italique*` | de l'italique |
| `~~barré~~` | du texte barré |
| `==surligné==` | du texte surligné |

```etape
Sous le titre Idée, écris une phrase sur ton projet. Mets un mot en gras avec deux étoiles de chaque côté.
verifier: texte.contient = **
```

## Des listes et des cases

Un tiret fait une puce. `1.` fait une liste numérotée. `- [ ]` fait une case à cocher.

```etape
Ajoute un titre `## À faire`. Écris dessous trois lignes qui commencent par `- [ ] `, une tâche par ligne.
verifier: texte.contient = - [ ]
```

```astuce
Quand tu appuies sur `Entrée` au bout d'une ligne de liste, Aryadne commence la ligne suivante de la même façon.
```

## Lire la note

Une note a deux visages : le texte que tu écris, et la lecture.

```etape
Appuie sur `Ctrl+E`. La note s'affiche en lecture : les titres sont grands, les cases sont de vraies cases.
```

En haut de la note, deux petites icônes font le même geste : le livre pour **Reading**, le crayon pour **Text**.

```etape
En lecture, clique une case pour la cocher.
```

```etape
Reviens au texte avec `Ctrl+E`. La ligne cochée contient maintenant `- [x]`.
verifier: texte.contient = [x]
```

## Aller plus loin dans la mise en forme

| Tu écris | Tu obtiens |
|---|---|
| `> une phrase` | Une citation |
| `> [!note] Titre` | Un encadré |
| Un tableau Markdown | Un tableau |
| Trois accents graves, du code, trois accents graves | Du code coloré |
| `[^1]` puis `[^1]: texte` | Une note de bas de page |
| `$x^2$` | Des maths simples |

```etape
Ajoute une citation : une ligne qui commence par `> `. Regarde-la en lecture, puis reviens au texte.
verifier: texte.contient = >
```

```attention
Les maths sont rendues en caractères simples : exposants, lettres grecques, symboles. Pas de fractions empilées ni de matrices.
```

## Le plan de la note

À droite, le panneau a deux onglets : **Structure** et **Links**. **Structure** liste les titres de la note. Un clic sur un titre t'y amène.

```etape
Clique le titre « À faire » dans l'onglet **Structure**.
```

## Enregistrer

```etape
Enregistre avec `Ctrl+S`.
verifier: fichier.enregistre
```

```quiz
Comment écrire une case à cocher ?
- [ ] `[] tâche`
- [x] `- [ ] tâche`
- [ ] `* tâche`
explication: Un tiret, un espace, des crochets avec un espace dedans, puis le texte.
```

```quiz
Que fait Ctrl+E dans une note ?
- [ ] Il exporte la note.
- [x] Il passe du texte à la lecture, et retour.
- [ ] Il efface la ligne.
explication: Ctrl+E bascule entre Text et Reading.
```

```defi
Ajoute à ta note un tableau de deux colonnes : « Plan » et « Durée », avec trois lignes.
indice: Une ligne de tableau s'écrit `| Plan | Durée |`. La deuxième ligne est `|---|---|`.
```

## Récapitulatif

- Une note est un fichier texte. `#` fait un titre, `-` une puce, `- [ ]` une case.
- `Ctrl+E` passe du texte à la lecture.
- L'onglet **Structure** montre le plan de la note.

Leçon suivante : [Relier les notes](02-liens-entre-notes.md)
