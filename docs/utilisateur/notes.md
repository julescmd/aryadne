# Notes et liens

Étape **Notes**. Trois outils : `Notes` (les fichiers), `Tasks` (les tâches), `Brain` (le graphe du coffre).

## Écrire une note

Une note est un fichier `.md` : du texte en Markdown.

- `Ctrl+N` : nouvelle note. Elle va dans le dossier choisi dans l'explorateur, sinon dans le dossier des nouvelles notes (`Settings › Editor › New notes folder`).
- `Ctrl+J` : note du jour. Elle est rangée dans `journal/`.
- `File › New › From a template…` : une note depuis un modèle.
- `Ctrl+E` : passer de l'écriture (`Editing`) à la lecture (`Reading`).

### Ce que le mode lecture affiche

| Écrit | Rendu |
|---|---|
| `# Titre`, `## Titre` | Titres |
| `**gras**`, `*italique*`, `~~barré~~`, `==surligné==` | Mise en forme |
| `- puce`, `1. numéro` | Listes |
| `- [ ] à faire`, `- [x] fait` | Cases à cocher, cliquables en lecture |
| `> citation` | Citation |
| `> [!note] Titre` | Encadré. `[!note]+` et `[!note]-` le rendent repliable |
| Tableaux Markdown | Tableaux |
| `![[image.png]]`, `![[image.png\|300]]` | Image du coffre, largeur en pixels en option |
| `![[Autre note]]`, `![[Note#Titre]]` | La note (ou une de ses sections) intégrée sur place |
| Bloc de code entre trois accents graves | Code coloré |
| `$x^2 + \alpha$` | Maths LaTeX simples, rendues en caractères Unicode |
| Bloc `mermaid` (`graph` ou `flowchart`) | Organigramme |
| `[^1]` et `[^1]: texte` | Notes de bas de page |
| `#mot` | Tag |

**Limites.** Les maths sont converties en caractères (lettres grecques, exposants, symboles) : pas de fractions empilées ni de matrices. Mermaid se limite aux organigrammes.

## Liens entre notes

- `[[Note]]` : lien vers une note. Tape `[[` et une liste propose les notes du coffre.
- `[[Note|texte affiché]]` : avec un alias.
- `[[Note#Titre]]` : vers une section.
- Un lien vers une note qui n'existe pas la crée au clic, dans le dossier des nouvelles notes.

### Le panneau de droite : `Structure` et `Links`

- **Structure** : les titres de la note. Un clic y va. Un titre peut être ajouté aux signets.
- **Backlinks** : les notes qui pointent vers celle-ci.
- **Unlinked mentions** : les notes qui écrivent le nom de celle-ci sans lien. `Link` change la première mention en `[[ ]]`. `Link all` les change toutes.
- **Related notes** : les notes qui parlent des mêmes choses (mots rares en commun) et ne sont pas encore liées. Calculé sur place, sans IA.

## Propriétés

Une note peut commencer par un en-tête YAML, entre deux lignes `---` :

```
---
type: plan
statut: en cours
échéance: 2026-12-04
tags: animation, layout
---
```

En lecture, ces propriétés s'affichent en haut. Double-clic pour modifier. Clic droit sur un nom : `Rename across the vault…` ou `Remove from all notes`.

## Bases : des tableaux tirés des notes

Un bloc `base` liste les notes du coffre selon leurs propriétés.

````
```base
from: projets/
where: statut = en cours
columns: file, statut, échéance
sort: échéance desc
group: statut
view: table
limit: 50
```
````

| Ligne | Sens |
|---|---|
| `from` | Un dossier, ou `#tag`. Plusieurs séparés par des virgules |
| `where` | Une condition par ligne : `=`, `!=`, `>`, `<`, `>=`, `<=`, `contient` |
| `columns` | Les colonnes. `file` est le nom de la note |
| `sort` | La propriété, puis `desc` pour l'ordre inverse |
| `group` | Regrouper par une propriété |
| `view` | `table`, `cartes`, `liste`, ou `kanban` |
| `order` | En kanban : l'ordre des colonnes, même vides |
| `limit` | Nombre maximum de notes (500 par défaut) |

En kanban, glisser une carte d'une colonne à l'autre change la propriété dans la note.

Un bloc `tasks` réunit les cases à cocher de tout le coffre.

### Suivi de production

`File › New › Production tracker` crée un dossier `Production/` tout prêt : une note `Suivi.md` avec un kanban des plans et un tableau des éléments, plus quelques fiches d'exemple (`Plans/SH010.md`, `Éléments/Léa.md`…). Les statuts vont de « à faire » à « validé ». C'est un point de départ : tout est fait de notes et de blocs `base`, modifiables.

## Tâches

L'outil `Tasks` réunit toutes les cases `- [ ]` des notes, groupées par note.

- Cocher ici coche dans la note.
- Un clic sur le texte ouvre la note.
- Le champ du haut ajoute une tâche à `tâches.md` (`Entrée`).
- `@2026-12-01` dans une tâche lui donne une échéance. Un filtre `Due dates` les trie.
- Filtres : `To do`, `All`, et un champ de texte.

## Tags

`#mot` n'importe où dans une note. La page de recherche affiche le nuage des tags. Un clic sur un tag liste ses notes.

## Cerveau

L'outil `Brain` (`Ctrl+G`) dessine le graphe du coffre : une bulle par note, un trait par lien.

| Option | Effet |
|---|---|
| `Folders` | Montre aussi les dossiers |
| `All files` | Tous les fichiers, pas seulement les notes |
| `Colors by tag` | Chaque note prend la couleur de son tag le plus courant (les six tags les plus portés) |
| `Local graph` | La dernière note ouverte et ses voisines jusqu'à deux liens |
| `Recenter` | Recadre la vue |
| `Tidy up with AI` | L'IA propose un rangement du dossier affiché. Rien ne bouge avant `Apply` |

Molette : zoom. Glisser : déplacer la vue ou une bulle. Survol : les voisines. Clic : ouvrir. Clic sur un dossier : entrer dedans, avec un fil d'Ariane pour remonter.

## Recherche

`Ctrl+Maj+F` cherche dans tout le coffre, dossiers liés compris : notes, code, texte, noms de fichiers. Majuscules et accents sont ignorés. Il faut au moins 2 caractères. Un clic ouvre le fichier à la bonne ligne.

`Ctrl+Maj+H` : remplacer dans tous les fichiers. Coche les fichiers à inclure, puis `Replace all in checked files`.

## Modèles de notes

Les modèles sont des notes du dossier `modèles/`. Modifie-les ou ajoutes-en. Trois mots sont remplacés à la création : `{{titre}}`, `{{date}}`, `{{heure}}`.

## Orthographe

`Settings › Editor › Spell checker for notes (French and English)`. Les mots inconnus sont soulignés. Clic droit : une correction, ou l'ajout au dictionnaire du coffre (`.app/dictionnaire.txt`).

## Exporter une note

Bouton **Export the note** :

- `HTML page (images included)` : une page autonome ;
- PDF simple : titres, paragraphes, listes, citations.

**Limite.** Le PDF ne reprend pas les tableaux ni les images.

## Scénario (Fountain)

Un fichier `.fountain` s'écrit comme du texte et se lit en mise en page de scénario (`Ctrl+E`). Aryadne estime le minutage, liste les personnages et les séquences, peut créer une note par personnage et par décor (dépouillement), et exporte en PDF (Courier 12, page US Letter).

## Autres outils du texte

- **Minuteur** de concentration dans la barre du bas : 25 min, 50 min, pause de 5 min (aussi par la palette).
- **IA sur le fichier** : résumer, modifier la sélection (`Ctrl+Maj+I`), relire, remplir les propriétés. Voir [Assistant IA](assistant-ia.md).
