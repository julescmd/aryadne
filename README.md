# Aryadne

Le fil entre tes idées. Notes, code, références et moodboards dans un dossier qui t'appartient.

100 % Rust ([egui](https://github.com/emilk/egui)). Un seul exécutable portable pour Windows et Linux : **aucune installation, aucun droit admin**. Look éditeur de code (JetBrains Mono, angles droits), mode clair par défaut, mode sombre en un clic.

## Fonctionnalités (v0.5)

**Fichiers**
- Explorateur de tout le coffre : dossiers repliables, filtre, fichiers modifiés marqués d'un ●.
- Clic droit : nouveau fichier ou dossier, renommer / déplacer, copier le chemin, supprimer.
- Glisser un fichier sur un dossier de l'explorateur pour le déplacer.
- Glisser des fichiers depuis ton bureau dans la fenêtre pour les importer (dans `refs/` par défaut, ou dans le dossier sélectionné). Au choix : **déplacer** (aucun doublon) ou copier ; réglable dans « ia ». Un fichier déjà dans le coffre n'est jamais recopié.
- « Supprimer » envoie dans `.app/trash/` du coffre : rien n'est perdu par accident.

**Éditeur**
- Onglets, sauvegarde automatique.
- Coloration syntaxique pour des dizaines de langages (Python, Rust, JS, JSON, C++, GLSL, YAML...), numéros de ligne.
- Notes Markdown : liens `[[Note]]` et `[[Note|alias]]`, mode lecture, liens entrants.
- Aperçu des images (png, jpg, webp, gif).

**Refs** (`Ctrl+3`)
- Galerie de toutes les images du coffre, miniatures en arrière-plan, tri récentes / nom / couleur, taille réglable.
- Formats : PNG, JPG, WebP, GIF, BMP, TIFF, TGA, EXR, HDR (EXR/HDR avec tone mapping), extensions en majuscules, images géantes.
- Palette de 5 couleurs extraite, clic pour copier le hex.

**Boards** (`Ctrl+4`)
- Canevas infini façon PureRef, enregistré dans `boards/*.board.json`.
- Glisser des images ou des notes depuis l'explorateur, les refs ou le bureau. Déplacer, redimensionner (poignée), `Suppr`, `Ctrl+D`, double-clic pour du texte.
- Moodboards automatiques : par mots-clés (nom, dossier, notes qui citent l'image) ou par couleurs proches d'une image.

**Chat** (`Ctrl+5`)
- Pose des questions sur ton coffre : Aryadne cherche les passages pertinents (notes, code, texte), les envoie au modèle et affiche la réponse en direct.
- Les notes `[[ ]]` et chemins de fichiers cités sont cliquables, les sources sont listées sous chaque réponse.
- Option : inclure le fichier ouvert. Conversation enregistrable en note.

**Cerveau** (`Ctrl+G` ou `Ctrl+2`)
- Graphe de tout le coffre : notes reliées par leurs liens, dossiers et racine en option, tous les fichiers en option.
- Molette pour zoomer, glisser pour déplacer la vue ou un nœud, survol pour voir les voisins, clic pour ouvrir.

**Résumé IA**
- Bouton « résumer » sur n'importe quel fichier texte ou de code. Le résumé peut être copié ou enregistré en note (liée au fichier source).
- Fonctionne avec toute API compatible OpenAI. Par défaut : [Ollama](https://ollama.com) en local (gratuit, privé).
- **IA locale en un clic, sans admin** : bouton « ia » en haut à droite, puis « installer ollama ». Aryadne télécharge Ollama depuis ses releases GitHub, l'installe dans le dossier utilisateur (`%LOCALAPPDATA%\aryadne\ollama` ou `~/.local/share/aryadne/ollama`), le démarre, et télécharge le modèle choisi. Ollama est lancé automatiquement au besoin et arrêté à la fermeture d'Aryadne. Un Ollama déjà installé ailleurs est détecté et réutilisé.
- Modèles conseillés : `llama3.2` (3B, ~2 Go), `qwen2.5:7b` (meilleur, ~4,7 Go), `qwen2.5:0.5b` (très léger).

| Raccourci | Action |
|---|---|
| `Ctrl+P` | ouvrir n'importe quel fichier (ou créer une note) |
| `Ctrl+N` | nouvelle note |
| `Ctrl+E` | édition / lecture (Markdown) |
| `Ctrl+1` à `Ctrl+5` | fichiers, cerveau, refs, boards, chat |
| `Ctrl+G` | cerveau |
| `Ctrl+S` | sauvegarder |
| `Ctrl+W` | fermer l'onglet |

## Lancer

- **Windows** : télécharge `aryadne-windows-x64.exe` (onglet Actions ou Releases) et double-clique. Tu peux le mettre n'importe où. L'exécutable n'est pas signé : si SmartScreen s'affiche, clique « Informations complémentaires » puis « Exécuter quand même » (aucun admin requis).
- **Linux** : `chmod +x aryadne-linux-x64 && ./aryadne-linux-x64`

Les réglages (dernier coffre, thème, IA) sont dans le dossier de config de l'utilisateur (`%APPDATA%\aryadne` ou `~/.config/aryadne`).

## Développer

Prérequis : [Rust](https://rustup.rs). Sur Linux (Ubuntu) :

```
sudo apt install libgtk-3-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev
```

```
cargo run             # lancer en dev
cargo test            # tests du coffre
cargo build --release # exécutable final dans target/release/
```

## Publier une version et mises à jour

`.github/workflows/build.yml` compile Windows et Linux **seulement quand tu pousses un tag `v*`** (ou à la main depuis l'onglet Actions), pour économiser les minutes GitHub Actions :

```
# 1. mets à jour `version` dans Cargo.toml (ex. 0.5.1), commit
git tag v0.5.1 && git push && git push --tags
```

La Release est créée avec les deux exécutables. Aryadne vérifie les nouvelles versions au démarrage (toutes les 6 h) ; le bouton de version en haut à droite permet d'installer et de redémarrer. Repo privé : il faut un jeton GitHub en lecture seule (Contents: read), à coller dans cette fenêtre. Repo public : rien à faire, et les Actions sont gratuites.

## Structure

```
src/
  main.rs      interface : explorateur, onglets, éditeur, fenêtres, raccourcis
  brain.rs     vue Cerveau (graphe à forces)
  ai.rs        résumé IA (API compatible OpenAI)
  ollama.rs    installation, démarrage et modèles Ollama, sans admin
  images.rs    chargement d'images, miniatures, palettes
  board.rs     boards (canevas infini)
  chat.rs      chat avec le coffre (recherche + streaming)
  updater.rs   mises à jour depuis l'application
  vault.rs     fichiers du coffre, liens, graphe (+ tests)
  preview.rs   mode lecture Markdown
  theme.rs     palettes claire/sombre, police, angles droits
assets/fonts/  JetBrains Mono (licence OFL)
```

## Prochaines étapes

Voir la liste d'idées dans les issues / la discussion.
