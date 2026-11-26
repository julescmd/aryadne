---
titre: Installer, lancer, ouvrir un coffre
niveau: débutant
duree: 10 min
prerequis:
objectifs: Lancer Aryadne sans installation ; Créer ton premier coffre ; Comprendre ce qu'est un coffre
---

## Ce que tu vas réussir à faire

Tu vas lancer Aryadne, choisir un dossier pour ton travail et écrire ta première ligne. Ce dossier s'appelle le **coffre**.

Si tu lis cette leçon dans Aryadne, ton coffre est déjà ouvert. Lis quand même les deux premières parties : tu sauras le refaire sur un autre ordinateur.

## Lancer l'application

Aryadne est un seul fichier exécutable, pour Windows et Linux. Il n'y a rien à installer. Aucun droit administrateur n'est demandé.

```etape
Lance l'exécutable d'Aryadne.
```

Au premier lancement, un écran d'accueil s'affiche avec un seul bouton : **Open or create a vault** (ouvrir ou créer un coffre).

```etape
Clique **Open or create a vault**. Choisis un dossier vide, ou crée-en un. Par exemple `Documents/MonCoffre`.
```

Aryadne crée dans ce dossier `notes/`, `refs/`, `boards/` et un dossier caché `.app/`. Au prochain lancement, ce coffre s'ouvrira tout seul.

## Ce qu'est un coffre

Le coffre est un dossier ordinaire de ton disque. Chaque chose que tu crées est un fichier dedans. Rien n'est enfermé dans une base de données.

Tu peux donc :

- ouvrir le coffre dans l'explorateur de ton système ;
- le copier sur une clé ;
- le synchroniser ou le mettre sous git.

| Dossier | Rôle |
|---|---|
| `notes/` | Tes notes |
| `refs/` | Les images de référence que tu importes |
| `boards/` | Tes boards |
| `.app/` | Les données d'Aryadne : corbeille, historique, caches |

```attention
Ne range rien toi-même dans `.app/`. Ce dossier appartient à Aryadne.
```

## Écrire ta première ligne

```essayer
nouveau: md
nom: premiers-pas
```

Le bouton ci-dessus ouvre une note vide dans le dossier `Apprendre/` de ton coffre.

```etape
Clique **Try it yourself**. Une note vide s'ouvre.
verifier: fichier.ouvert = md
```

```etape
Tape le mot `bonjour` dans la note.
verifier: texte.contient = bonjour
```

```etape
Appuie sur `Ctrl+S` pour enregistrer.
verifier: fichier.enregistre
```

```astuce
Les notes s'enregistrent aussi toutes seules après un court délai. `Ctrl+S` reste un bon réflexe.
```

## Changer de coffre

Tu peux avoir plusieurs coffres : un par projet, par exemple.

- En haut à gauche, sous la barre de titre, un bouton porte une icône de dossier et le nom de ton coffre. Clique-le.
- **Change vault…** ouvre un autre dossier.
- **Recent vaults** liste les derniers coffres ouverts.

Le même choix existe dans **File › Vault › Change vault…**.

```quiz
Qu'est-ce qu'un coffre ?
- [ ] Un fichier unique qui contient tout ton travail.
- [x] Un dossier ordinaire de ton disque.
- [ ] Un compte en ligne.
explication: Le coffre est un dossier. Chaque création est un fichier dedans, lisible sans Aryadne.
```

```quiz
Que faut-il pour lancer Aryadne ?
- [ ] Un installateur et les droits administrateur.
- [x] Seulement l'exécutable.
- [ ] Une connexion à internet.
explication: Aryadne est un exécutable portable. Rien n'est installé sur le système.
```

```defi
Ouvre ton coffre dans l'explorateur de ton système et retrouve la note `premiers-pas`.
indice: Dans le menu du coffre, **Open the folder** ouvre le dossier. La note est dans `Apprendre/`.
```

## Récapitulatif

- Aryadne se lance sans installation et ouvre un coffre.
- Le coffre est un dossier ordinaire : tes fichiers t'appartiennent.
- `Ctrl+S` enregistre le fichier ouvert.

Leçon suivante : [L'interface](02-l-interface.md)
