# Aryadne

> **Projet personnel.** Aryadne est un outil que je développe pour mon propre usage. Il n'est pas destiné à être distribué au grand public : pas de support, pas de garantie, et tout peut changer ou casser d'une version à l'autre.

Le fil entre tes idées. Notes, code, références et moodboards dans un dossier qui t'appartient.

100 % Rust ([egui](https://github.com/emilk/egui)). Un seul exécutable portable pour Windows et Linux : **aucune installation, aucun droit admin**. Look éditeur de code (JetBrains Mono, angles droits), mode clair par défaut, mode sombre en un clic.

## Fonctionnalités (v0.12)

**3D**
- Visionneuse intégrée pour OBJ, FBX (binaire), glTF / GLB et STL. Rendu par pixel avec profondeur (comme les pixols de ZBrush) : pas de trous ni de mouchetures, calculé sur tous les cœurs, et recalculé seulement quand la vue change.
- Éclairage façon Substance : environnements HDRI (studio, studio doux, extérieur, coucher de soleil, nuit, neutre) ou tes propres fichiers .hdr / .exr (ceux du coffre sont listés), rotation (H + glisser), intensité, exposition, HDRI en fond net ou flou, plus une lumière directionnelle réglable (L + glisser, couleur, intensité).
- Matériaux argile, gris, blanc, chrome, normales ou couleurs du fichier ; ombrage lisse.
- Fil de fer par-dessus le modèle (opacité, noir / blanc / accent, ou fil de fer seul), sans les diagonales des quads.
- Subdivisions (lissage de Loop, jusqu'à 2,5 millions de triangles).
- Rotation automatique (platine), capture PNG 2400 px et turntable en GIF animé.
- Menus « affichage », « éclairage » et « vue » pour garder la barre propre.
- Navigation façon Blender : glisser pour tourner, Maj ou clic droit pour déplacer, Ctrl ou molette pour zoomer, 1 / 3 / 7 pour les vues (Maj : opposée), 5 pour l'orthographique, Origine ou « . » pour cadrer. Repère des axes.
- Les modèles très lourds (millions de triangles) sont simplifiés pour l'affichage en gardant leur forme.
- Miniatures 3D dans les refs et sur les boards.
- Miniatures des fichiers `.blend` (aperçu enregistré par Blender, formats Blender 2.x à 5, fichiers compressés), aussi dans les refs.
- « ouvrir dans Blender » / « ouvrir avec l'app par défaut » (clic droit ou bouton de l'éditeur).

**Vidéo** (MP4, MOV, WebM, MKV, AVI)
- Lecture image par image (← →, Espace), cadence réglable (12, 24, 30 i/s ou libre).
- Lecture « en 1s / 2s / 4s / 8s » comme en animation : chaque image tenue N fois.
- Pelures d'oignon (image précédente en rouge, suivante en vert).
- Dessin par-dessus, image par image (couleur, épaisseur, Ctrl+Z), repères sur la ligne de temps, gardé dans `.app/annotations/`.
- Export en séquence PNG / JPG (cadence, 1 image sur N, hauteur, dessins incrustés) ou en MP4 retimé (ex. 24 i/s en 2s).
- Miniatures vidéo dans les refs et les boards.
- ffmpeg : utilisé s'il est installé, sinon installé par Aryadne en un clic, sans admin.

**Documents**
- PowerPoint affiché comme dans PowerPoint : formes, images, textes, couleurs du thème, fond et décor du masque, chacun à sa place. Double-clic sur un texte pour le modifier, glisser une forme pour la déplacer, puis « enregistrer » réécrit le .pptx (le reste du fichier n'est pas touché).
- Aperçu des Excel (feuilles et cellules), Word, et LibreOffice (.odp, .ods, .odt), sans Office. Miniatures dans les refs et les boards.

**Recherche** (`Ctrl+Maj+F`)
- Dans tout le coffre, dossiers liés compris : notes, code, texte et noms de fichiers. Majuscules et accents ignorés, passages surlignés.

**Travailler à côté de Blender**
- « épingler » : Aryadne reste au-dessus des autres fenêtres.
- « compact » (`Ctrl+Maj+M`) : masque les panneaux, idéal pour garder un board de refs par-dessus Blender.

**Fichiers liés : rien n'est copié**
- Le coffre est une vue. Lie des dossiers ou des fichiers de ton disque (réglages → fichiers, ou glisse-les dans la fenêtre) : ils apparaissent dans l'explorateur comme n'importe quel dossier ou fichier du coffre (cerveau, recherche, refs, boards, assistant), et restent à leur place sur le disque. Survole-les pour voir leur emplacement d'origine.
- Clic droit : « déplacer dans le coffre » ou « copier dans le coffre » seulement si tu le décides. « Retirer le lien » ne supprime jamais l'original.
- Liste des liens dans `.app/links.json` du coffre.

**Export et transfert**
- Clic droit → « exporter en zip » (fichier ou dossier), ou « exporter avec ses liens » : notes liées, images citées, contenu d'un board.
- « transfert » : envoie des fichiers à un autre ordinateur du même réseau, sans cloud ni limite de taille (fichiers 3D). L'ordi qui reçoit affiche un code à 4 chiffres ; les fichiers arrivent dans `reçus/`.

**Réglages**
- Thème, couleur d'accent, taille de l'interface, taille du texte de l'éditeur, numéros de ligne, retour à la ligne, délai de sauvegarde, dossier des notes, import (lier / déplacer / copier / demander), dossiers liés, IA, mises à jour, liste des raccourcis.

**Fichiers**
- Explorateur de tout le coffre : dossiers repliables, filtre, fichiers modifiés marqués d'un ●.
- Clic droit : nouveau fichier ou dossier, renommer / déplacer, copier le chemin, supprimer.
- Glisser un fichier sur un dossier de l'explorateur pour le déplacer.
- Glisser des fichiers depuis ton bureau dans la fenêtre pour les importer (dans `refs/` par défaut, ou dans le dossier sélectionné). Au choix : **déplacer** (aucun doublon) ou copier ; réglable dans « ia ». Un fichier déjà dans le coffre n'est jamais recopié.
- « Supprimer » envoie dans `.app/trash/` du coffre : rien n'est perdu par accident.

**Éditeur de code**
- Fermeture automatique des ( [ { " ' ` et des balises HTML (`<div>` → `</div>`, `</` complète la balise ouverte).
- Entrée garde l'indentation et ajoute un niveau après { [ ( : ou entre deux balises ; listes Markdown continuées.
- Suggestions pendant la frappe (mots du fichier, mots-clés du langage, balises, `[[notes]]`), Ctrl+Espace pour les forcer.
- Abréviations Emmet en HTML : `!`, `div.carte#id`, `ul>li*3` puis Tab.
- Tab / Maj+Tab sur plusieurs lignes, Ctrl+/ commenter, Ctrl+D dupliquer, Ctrl+Maj+K supprimer la ligne, Alt+↑↓ déplacer, parenthèse correspondante surlignée.
- Ctrl+F chercher, Ctrl+H remplacer (tout ou un par un), F3 suivant.
- « aperçu live » pour le HTML / SVG : la page s'ouvre dans ton navigateur et se recharge à chaque enregistrement.

**Nodes : matériaux et compositing**
- Éditeur de nodes (glisser une sortie sur une entrée, Maj+A pour ajouter, zoom, déplacement), aperçu en direct à gauche, enregistré en JSON dans le coffre.
- Matériaux (`.mat.json`) : bruit, Voronoï, damier, dégradés, rayures, Fresnel, géométrie (normale, position, pente), rampes, mélanges (multiplier, écran, incrustation…), teinte/saturation, niveaux, maths… vers couleur, rugosité, métal, émission. Appliqués aux modèles dans la visionneuse 3D (menu affichage), mis à jour en direct.
- Compositing (`.comp.json`) : images du coffre, flou, netteté, halo, vignette, grain, aberration chromatique, pixeliser, postériser, contours, niveaux, couleurs… Export PNG en pleine résolution.
- Node **shader perso** : un petit langage d'expressions (`r = mix(ar, br, fbm(x*6, y*6, 0))`), avec variables, fonctions de bruit et conditions, exécuté pixel par pixel.

**Organisation**
- Palette de commandes (`Ctrl+Maj+P`) : toutes les actions au clavier.
- Barre du haut épurée : menu du coffre (nouvelle note, modèles, import, transfert, sauvegarde), menu « fenêtre » (thème, épingler, compact), réglages.
- Vue **tâches** : toutes les cases `- [ ]` des notes, cochables, filtrables, et ajout rapide.
- **Note du jour** (`Ctrl+J`) et **modèles** de notes (fiche personnage, plan de shot, réunion, projet, critique) dans `modèles/`, avec `{{titre}}` et `{{date}}`.
- **Tags** `#mot` : nuage de tags dans la recherche.
- **Historique** : chaque fichier texte garde ses 40 dernières versions, consultables et restaurables.
- **Sauvegarde** automatique du coffre en zip, une fois par jour (7 gardées).
- **Minuteur** de concentration dans la barre du bas.
- Images depuis une URL (menu du coffre, ou Ctrl+V d'un lien dans les refs).

**Linux et Windows**
- Aryadne s'ajoute tout seul au menu des applications (Linux) ou au menu Démarrer (Windows), avec son icône.
- Sous Wayland, Aryadne passe par XWayland pour que le glisser-déposer de fichiers marche (réglable). Boutons « importer des fichiers… » et « lier un dossier… » partout.

**Vidéo**
- Pelures d'oignon réglables : jusqu'à 6 images avant et après, opacité dégressive, teinte rouge/vert ou naturelle, dessins inclus.
- Repères : règle des tiers, centre, zones de sécurité, caches de format (2.39, 1.85, 16:9, 9:16…).

**Code**
- « ▶ exécuter » (F5) pour les scripts Python, JS, Bash, Lua, Ruby, PowerShell : sortie en direct sous l'éditeur, arrêter, relancer.

**Coffre**
- Coffres récents, statistiques (taille par type, fichiers les plus lourds, nombre de mots), liens `[[Note#Titre]]`, échéances `@AAAA-MM-JJ` dans les tâches.

**Assistant IA partout** (`Ctrl+L`)
- Pièces jointes : fichiers, images, dossiers entiers, fichier ouvert, image du presse-papiers (ou glisser dans le chat). Les images vont aux modèles qui voient (Qwen 3.8, Gemma 3…).
- Choix du modèle directement dans le chat, bouton « arrêter » pendant la réponse.
- L'IA peut poser une question en retour, avec des réponses toutes prêtes en boutons.
- Gestion des modèles (réglages → IA) : liste des modèles installés avec leur taille, utiliser, supprimer, et télécharger n'importe quel modèle d'Ollama ou de Hugging Face (`hf.co/…`), par exemple `qwen3.8:27b`.
- Panneau à côté de toutes les vues : il voit le fichier ouvert (ou ta sélection), ou le board courant. Boutons rapides (expliquer, corriger, améliorer, commenter...) et blocs de code insérables au curseur.

**Cerveau**
- Clic sur un dossier pour entrer dedans, fil d'Ariane pour remonter.
- « ranger avec l'IA » : l'IA propose un rangement du dossier affiché, tu coches ce que tu veux appliquer.

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
- Canevas infini façon PureRef, enregistré dans `boards/*.board.json`. Les images gardent leurs proportions.
- Outils : sélection (V), dessin à main levée (B, Maj = ligne droite), gomme (E), texte (T). Couleur et épaisseur du trait.
- Sélection multiple au lasso ou Maj+clic, 4 poignées de redimensionnement (Maj = proportions libres), annuler / rétablir (`Ctrl+Z` / `Ctrl+Y`), `Ctrl+A`, `Ctrl+D`, `Suppr`.
- Coller une image du presse-papier (`Ctrl+V`, enregistrée dans `refs/`).
- Clic droit : premier plan / arrière-plan, retourner l'image, taille d'origine, taille du texte.
- Vue : molette pour zoomer, clic milieu / clic droit / Espace + glisser pour se déplacer.
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
  settings.rs  fenêtre de réglages
  transfer.rs  export zip et transfert réseau
  model3d.rs   visionneuse 3D (OBJ, glTF, STL)
  blend.rs     miniatures des fichiers .blend
  search.rs    recherche plein texte
  fbx.rs       lecteur FBX binaire
  office.rs    aperçu PowerPoint, Excel, Word, LibreOffice
  video.rs     lecteur vidéo, timing, dessin, export de séquences
  ffmpeg.rs    installation de ffmpeg sans admin
assets/        logo, icônes, polices
  vault.rs     fichiers du coffre, liens, graphe (+ tests)
  preview.rs   mode lecture Markdown
  theme.rs     palettes claire/sombre, police, angles droits
assets/fonts/  JetBrains Mono (licence OFL)
```

## Prochaines étapes

Voir la liste d'idées dans les issues / la discussion.
