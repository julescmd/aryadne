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
- **Nœuds** : dans un nœud `Custom shader`, le bouton `AI` écrit le code du shader d'après une description.
- **Montage** : la transcription de la parole en sous-titres passe par whisper.cpp s'il est installé, sinon par l'API de transcription d'OpenAI si une clé est enregistrée. Voir [Montage](montage.md#sous-titres).

## Règle de conduite

L'IA propose, tu acceptes ou non. Aucune commande de l'IA ne modifie un fichier sans un clic de ta part.

## Limites

- La qualité dépend entièrement du modèle choisi. Un petit modèle local se trompe souvent.
- La recherche dans le coffre repose sur les mots, pas sur le sens : une question formulée avec d'autres mots que ceux de tes notes peut manquer le bon passage.
