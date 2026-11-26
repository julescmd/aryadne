---
titre: Organiser : dossiers, propriétés, tags, modèles
niveau: débutant
duree: 15 min
prerequis: 02-notes/02-liens-entre-notes
objectifs: Donner des propriétés à une note ; Poser des tags ; Tenir une note du jour ; Créer une note depuis un modèle ; Exporter une note
---

## Ce que tu vas réussir à faire

Tu vas transformer une note en fiche : un type, un statut, des tags. Tu sauras aussi lancer ta note du jour et partir d'un modèle au lieu d'une page blanche.

```essayer
fichier: fichiers/plan-sh010.md
```

```etape
Clique **Try it yourself**. La fiche d'un plan s'ouvre.
verifier: fichier.ouvert = md
```

## Les propriétés

Regarde le haut du texte. Entre deux lignes `---`, la note porte un en-tête : ce sont ses **propriétés**.

```
---
type: plan
statut: à faire
durée: 4
tags: phare, nuit
---
```

Une propriété est un nom, deux points, une valeur. Tu choisis les noms que tu veux.

```etape
Passe en lecture avec `Ctrl+E`. Les propriétés s'affichent en haut, dans un bloc propre.
```

```etape
En lecture, double-clique la valeur de `statut`. Remplace-la par `en cours`. Valide.
verifier: texte.contient = en cours
```

```astuce
Clic droit sur le nom d'une propriété : **Rename across the vault…** la renomme dans toutes les notes. **Remove from all notes** la retire partout.
```

```etape
Reviens au texte avec `Ctrl+E`. Ajoute une ligne `échéance: 2026-12-04` dans l'en-tête, avant le deuxième `---`.
verifier: texte.contient = échéance
```

Les propriétés servent à trier et à filtrer. Tu le verras dans la leçon suivante, avec les tableaux.

## Les tags

Un tag est un mot précédé d'un `#`, n'importe où dans le texte. Il regroupe des notes sans les ranger dans un dossier.

```etape
Ajoute à la fin de la note une ligne avec le tag `#layout`.
verifier: texte.contient = #layout
```

Tu retrouves les tags sur la page de recherche : elle affiche tous les tags du coffre. Un clic sur un tag liste ses notes.

## Dossiers ou liens ?

- Un **dossier** range : une note n'est qu'à un endroit.
- Un **tag** regroupe : une note peut en porter plusieurs.
- Un **lien** relie : il dit qu'une note parle d'une autre.

Commence simple. Un dossier par projet, des liens entre les notes, quelques tags.

## La note du jour

```etape
Appuie sur `Ctrl+J`. La note du jour s'ouvre. Elle porte la date et se range dans `journal/`.
verifier: fichier.ouvert = md
```

```attention
`Ctrl+J` ne crée la note du jour que si la souris n'est pas sur un éditeur qui garde cette touche pour lui. Sur la vue 3D, `Ctrl+J` joint des objets. La palette marche partout : **Daily note**.
```

```etape
Écris une ligne dans ta note du jour : ce que tu as appris aujourd'hui.
```

## Partir d'un modèle

Un modèle est une note du dossier `modèles/` de ton coffre. Trois mots y sont remplacés à la création : `{{titre}}`, `{{date}}` et `{{heure}}`.

```etape
Ouvre **File › New › From a template…**. Choisis un modèle et donne un titre.
```

Pour créer ton propre modèle, écris une note dans `modèles/`. Elle apparaîtra dans cette liste.

## Exporter une note

Le bouton **Export the note**, dans la barre du fichier, propose deux sorties.

- **HTML page (images included)** : une page autonome, à envoyer à quelqu'un.
- Un PDF simple : titres, paragraphes, listes, citations.

```attention
Le PDF ne reprend ni les tableaux ni les images. Pour une note illustrée, choisis le HTML.
```

## L'orthographe

Dans les réglages, onglet **Editor**, coche **Spell checker for notes (French and English)**. Les mots inconnus sont soulignés. Un clic droit propose une correction, ou l'ajout du mot au dictionnaire du coffre.

```quiz
Où s'écrivent les propriétés d'une note ?
- [ ] À la fin, après le texte.
- [x] Tout en haut, entre deux lignes `---`.
- [ ] Dans un fichier à part.
explication: L'en-tête ouvre la note. Chaque ligne est un nom, deux points, une valeur.
```

```quiz
Quels mots sont remplacés dans un modèle de note ?
- [x] `{{titre}}`
- [x] `{{date}}`
- [x] `{{heure}}`
- [ ] `{{auteur}}`
explication: Trois mots sont remplacés à la création : titre, date et heure.
```

```defi
Crée ton propre modèle « fiche de plan » avec les propriétés type, statut et durée, puis crée une note SH020 à partir de lui.
indice: Écris la note dans le dossier `modèles/`. Mets `# {{titre}}` comme titre.
```

## Récapitulatif

- Les propriétés vivent en haut de la note, entre deux `---`. Elles se modifient aussi en lecture.
- `#mot` pose un tag. `Ctrl+J` ouvre la note du jour.
- **File › New › From a template…** part d'un modèle du dossier `modèles/`.

Leçon suivante : [Tâches et tableaux](04-taches-et-bases.md)
