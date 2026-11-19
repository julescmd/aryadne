# Aryadne

Le fil entre tes idées. Notes, références et moodboards dans un dossier qui t'appartient.

Interface plate en mode clair par défaut (mode sombre avec le bouton en haut à droite), police Inter intégrée.

100 % Rust ([egui](https://github.com/emilk/egui)), sans navigateur intégré. Un seul exécutable portable pour Windows et Linux : **aucune installation, aucun droit admin**.

## Ce qui marche (v0.1)

- Ouvrir ou créer un coffre (n'importe quel dossier). La structure `notes/ refs/ boards/ .app/` est créée automatiquement.
- Notes en Markdown, sauvegardées 500 ms après la dernière frappe et à la fermeture.
- Liens `[[Note]]` et `[[Note|alias]]`. En mode lecture, un lien bleu ouvre la note et un lien rouge la crée.
- Liens entrants dans le panneau de droite.
- Le dernier coffre ouvert est mémorisé dans le dossier de config de l'utilisateur.

| Raccourci | Action |
|---|---|
| `Ctrl+P` | ouvrir ou créer une note |
| `Ctrl+N` | nouvelle note |
| `Ctrl+E` | basculer édition / lecture |
| `Ctrl+S` | sauvegarder tout de suite |

## Lancer

- **Windows** : télécharge `aryadne-windows-x64.exe` (onglet Actions ou Releases) et double-clique. Tu peux le mettre n'importe où, même sur une clé USB.
- **Linux** : `chmod +x aryadne-linux-x64 && ./aryadne-linux-x64`

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

## Builds automatiques

`.github/workflows/build.yml` compile Windows et Linux à chaque push sur `main` (fichiers dans Actions, section Artifacts). Pousse un tag `v0.1.0` pour créer une Release GitHub avec les deux exécutables :

```
git tag v0.1.0 && git push --tags
```

## Structure

```
src/
  main.rs      interface : panneaux, éditeur, sélecteur Ctrl+P
  theme.rs     le look : palettes claire et sombre, police Inter, angles droits
assets/fonts/ Inter (licence OFL)
  preview.rs   mode lecture : Markdown + liens [[ ]]
  vault.rs     lecture/écriture du coffre, backlinks (+ test)
```

Pour changer les couleurs, tout est en haut de `src/theme.rs`.

## Prochaines étapes

1. **refs** : glisser-déposer d'images dans `refs/`, tags, palette de couleurs
2. **boards** : canevas infini façon PureRef, sauvegardé dans `boards/`
3. Index SQLite dans `.app/index.db`
4. Recherche par texte avec CLIP (ONNX) et moodboards automatiques
5. Thèmes et plugins
