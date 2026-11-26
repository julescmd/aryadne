---
titre: Relier les notes
niveau: débutant
duree: 15 min
prerequis: 02-notes/01-premiere-note
objectifs: Créer un lien vers une note ; Créer une note depuis un lien ; Lire les liens entrants ; Intégrer une note ou une image dans une autre
---

## Ce que tu vas réussir à faire

Tu vas relier la note d'un court métrage à la fiche de son personnage. Tu verras ensuite, depuis la fiche, quelles notes parlent d'elle.

Des notes reliées forment un réseau. C'est ce qui fait la force du coffre : tu retrouves une idée par ce qui l'entoure.

```essayer
fichier: fichiers/court-metrage.md
```

```etape
Clique **Try it yourself**. La note du court métrage s'ouvre.
verifier: fichier.ouvert = md
```

## Écrire un lien

Un lien s'écrit entre doubles crochets : `[[Nom de la note]]`.

```etape
Sous le titre « Personnages », va à la fin de la phrase. Tape `[[`. Une liste des notes du coffre s'affiche.
```

```etape
Continue en tapant `Anselme]]`. Tu as écrit `[[Anselme]]`.
verifier: texte.contient = [[Anselme]]
```

Cette note n'existe pas encore. Ce n'est pas un problème.

## Créer la note depuis le lien

```etape
Passe en lecture avec `Ctrl+E`. Clique le lien **Anselme**. La note est créée et s'ouvre.
verifier: fichier.ouvert = md
```

```astuce
Une note créée depuis un lien va dans le dossier des nouvelles notes. Tu le choisis dans les réglages (`Ctrl+,`), onglet **Editor**, réglage **New notes folder**.
```

```etape
Écris deux lignes dans la fiche d'Anselme : son âge, son caractère. Enregistre avec `Ctrl+S`.
verifier: fichier.enregistre
```

## Les liens entrants

```etape
Dans le panneau de droite, clique l'onglet **Links**.
```

L'onglet montre trois listes.

| Liste | Ce qu'elle contient |
|---|---|
| **Backlinks** | Les notes qui pointent vers celle-ci |
| **Unlinked mentions** | Les notes qui écrivent son nom sans faire de lien |
| **Related notes** | Les notes qui parlent des mêmes choses et ne sont pas encore liées |

La note du court métrage apparaît sous **Backlinks**. Tu n'as rien eu à faire dans la fiche.

```etape
Clique la note du court métrage dans **Backlinks**. Tu y reviens.
```

## Les mentions non liées

La note du court métrage écrit aussi « Anselme » sans lien, dans le texte et dans les tâches.

```etape
Rouvre la fiche Anselme. Dans **Links**, regarde **Unlinked mentions**. Clique **Link** : la première mention devient un lien.
```

**Link all** change toutes les mentions d'un coup.

## Les variantes d'un lien

| Tu écris | Effet |
|---|---|
| `[[Anselme]]` | Un lien vers la note |
| `[[Anselme\|le gardien]]` | Le lien affiche « le gardien » |
| `[[Court métrage#Décors]]` | Un lien vers un titre précis de la note |

```etape
Dans la fiche Anselme, écris une phrase avec un lien qui affiche un autre texte : `[[court-metrage|son film]]`.
verifier: texte.contient = son film
```

## Intégrer au lieu de lier

Un point d'exclamation devant le lien intègre le contenu sur place.

| Tu écris | Effet en lecture |
|---|---|
| `![[image.png]]` | L'image du coffre |
| `![[image.png\|300]]` | L'image, large de 300 pixels |
| `![[Anselme]]` | Toute la note Anselme, affichée ici |
| `![[Anselme#Caractère]]` | Seulement cette section |

```etape
Dans la note du court métrage, ajoute une ligne `![[Anselme]]` sous « Personnages ». Passe en lecture : la fiche s'affiche dans la note.
verifier: texte.contient = ![[Anselme]]
```

```attention
Un lien cherche la note par son nom. Si tu renommes la fiche Anselme, pense à corriger les liens qui la visent : ils ne se corrigent pas tout seuls. `Ctrl+Maj+H` remplace un texte dans tous les fichiers.
```

```quiz
Tu cliques un lien vers une note qui n'existe pas. Que se passe-t-il ?
- [ ] Une erreur s'affiche.
- [x] La note est créée et s'ouvre.
- [ ] Rien.
explication: Le lien crée la note au clic, dans le dossier des nouvelles notes.
```

```quiz
Où vois-tu les notes qui pointent vers la note ouverte ?
- [ ] Dans la barre du bas.
- [x] Dans l'onglet Links du panneau de droite, sous Backlinks.
- [ ] Dans le menu Edit.
explication: Backlinks liste les liens entrants. Tu n'as rien à écrire dans la note visée.
```

```defi
Crée une note « Phare » depuis un lien dans la section Décors. Fais en sorte qu'elle pointe à son tour vers Anselme.
indice: Écris `[[Phare]]` dans Décors, clique le lien en lecture, puis écris `[[Anselme]]` dans la nouvelle note.
```

## Récapitulatif

- `[[Nom]]` fait un lien. Un clic en lecture ouvre la note, ou la crée.
- L'onglet **Links** montre les liens entrants et les mentions à relier.
- `![[Nom]]` intègre une note ou une image sur place.

Leçon suivante : [Organiser : dossiers, propriétés, tags, modèles](03-organiser.md)
