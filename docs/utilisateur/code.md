# Code et scripts Python

Étape **Code**, outil `Code`.

Deux sujets sur cette page : l'éditeur de code, et l'API Python qui pilote les fichiers d'Aryadne.

## L'éditeur de code

Il ouvre tout fichier texte : Python, Rust, JavaScript, TypeScript, HTML, CSS, JSON, TOML, YAML, C, C++, Java, Go, Lua, Bash, GLSL, WGSL, C#, PHP, Ruby, Swift, Kotlin, et les autres.

### Écrire

| Aide | Comportement |
|---|---|
| Fermeture automatique | `( [ { " ' `` ` `` et les balises HTML : `<div>` donne `</div>` |
| `Entrée` | Garde l'indentation. Ajoute un niveau après `{ [ ( :` ou entre deux balises. Continue les listes Markdown |
| `Tab`, `Maj+Tab` | Indenter, désindenter la sélection |
| Emmet (HTML) | `!`, `div.carte#id`, `ul>li*3`, puis `Tab` |
| Suggestions | Mots du fichier, mots-clés du langage, balises, `[[notes]]`. `Ctrl+Espace` les force. `Tab` ou `Entrée` insère |
| Extraits | Par langage : `def`, `class`, `fn`, `for`, opérateur Blender… |

### Raccourcis

| Raccourci | Effet |
|---|---|
| `Ctrl+F` | Chercher. `F3` ou `Entrée` : suivant. `Maj+F3` : précédent. `Match case` respecte la casse |
| `Ctrl+H` | Remplacer : un par un, ou `Replace all` |
| `Ctrl+G` | Aller à la ligne. Tape un numéro, ou `@` pour un symbole |
| `Ctrl+Maj+O` | Aller au symbole |
| `Ctrl+/` | Commenter |
| `Ctrl+D` | Ajouter un curseur sur l'occurrence suivante |
| `Ctrl+Maj+L` | Un curseur sur toutes les occurrences |
| `Ctrl+L` | Choisir la ligne |
| `Ctrl+Maj+D` | Dupliquer la ligne |
| `Alt+Maj+↑`, `Alt+Maj+↓` | Dupliquer vers le haut, vers le bas |
| `Alt+↑`, `Alt+↓` | Déplacer la ligne |
| `Ctrl+Maj+K` | Supprimer la ligne |
| `Ctrl+]`, `Ctrl+[` | Indenter, désindenter |
| `Ctrl+Maj+[`, `Ctrl+Maj+]` | Replier, déplier le bloc |
| `Alt+Z` | Retour à la ligne automatique |
| `Alt+Maj+F` | Formater le fichier |
| `Origine` | Début du texte de la ligne, puis début de la ligne |
| `F5` | Exécuter le script |
| `` Ctrl+` `` | Terminal |

Avec plusieurs curseurs, la frappe et l'effacement se font partout à la fois.

**Note.** `Ctrl+L` ouvre aussi l'assistant IA (raccourci global). Dans l'éditeur de code, la touche choisit la ligne. Si le comportement te gêne, change le raccourci de l'assistant dans [Réglages](reglages.md#shortcuts-raccourcis).

### Affichage

Numéros de ligne, ligne courante surlignée, guides d'indentation, parenthèse correspondante surlignée, repli des blocs (flèches dans la gouttière), mini-carte à droite (réglable).

Dans la gouttière, des marques montrent les `Added lines`, `Changed lines` et `Deleted lines` depuis le dernier commit git, ou depuis l'ouverture du fichier. `Restore` remet ces lignes comme avant.

### Structure

Le panneau `Structure` liste les fonctions, classes et titres du fichier. Un clic y va.

### Vérification et formatage

- **À l'enregistrement** : Python, JavaScript, Bash, Lua et JSON sont vérifiés. La ligne fautive est soulignée, un point rouge s'affiche, le message apparaît au survol. `Check now` le fait à la demande.
- **Formater** (`Alt+Maj+F`) : utilise rustfmt, ruff ou black, prettier, clang-format, gofmt s'ils sont installés. Le JSON est formaté en interne.

### Serveurs de langage (LSP)

Aryadne utilise les serveurs déjà installés sur l'ordinateur : rust-analyzer, pyright, pylsp, typescript-language-server, clangd, gopls, lua-language-server, bash-language-server. Un serveur par langage et par projet.

| Fonction | Geste |
|---|---|
| Complétion intelligente | Pendant la frappe |
| Erreurs en direct | Soulignées. Le compteur `problems` en bas liste toutes les erreurs du projet |
| Documentation | Au survol |
| Aller à la définition | `F12` |
| Références | `Maj+F12` |
| Renommer partout | `F2` sur un symbole. `Maj+F2` renomme le fichier |

Sans serveur installé pour le langage, la barre du bas affiche `no lsp`. L'éditeur marche quand même, avec ses propres suggestions.

### Exécuter un script

`F5`, ou le bouton d'exécution. La sortie s'affiche en direct sous l'éditeur : **Program output**, avec `Stop` et `Retry`.

| Extension | Programme lancé |
|---|---|
| `.py` | `python3` (`python` sous Windows) |
| `.js`, `.mjs` | `node` |
| `.sh` | `bash` |
| `.lua` | `lua` |
| `.rb` | `ruby` |
| `.ps1` | `powershell` |
| `.bat`, `.cmd` | `cmd` |

Le programme doit être installé sur l'ordinateur. Un bouton lance aussi un script Python **dans Blender** en arrière-plan (`blender --background --python`).

### Terminal

`` Ctrl+` `` ouvre le terminal intégré. `↑` `↓` rappellent l'historique. `Ctrl+C` arrête la commande. `Clear` vide l'affichage.

### Aperçu en direct

Pour un fichier HTML ou SVG : **Live preview**. La page s'ouvre dans ton navigateur et se recharge à chaque enregistrement. Un petit serveur web local (127.0.0.1) sert les fichiers du coffre.

### Comparer et git

Palette de commandes : `File: compare with another file…`, `File: git history` (si le coffre est sous git).

### Limites de l'éditeur

- Pas de débogueur.
- Le terminal est simple : une commande à la fois, pas de programmes plein écran (vim, htop).
- La coloration couvre les langages courants. Les autres passent par un colorateur générique.

## L'API Python `aryadne`

Les fichiers d'Aryadne sont du JSON lisible. Le module `aryadne` les ouvre, les modifie et les réenregistre depuis un script Python. Aryadne recharge un fichier quand il change sur le disque.

### Démarrer

`Help › Example Python script (API)` :

- crée `scripts/exemple aryadne.py` dans le coffre et l'ouvre ;
- pose le module dans `.app/python/aryadne.py`.

`F5` le lance. Aryadne ajoute `.app/python` au chemin de Python et indique le coffre au script (variable `ARYADNE_VAULT`).

Il faut Python 3 installé. Le module n'a aucune dépendance.

### Sécurité

- Écrire hors du coffre est refusé (`ValueError`).
- Une copie de chaque fichier modifié est gardée dans `.app/backup`, avec l'heure dans son nom.
- L'écriture est atomique : un fichier temporaire, puis un renommage.

### Coffre

```python
import aryadne
coffre = aryadne.Coffre()          # le coffre du script, ou Coffre("/chemin")
```

| Méthode | Effet |
|---|---|
| `coffre.fichiers(extension="")` | Tous les fichiers du coffre (chemins relatifs), ou ceux d'une extension. Les dossiers cachés sont ignorés |
| `coffre.lire(rel)` | Le texte d'un fichier |
| `coffre.ecrire(rel, texte)` | Remplace un fichier (avec copie de sauvegarde) |
| `coffre.note(rel)` | Lit une note |
| `coffre.note(rel, texte)` | La remplace |
| `coffre.scene(rel)` | Ouvre une scène 3D |
| `coffre.montage(rel)` | Ouvre un montage |
| `coffre.dessin(rel)` | Ouvre un dessin vectoriel |
| `coffre.chemin(rel)` | Le chemin complet. Refuse de sortir du coffre |

Les trois objets ouverts ont une méthode `enregistrer()` et un champ `data` : le JSON complet du fichier, à lire ou modifier librement.

### Scene (`.aryscene`)

| Méthode | Effet |
|---|---|
| `scene.objets()` | La liste des objets (des dictionnaires) |
| `scene.objet(nom)` | Un objet par son nom. `KeyError` s'il n'existe pas |
| `scene.cle(nom, image, pos=None, rot=None, scale=None, lissage="bezier")` | Pose ou remplace une clé. Les valeurs absentes reprennent celles de l'objet |
| `scene.cadence()` | Les images par seconde |

Champs utiles d'un objet : `name`, `file`, `pos`, `rot` (degrés), `scale`, `visible`, `parent`, `material`, `keys`. Voir [Formats](../technique/formats.md#scène-3d-aryscene).

```python
scene = coffre.scene("3d/Cube.aryscene")
boule = scene.objet("boule")
boule["pos"] = [0, 1, 0]
scene.cle("boule", 24, pos=[0, 2, 0])     # une clé à l'image 24
scene.enregistrer()
```

Valeurs de `lissage` : `"bezier"`, `"linear"`, `"constant"`.

### Montage (`.aryseq`)

| Méthode | Effet |
|---|---|
| `montage.clips(piste=None, son=None)` | Les clips. Filtre par numéro de piste, et par `son=True` ou `False` |
| `montage.duree_clip(c)` | Durée d'un clip, en secondes, vitesse comprise |
| `montage.duree()` | Durée totale |
| `montage.renommer_plans(modele="Plan {n}")` | Renomme les plans vidéo dans l'ordre de la timeline. `{n}` : le numéro. `{nom}` : l'ancien nom. Rend le nombre de plans |

Champs utiles d'un clip : `name`, `start`, `src_in`, `src_out`, `speed`, `track`, `audio`.

```python
montage = coffre.montage("montage/Essai.aryseq")
for c in montage.clips(son=False):
    print(c["name"], c["start"], montage.duree_clip(c))
montage.renommer_plans("SH{n:03d}")
montage.enregistrer()
```

### Dessin (`.aryvec`)

| Méthode | Effet |
|---|---|
| `dessin.plans()` | Les plans de travail |
| `dessin.formes()` | Les formes |
| `dessin.forme(nom)` | Une forme par son nom |
| `dessin.couleur(forme, couleur)` | Remplissage uni : `"#rrggbb"`, `"#rrggbbaa"` ou `[r, g, b, a]`. Détache la forme de sa variable de couleur |

```python
dessin = coffre.dessin("2d/Maquette.aryvec")
for f in dessin.formes():
    if f["name"].startswith("Bouton"):
        dessin.couleur(f, "#ec6c59")
dessin.enregistrer()
```

### Aller plus loin

L'API fournie est petite. Pour le reste, modifie `data` directement :

```python
scene = coffre.scene("3d/Ville.aryscene")
scene.data["fps"] = 25
for o in scene.objets():
    if o.get("role") == "light":
        o["energy"] = 2.0
scene.enregistrer()
```

Les graphes de nœuds (`.arymat`, `.arycomp`, `.arygeo`), les boards, les compositions Motion, les sessions audio et les morceaux sont aussi du JSON : `json.loads(coffre.lire(rel))` les ouvre. Leur structure est décrite dans [Formats](../technique/formats.md).

### Défauts connus de l'API

- **Ancienne extension dans les exemples.** Le script d'exemple fourni et l'aide du module cherchent les montages avec `coffre.fichiers(".aryav")`. C'est l'ancienne extension. Un montage créé aujourd'hui porte `.aryseq` : l'exemple ne le listera pas. Écris `.aryseq` dans tes scripts.
- L'API est un module de fichiers. Elle ne parle pas à l'application ouverte : pas de sélection courante, pas de commande « lance un rendu ».
- Pas de classe pour les nœuds, les boards, le Motion ni le son.
- Les images à calques (`.aryimg`) et les caches de géométrie ne sont pas du JSON : l'API ne les lit pas.
- Si un fichier est ouvert et modifié dans Aryadne pendant que le script l'écrit, la dernière écriture gagne.
