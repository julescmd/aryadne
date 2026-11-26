---
titre: Piloter Aryadne en Python
niveau: avancé
duree: 15 min
prerequis: 06-video-son-code/06-editeur-de-code
objectifs: Comprendre ce que fait le module aryadne ; Lire et écrire des notes par script ; Modifier une scène, un montage ou un dessin ; Connaître les garde-fous et les limites
---

## Ce que tu vas réussir à faire

Tu vas écrire un script qui lit tout ton coffre et fabrique une note de synthèse. Puis tu verras comment le même module renomme les plans d'un montage ou pose des clés dans une scène 3D.

Il te faut Python 3 et les bases du langage : variables, boucles, fonctions.

## L'idée

Les fichiers d'Aryadne sont du texte lisible, en JSON. Le module Python `aryadne` les ouvre, les modifie et les réenregistre. Aryadne recharge un fichier quand il change sur le disque.

Ton script ne parle donc pas à l'application. Il travaille sur les fichiers.

## Installer le module

```etape
Ouvre **File › Help › Example Python script (API)**.
verifier: onglet = code
```

Cette commande fait deux choses :

- elle crée `scripts/exemple aryadne.py` dans le coffre et l'ouvre ;
- elle pose le module dans `.app/python/aryadne.py`.

```etape
Lis le script d'exemple. Appuie sur `F5`. Il liste tes scènes, tes montages et tes dessins.
```

Quand tu lances un script avec `F5`, Aryadne indique à Python où est le module et où est le coffre. Tu n'as rien à configurer.

## Le coffre

```essayer
fichier: fichiers/inventaire.py
```

```etape
Clique **Essaie par toi-même**. Le script d'inventaire s'ouvre.
verifier: fichier.ouvert = py
```

Tout commence par ces deux lignes :

```
import aryadne
coffre = aryadne.Coffre()
```

| Méthode | Effet |
|---|---|
| `coffre.fichiers(extension)` | La liste des fichiers du coffre, en chemins relatifs. Avec une extension : seulement ceux-là |
| `coffre.lire(rel)` | Le texte d'un fichier |
| `coffre.ecrire(rel, texte)` | Remplace un fichier |
| `coffre.note(rel)` | Lit une note |
| `coffre.note(rel, texte)` | Remplace une note |
| `coffre.chemin(rel)` | Le chemin complet d'un fichier |

Les dossiers cachés, comme `.app/`, sont ignorés par `fichiers`.

```etape
Lis le script. Repère les trois temps : lister les notes, compter les cases `- [ ]`, préparer les lignes du résultat.
```

```etape
Lance-le avec `F5`. Lis le nombre de notes et de tâches dans **Program output**.
```

## Écrire dans le coffre

```etape
Retire le `#` au début de la dernière ligne. Enregistre avec `Ctrl+S`.
verifier: fichier.enregistre
```

```etape
Relance avec `F5`. Ouvre la note créée avec `Ctrl+P`, en tapant `inventaire`.
```

### Les garde-fous

- Écrire hors du coffre est refusé. Le script s'arrête sur une erreur `ValueError`.
- Avant chaque écriture, une copie du fichier est gardée dans `.app/backup`, avec l'heure dans son nom.
- L'écriture se fait dans un fichier temporaire, puis par un renommage. Un script interrompu ne laisse pas un fichier à moitié écrit.

```attention
Si un fichier est ouvert et modifié dans Aryadne pendant que ton script l'écrit, la dernière écriture gagne. Enregistre tes fichiers ouverts avant de lancer un script qui les modifie.
```

## Les scènes 3D

```
scene = coffre.scene("3d/Cube.aryscene")
boule = scene.objet("boule")
boule["pos"] = [0, 1, 0]
scene.cle("boule", 24, pos=[0, 2, 0])
scene.enregistrer()
```

| Méthode | Effet |
|---|---|
| `scene.objets()` | La liste des objets |
| `scene.objet(nom)` | Un objet par son nom |
| `scene.cle(nom, image, pos, rot, scale, lissage)` | Pose ou remplace une clé |
| `scene.cadence()` | Les images par seconde |

Un objet est un dictionnaire. Ses champs utiles : `name`, `pos`, `rot` en degrés, `scale`, `visible`, `parent`, `material`, `keys`.

Les valeurs de `lissage` : `"bezier"`, `"linear"`, `"constant"`.

## Les montages

```
montage = coffre.montage("montage/Essai.aryseq")
for c in montage.clips(son=False):
    print(c["name"], c["start"], montage.duree_clip(c))
montage.renommer_plans("SH{n:03d}")
montage.enregistrer()
```

| Méthode | Effet |
|---|---|
| `montage.clips(piste, son)` | Les clips. Filtre par numéro de piste, et par `son=True` ou `False` |
| `montage.duree_clip(c)` | La durée d'un clip, en secondes |
| `montage.duree()` | La durée totale |
| `montage.renommer_plans(modele)` | Renomme les plans vidéo dans l'ordre. `{n}` : le numéro. `{nom}` : l'ancien nom |

`"SH{n:03d}"` donne SH001, SH002, SH003.

## Les dessins vectoriels

```
dessin = coffre.dessin("2d/Maquette.aryvec")
for f in dessin.formes():
    if f["name"].startswith("Bouton"):
        dessin.couleur(f, "#ec6c59")
dessin.enregistrer()
```

| Méthode | Effet |
|---|---|
| `dessin.plans()` | Les plans de travail |
| `dessin.formes()` | Les formes |
| `dessin.forme(nom)` | Une forme par son nom |
| `dessin.couleur(forme, couleur)` | Un remplissage uni : `"#rrggbb"`, `"#rrggbbaa"` ou `[r, g, b, a]` |

```attention
`dessin.couleur` détache la forme de sa variable de couleur. Elle ne suivra plus le mode clair ou sombre.
```

## Aller plus loin : le champ data

Les trois objets ont un champ `data` : le JSON complet du fichier. Ce que l'API ne couvre pas, tu le changes là.

```
scene = coffre.scene("3d/Ville.aryscene")
scene.data["fps"] = 25
scene.enregistrer()
```

Les graphes de nodes, les boards, les compositions Motion, les sessions audio et les morceaux sont aussi du JSON. Le module `json` de Python les lit à partir de `coffre.lire(rel)`. Leur structure est décrite dans la partie technique de la documentation, page des formats.

```etape
Ajoute à la fin de ton script deux lignes qui affichent le nombre de montages du coffre. Lance avec `F5`.
verifier: texte.contient = .aryseq
```

```astuce
Besoin d'un indice ? `len(coffre.fichiers(".aryseq"))` donne le nombre de montages.
```

## Ce que l'API ne fait pas

- Elle ne parle pas à l'application ouverte. Pas de sélection courante, pas de commande pour lancer un rendu.
- Elle n'a pas de classe pour les nodes, les boards, Motion ni le son.
- Elle ne lit pas les images à calques `.aryimg` : ce n'est pas du JSON.

## Dans Blender

L'éditeur a aussi un bouton qui lance un script Python dans Blender, en arrière-plan. Il faut Blender installé. C'est un autre usage : ton script parle alors à Blender, pas à Aryadne.

```quiz
Comment le module aryadne modifie-t-il une scène ?
- [ ] Il envoie des commandes à l'application ouverte.
- [x] Il lit et réécrit le fichier de la scène.
- [ ] Il passe par internet.
explication: C'est un module de fichiers. Aryadne recharge le fichier quand il change sur le disque.
```

```quiz
Ton script essaie d'écrire dans un dossier hors du coffre. Que se passe-t-il ?
- [ ] Le fichier est écrit.
- [x] L'écriture est refusée, avec une erreur ValueError.
- [ ] Le coffre est déplacé.
explication: Le module refuse de sortir du coffre. C'est un garde-fou.
```

```quiz
Où retrouver un fichier tel qu'il était avant le passage d'un script ?
- [x] Dans `.app/backup`, avec l'heure dans son nom.
- [ ] Nulle part.
- [ ] Dans la corbeille du système.
explication: Une copie est gardée avant chaque écriture.
```

```defi
Écris un script qui renomme les plans d'un de tes montages en SH010, SH020, SH030.
indice: `renommer_plans` numérote de 1 en 1. Ajoute un zéro après le numéro dans le modèle : `montage.renommer_plans("SH{n:02d}0")`. N'oublie pas `montage.enregistrer()`.
```

## Récapitulatif

- `aryadne.Coffre()` ouvre le coffre. `fichiers`, `lire`, `ecrire` et `note` travaillent sur les textes.
- `scene`, `montage` et `dessin` ouvrent un fichier, avec `enregistrer()` et le champ `data`.
- Écriture limitée au coffre, copie dans `.app/backup` : un script ne peut pas tout casser.

Leçon suivante : [L'assistant IA](08-assistant-ia.md)
