---
titre: L'assistant IA
niveau: intermédiaire
duree: 15 min
prerequis: 01-demarrer/04-palette-et-raccourcis
objectifs: Brancher un modèle d'IA, local ou en ligne ; Poser une question sur le fichier ouvert ; Accepter un fichier proposé par l'assistant ; Faire modifier un graphe de nodes ; Garder la main sur chaque changement
---

## Ce que tu vas réussir à faire

Tu vas brancher un modèle, demander à l'assistant de compléter une note, puis lui faire modifier une simulation de tissu en une phrase. À chaque fois, c'est toi qui valides.

L'IA est facultative. Sans modèle branché, tout le reste d'Aryadne fonctionne.

## La règle

L'IA propose, tu acceptes ou non. Aucune proposition ne modifie un fichier sans un clic de ta part. Une seule exception, que tu actives toi-même et qui reste annulable. Tu la verras plus bas.

## Brancher un modèle

Deux chemins.

### Local, gratuit, privé : Ollama

Ollama fait tourner des modèles sur ton ordinateur. Rien ne sort de chez toi.

```etape
Ouvre les réglages avec `Ctrl+,`. Va dans l'onglet **AI**.
```

1. Clique **Install ollama (no admin)**. Ollama s'installe dans ton dossier utilisateur.
2. Clique **Download a model** et tape un nom de modèle, par exemple `llama3.2`.
3. Dans **Installed models**, clique **Use** sur le modèle.

Il faut en mémoire à peu près la taille du modèle, plus 2 à 4 Go. Un avertissement **⚠ slow** s'affiche quand un modèle est trop gros pour ta machine.

### En ligne : un service

Dans le même onglet, **AI providers** accepte une clé API : Claude, ChatGPT, Gemini, Mistral et d'autres. Colle la clé. Les modèles du service apparaissent dans le menu du chat.

```attention
Avec un service en ligne, ce que tu envoies au modèle quitte ton ordinateur : le fichier ouvert, ta sélection, les passages de tes notes. Ne l'utilise pas sur un contenu que tu dois garder privé.
```

```etape
Branche un modèle, par l'un ou l'autre chemin. Ferme les réglages.
```

## L'assistant à côté

```essayer
fichier: fichiers/court-metrage.md
```

```etape
Clique **Essaie par toi-même**. La note du court métrage s'ouvre.
verifier: fichier.ouvert = md
```

```etape
Appuie sur `Ctrl+L`, ou clique le bouton **Assistant** en haut à droite. Le panneau de l'assistant s'ouvre à côté de la note.
```

L'assistant voit le fichier ouvert, ta sélection, ou le board courant.

```etape
Clique le bouton rapide **Summarize**. L'assistant résume la note.
```

```etape
Tape une question dans le champ : `Quels décors faut-il dessiner ?` Envoie avec `Entrée`.
```

| Geste | Effet |
|---|---|
| `Entrée` | Envoie |
| `Maj+Entrée` | Passe à la ligne |
| **Clear** | Vide la conversation |
| **Insert at cursor** | Sous un bloc de code proposé : l'insère dans ton fichier |
| **Copy** | Copie le bloc |

## L'assistant propose un fichier

L'assistant peut proposer un fichier entier, modifié ou nouveau. Il écrit pour cela un bloc spécial, qui commence par `fichier:` suivi d'un chemin. Tu n'as pas à l'écrire toi-même : demande simplement le changement.

```etape
Demande : `Ajoute à cette note une section Musique avec trois pistes d'ambiance à chercher.`
```

Sous la réponse, une ligne montre le chemin du fichier et un bouton.

| Bouton | Quand |
|---|---|
| **remplacer le fichier** | Le fichier existe déjà : son contenu sera remplacé |
| **créer le fichier** | Le fichier n'existe pas encore |

Ces deux libellés restent en français, même quand l'interface est en anglais.

```etape
Lis la proposition. Si elle te convient, clique **remplacer le fichier**.
verifier: texte.contient = Musique
```

L'ancienne version reste dans l'historique du fichier. **File › Version history** la retrouve.

```astuce
L'assistant réécrit le fichier entier. Sur une longue note, relis tout, pas seulement la partie demandée. Pour changer un seul passage, sélectionne-le et utilise `Ctrl+Maj+I` : tu vois la différence avant d'accepter.
```

## L'IA dans le fichier

Le menu **AI on this file**, dans la barre du fichier, agit sans passer par la conversation.

| Commande | Effet |
|---|---|
| **Summarize this file** | Un résumé, à copier ou à enregistrer en note |
| **Edit selection…** (`Ctrl+Maj+I`) | Une consigne sur la sélection : « plus court », « en anglais » |
| **Proofread (mistakes, repetitions)** | Des corrections une par une, sans réécrire ton texte |
| **Fill in properties** | Tags, type et résumé ajoutés à l'en-tête de la note |
| **Explain the error** | Dans le code : explique l'erreur sous le curseur |

## L'assistant modifie un graphe de nodes

Devant un graphe de nodes, l'assistant ne se contente pas d'expliquer. Il fait le changement. Cela vaut pour la géométrie et la simulation, les matériaux et le compositing.

```essayer
exemple: tissu sur une boule
```

```etape
Clique ce deuxième **Essaie par toi-même**. Un graphe de simulation de tissu s'ouvre dans l'étape 3D.
verifier: fichier.ouvert = arygeo
```

```etape
Ouvre l'assistant avec le bouton **Assistant**, en haut à droite, ou avec `Ctrl+L`. Sous le titre du panneau, une ligne commence par **Sees and edits: the graph**.
```

Il voit le graphe tel que tu l'as à l'écran : chaque node, ses réglages, ses liens.

```etape
Clique le bouton rapide **Explain**. Lis ce que fait chaque partie du graphe.
```

```etape
Demande : `Ajoute du vent.`
```

Sous la réponse, une carte **Apply to the graph** résume les changements : les nodes posés, les réglages, le nombre de liens. **All the details** déplie la liste complète.

```etape
Lis la carte. Clique **Apply**.
```

Tout est appliqué d'un coup. C'est une seule étape d'annulation.

```etape
La souris sur le graphe, appuie sur `Ctrl+Z`. Tout le changement est annulé. `Ctrl+Maj+Z` le refait.
```

### Quand la proposition ne tient pas

Chaque changement est vérifié avant d'être proposé : le node existe, la prise existe, la valeur a le bon type, le lien ne fait pas de boucle.

S'il y a une erreur, la carte devient **Change refused**. Elle liste ce qui ne va pas. Rien n'est appliqué, pas même les parties justes.

```etape
Si la carte affiche **Change refused**, clique **Ask for a fix**. Les erreurs repartent à l'assistant, qui corrige sa proposition.
```

```attention
Devant un graphe, la demande envoyée au modèle est longue : elle contient le catalogue des nodes. Un petit modèle local répond lentement et se trompe plus souvent. Les modèles en ligne s'en sortent bien mieux.
```

### L'application automatique

Le bouton éclair, en haut du panneau, n'apparaît que devant un graphe. Activé, les changements valides sont appliqués dès que la réponse arrive, sans passer par la carte. `Ctrl+Z` les annule de la même façon. Il est désactivé par défaut. C'est la seule exception à la règle du début.

### Ce qu'il sait faire sur un graphe

Ajouter des nodes, les relier, les délier, régler une valeur, renommer, supprimer, grouper, poser une zone de simulation, tracer un cadre. Dans un matériau : ajouter, régler, déplacer et supprimer des calques.

D'autres boutons rapides : **Frame** entoure les parties du graphe de cadres titrés. **Improve** propose des améliorations.

Ses limites :

- il modifie le niveau affiché. Pour l'intérieur d'un groupe, entre d'abord dans le groupe ;
- il ne dessine pas de courbes ;
- il ne choisit pas un fichier à ta place : donne-lui le chemin.

## Le chat avec le coffre

La page **Chat** pose une question à tout ton coffre. Aryadne cherche les passages utiles dans tes notes et ton code, les envoie au modèle, et cite ses sources sous la réponse.

- `@` cite un fichier précis.
- **Attach** joint des fichiers, un dossier ou une image.
- **Save as note** garde la conversation en note.

```attention
La page **Chat** ne modifie pas les graphes. C'est l'assistant à côté, `Ctrl+L`, qui le fait. Et la recherche du chat repose sur les mots, pas sur le sens : pose ta question avec les mots de tes notes.
```

```quiz
L'assistant propose un fichier modifié. Quand le fichier change-t-il vraiment ?
- [ ] Dès que la réponse arrive.
- [x] Quand tu cliques le bouton sous la réponse.
- [ ] À la fermeture d'Aryadne.
explication: L'IA propose, tu acceptes ou non. L'ancienne version reste dans l'historique du fichier.
```

```quiz
La carte affiche Change refused. Qu'est-ce qui a été appliqué au graphe ?
- [x] Rien.
- [ ] Les changements justes seulement.
- [ ] Tout, erreurs comprises.
explication: Une proposition avec une erreur est refusée en entier. Ask for a fix demande une correction.
```

```quiz
Quel avantage a un modèle local avec Ollama ?
- [x] Tes fichiers ne quittent pas ton ordinateur.
- [ ] Il est toujours plus juste qu'un modèle en ligne.
- [ ] Il n'utilise pas de mémoire.
explication: Local veut dire privé et hors ligne. En contrepartie, un petit modèle local se trompe plus souvent.
```

```defi
Fais transformer par l'assistant le tissu de l'exemple en drapeau qui flotte au vent, puis annule tout d'un seul `Ctrl+Z`.
indice: Une phrase suffit : « fais un drapeau qui flotte au vent ». Lis la carte avant d'appliquer. Si elle est refusée, **Ask for a fix**.
```

## Récapitulatif

- **Settings**, onglet **AI** : Ollama en local, ou une clé pour un service en ligne.
- `Ctrl+L` ouvre l'assistant. Il propose un fichier, tu cliques pour l'accepter.
- Devant un graphe, la carte **Apply to the graph** applique tout d'un coup, annulable d'un `Ctrl+Z`.

Parcours suivant : [Les bases de la 3D](../07-3d-bases/index.md)
