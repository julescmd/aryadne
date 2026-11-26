# Prise en main

## L'idée

Aryadne ouvre un **coffre** : un dossier de ton disque. Tout ce que tu crées est un fichier dans ce dossier. Rien n'est enfermé dans une base de données. Voir [Coffre et fichiers](coffre-et-fichiers.md).

Le travail est rangé par **étapes du pipeline**. Chaque étape est un espace de travail avec ses outils.

## Premier lancement

1. Lance l'exécutable. Aucune installation, aucun droit administrateur.
2. L'écran d'accueil propose **Open or create a vault** (ouvrir ou créer un coffre). Choisis un dossier.
3. Aryadne crée dedans `notes/`, `refs/`, `boards/` et un dossier caché `.app/`.

Au lancement suivant, le dernier coffre s'ouvre tout seul.

## La fenêtre

De haut en bas :

- **La barre de menus** : `File`, `Edit`, `View`, `Window`, `Help`. Au centre, un champ **Search or run a command** ouvre la [palette de commandes](palette-de-commandes.md).
- **La barre des étapes** : `Notes`, `2D`, `3D`, `Motion`, `Video`, `Sound`, `Code`. Un clic change d'étape. `Ctrl+1` à `Ctrl+7` font de même.
- **Les outils de l'étape** : par exemple `Scene`, `Materials` et `Simulation` en 3D. `Alt+1`, `Alt+2`… les choisissent.
- **Les onglets** des fichiers ouverts.
- **Le document** au centre, avec ses panneaux autour.
- **La barre du bas** : aide contextuelle (les raccourcis utiles de l'outil en cours), état d'enregistrement, minuteur, version.

La barre du bas change selon ce que tu fais. C'est le premier endroit où regarder quand tu cherches un raccourci.

## Les étapes et leurs outils

| Étape | Couleur | Outils (libellé affiché) | Pages |
|---|---|---|---|
| Notes | celle des réglages | `Notes`, `Tasks`, `Brain` | Files, Search |
| 2D | rose | `Moodboard`, `Retouch`, `Vector`, `Compositing` | Files, References |
| 3D | bleu | `Scene`, `Materials`, `Simulation` | Files, References |
| Motion | doré | `Comps` | Files, References |
| Video | violet | `Storyboard`, `Edit`, `Review` | Files, Media |
| Sound | bleu-vert | `Audio`, `Music` | Files, Search |
| Code | vert | `Code` | Files, Search |

Chaque étape garde ses onglets ouverts, sa disposition de panneaux et sa couleur d'accent. Passer de 3D à Video ne ferme rien : tu retrouves l'étape comme tu l'as laissée.

Un fichier s'ouvre dans l'étape qui lui correspond. Ouvrir un `.aryscene` depuis les Notes te fait passer en 3D.

## Créer un fichier

Trois chemins mènent à la même fenêtre **New file** :

- `File › New`, puis le genre voulu ;
- le bouton `+` du panneau des fichiers ;
- la palette de commandes : tape « new ».

Dans la fenêtre :

1. Clique le genre de fichier à gauche. Son extension s'affiche à côté.
2. Règle les options à droite (voir tableau).
3. `Create` (ou `Entrée`).

| Genre | Options |
|---|---|
| 3D scene | `Cube, light and camera` ou `Empty scene` |
| Layered image | Taille (dont `A4 at 150 dpi`), `White background` ou `Transparent background` |
| Video edit, composition | Résolution, `fps` |
| Node material | Un préréglage, ou `Empty (output only)` |
| Code or text | Le langage |
| Folder, board | Un nom |

Le fichier s'ouvre sous le nom `untitled.<extension>`. Il n'a pas encore de place dans le coffre. `Ctrl+S` demande son nom et son dossier.

Si Aryadne se ferme avant, les fichiers sans titre sont proposés au lancement suivant : **Files to recover**, avec `Reopen them` ou `Delete them`.

## Enregistrer

- `Ctrl+S` : enregistrer. Sur un fichier sans titre : choisir le nom et le dossier.
- `Ctrl+Maj+S` : `Save a copy as…`. L'original reste, la copie s'ouvre.
- Les notes et le code s'enregistrent aussi tout seuls après un délai (réglable).
- La barre du bas affiche `● modified` ou `saved`.

## Ouvrir un fichier

- `Ctrl+P` : tape quelques lettres du nom, `Entrée`. Si rien ne correspond, `Entrée` crée une note de ce nom.
- Double-clic dans le panneau des fichiers.
- Glisse un fichier depuis ton bureau dans la fenêtre. Aryadne demande s'il faut le **lier**, le **déplacer** ou le **copier**. Voir [Coffre et fichiers](coffre-et-fichiers.md#importer).

## Annuler

`Ctrl+Z` annule dans l'éditeur sous la souris. Chaque éditeur a son propre historique. Dans le panneau des fichiers, `Ctrl+Z` annule la dernière opération sur les fichiers (déplacement, suppression).

**Limite.** Il n'y a pas d'historique global qui traverse les éditeurs.

## L'écran d'accueil

`File › Help › Home` (ou `Edit › Window › Home`) montre :

- **Pick up where you left off** : le coffre et les onglets à rouvrir ;
- **Start** : les fichiers les plus courants à créer ;
- **Recent**, filtrable par genre ;
- **Ask the Assistant** : une question directe à l'[assistant](assistant-ia.md) ;
- **Useful shortcuts**.

Chaque étape vide affiche aussi sa propre page d'accueil, avec un bouton pour créer le bon fichier.

## Langue

`Settings › Appearance` propose le français et l'anglais. Le français est la langue d'origine du code. Un texte sans traduction reste en français.

## Mises à jour

Aryadne vérifie les nouvelles versions au démarrage (toutes les 6 heures). Un clic sur le numéro de version, en bas, ouvre la fenêtre **Updates** : `Check`, `Install`, `Restart`.
