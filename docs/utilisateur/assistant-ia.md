# Assistant IA

L'IA est facultative. Sans modèle branché, tout le reste d'Aryadne fonctionne.

Par défaut, Aryadne vise **Ollama**, un programme qui fait tourner des modèles sur ton ordinateur : gratuit, privé, hors ligne.

## Installer l'IA locale

1. Bouton `AI` en haut à droite, ou `Settings › AI`.
2. `Install ollama (no admin)`. Ollama est téléchargé depuis ses publications GitHub et installé dans ton dossier utilisateur.
3. `Download a model` : tape un nom, par exemple `llama3.2`, ou un modèle Hugging Face sous la forme `hf.co/utilisateur/modèle-GGUF`.

Il faut à peu près la taille du modèle en mémoire (RAM ou carte graphique), plus 2 à 4 Go. Un avertissement `⚠ slow` s'affiche quand un modèle est trop gros pour ta machine.

Ollama est lancé au besoin et arrêté à la fermeture d'Aryadne. Un Ollama déjà installé ailleurs est détecté et réutilisé.

La liste **Installed models** montre la taille de chaque modèle, avec `Use` et la suppression (`Frees disk space`).

## Brancher un service en ligne

`Settings › AI › AI providers` : Claude, ChatGPT, DeepSeek, Gemini, Mistral, Grok, Groq, OpenRouter, ou LM Studio en local. Colle la clé. Les modèles du service apparaissent dans le menu du chat.

Avec un service en ligne, le contenu envoyé au modèle quitte ton ordinateur.

## L'assistant à côté (`Ctrl+L`)

Un panneau qui reste ouvert à côté de n'importe quelle vue. Il voit le fichier ouvert, ta sélection, ou le board courant.

- Boutons rapides : `Summarize`, `To do`, et d'autres selon le fichier (expliquer, corriger, améliorer, commenter).
- `Entrée` envoie. `Maj+Entrée` passe à la ligne.
- Un bloc de code proposé a deux boutons : `Insert at cursor` et `Copy`.
- `Clear` vide la conversation.

L'assistant peut proposer un fichier modifié (note, board, code). Tu vois `Replace` ou `Create`. L'ancienne version reste dans l'historique du fichier. Il peut aussi poser une question en retour, avec des réponses toutes prêtes en boutons.

## Modifier un graphe de nœuds

Quand le document ouvert est un graphe de nœuds, l'assistant ne se contente pas d'expliquer : il fait le changement. Cela vaut pour la [géométrie et la simulation](noeuds-geometrie.md) (`.arygeo`), les [matériaux et leurs calques](3d-atelier.md) (`.arymat`) et le [compositing](noeuds-compositing.md) (`.arycomp`).

La ligne sous le titre du panneau le dit : `Sees and edits: the graph …`.

### Ce qu'il voit

- Le graphe tel que tu l'as à l'écran : chaque nœud, ses réglages, ses liens, les groupes, les cadres. Dans un groupe ou dans un calque, il voit ce niveau-là.
- Le catalogue des nœuds de ce genre de graphe, avec leurs prises et leurs réglages.
- Une notice : comment fonctionne une zone de simulation, le nœud `Solver` et son choix « tissu (carte graphique) », les calques de l'Atelier.

### Ce qu'il fait

Demande en une phrase : « fais un drapeau qui flotte au vent, en simulation de tissu », « ajoute du vent », « mets de la rouille dans les creux », « ajoute un halo et du grain avant la sortie ».

Sous sa réponse, une carte **Apply to the graph** résume les changements : les nœuds posés, les réglages, le nombre de liens. `All the details` déplie la liste complète.

- `Apply` applique tout d'un coup. C'est une seule étape : `Ctrl+Z` dans le graphe l'annule en entier, `Ctrl+Maj+Z` la refait.
- Les nœuds posés sont rangés sans chevauchement, et la vue se recadre sur le graphe.
- Rien n'est modifié avant ton clic.

Il peut ajouter des nœuds, les relier, les délier, régler une entrée ou un réglage, renommer, supprimer, grouper, défaire un groupe, poser une zone de simulation ou de répétition, tracer un cadre, régler la durée de lecture, ou repartir d'un graphe vide. Dans un matériau, il ajoute, règle, déplace et supprime des calques (couleur, préréglage ou matériau maître, masque, opacité, paramètres exposés) et modifie les nœuds d'un calque.

### Quand la proposition ne tient pas

Chaque changement est vérifié avant d'être proposé : le type de nœud existe dans ce genre de graphe, la prise et le réglage existent, la valeur a le bon type, le choix fait partie de la liste, le lien ne fait pas de boucle, le graphe obtenu se calcule encore.

S'il y a une erreur, la carte devient **Change refused** et liste ce qui ne va pas, opération par opération. Rien n'est appliqué, pas même les opérations justes. `Ask for a fix` renvoie les erreurs à l'assistant, qui corrige sa proposition.

La carte se vérifie contre le graphe ouvert. Si tu changes de document, elle indique d'ouvrir le graphe visé.

### Appliquer automatiquement

Le bouton éclair, en haut du panneau, n'apparaît que devant un graphe. Activé, les changements valides sont appliqués dès que la réponse arrive, sans passer par la carte. `Ctrl+Z` les annule de la même façon. Il est désactivé par défaut.

### Boutons rapides

Devant un graphe : `Explain`, `Frame` (entoure les parties du graphe de cadres titrés), `Improve`, et `Flag` dans un graphe de géométrie.

## Le chat avec le coffre

Page `Chat`. Tu poses une question sur ton coffre. Aryadne cherche les passages pertinents dans tes notes, ton code et tes textes, les envoie au modèle, et affiche la réponse en direct.

- Les notes `[[ ]]` et les chemins cités sont cliquables. Les sources sont listées sous chaque réponse.
- `@` cite un fichier précis.
- `＋ Attach` : `Files or images…`, `A folder…`, `Clipboard image`. Tu peux aussi glisser des fichiers dans la fenêtre. Les images vont aux modèles qui savent les lire.
- Sur un message : `Edit` (reprendre la question, la suite est remplacée), `Copy`, `Regenerate`.
- `■ Stop` arrête la réponse.
- `Thinking` : certains modèles réfléchissent avant de répondre. Plus juste, bien plus lent. Désactivé par défaut.
- `Conversations ▾` : elles s'enregistrent toutes seules. `Save as note` en fait une note.

## L'IA dans un fichier

Menu **AI on this file** de la barre du fichier :

| Commande | Effet |
|---|---|
| `Summarize this file` | Un résumé, à copier ou à enregistrer en note liée au fichier |
| `Edit selection…` (`Ctrl+Maj+I`) | Une consigne (« plus court », « en anglais »). Tu vois la différence avant d'accepter |
| `Proofread (mistakes, repetitions)` | Des corrections une par une, sans réécrire ton texte |
| `Fill in properties` | Tags, type et résumé déduits du texte, ajoutés à l'en-tête YAML |
| `Explain the error` | Dans le code, explique l'erreur sous le curseur |

## Ailleurs dans l'application

- **Cerveau** : `Tidy up with AI` propose un rangement d'un dossier. Tu coches ce que tu veux appliquer.
- **Nœuds** : l'assistant modifie le graphe ouvert, voir [Modifier un graphe de nœuds](#modifier-un-graphe-de-nœuds). Dans un nœud `Custom shader`, le bouton `AI` écrit le code du shader d'après une description.
- **Montage** : la transcription de la parole en sous-titres passe par whisper.cpp s'il est installé, sinon par l'API de transcription d'OpenAI si une clé est enregistrée. Voir [Montage](montage.md#sous-titres).

## Règle de conduite

L'IA propose, tu acceptes ou non. Aucune commande de l'IA ne modifie un fichier sans un clic de ta part. Seule exception, que tu actives toi-même : l'application automatique des changements de graphe, qui reste annulable.

## Limites

- La qualité dépend entièrement du modèle choisi. Un petit modèle local se trompe souvent.
- Devant un graphe, la demande envoyée au modèle est longue (le catalogue des nœuds). Un petit modèle local répond plus lentement et propose plus souvent des changements refusés. Les modèles en ligne s'en sortent bien mieux.
- L'assistant modifie le niveau affiché : pour l'intérieur d'un groupe, entre d'abord dans le groupe. Un calque lié à un matériau maître se modifie dans le fichier du maître.
- Il ne dessine pas de courbes (`RGB curves`) et ne choisit pas de fichier à ta place : il écrit le chemin que tu lui donnes.
- Le panneau `Chat` (la page) ne modifie pas les graphes. C'est l'assistant à côté (`Ctrl+L`) qui le fait.
- La recherche dans le coffre repose sur les mots, pas sur le sens : une question formulée avec d'autres mots que ceux de tes notes peut manquer le bon passage.
