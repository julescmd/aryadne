---
titre: Panneaux, fenêtres et dispositions
niveau: débutant
duree: 15 min
prerequis: 01-demarrer/05-fichiers-du-coffre
objectifs: Déplacer et empiler un panneau ; Sortir un panneau sur un autre écran ; Voir deux fichiers côte à côte ; Enregistrer une disposition par étape
---

## Ce que tu vas réussir à faire

Tu vas arranger la fenêtre à ta façon, sortir un panneau sur un deuxième écran, puis garder cet arrangement sous un nom. Tu sauras aussi tout remettre comme au départ.

```essayer
fichier: fichiers/recette.md
```

```etape
Clique **Essaie par toi-même** pour avoir un fichier ouvert.
verifier: fichier.ouvert = md
```

## Les panneaux de base

| Panneau | Contenu | Raccourci |
|---|---|---|
| **Files** | Les fichiers du coffre | `Ctrl+B` |
| Document | Le fichier ouvert. Il ne se ferme pas | |
| **Outline and links** | Les titres d'une note et ses liens | |
| **Assistant** | L'assistant IA | `Ctrl+L` |
| **Terminal** | Le terminal intégré | `` Ctrl+` `` |

Chaque éditeur ajoute ses propres panneaux. La 3D a les siens, le montage aussi.

```etape
Ouvre **Edit › Window**. Sous **Panels**, décoche puis recoche **Outline and links**. Le panneau de droite disparaît, puis revient.
```

## Déplacer un panneau

La fenêtre est découpée en cases. Chaque panneau a un onglet.

- Glisse l'onglet d'un panneau vers le bord d'une case : la case se divise.
- Glisse-le sur les onglets d'une autre case : il s'y empile.
- Glisse la limite entre deux cases : elles changent de taille.

```etape
Glisse la limite entre le panneau des fichiers et le document pour élargir le panneau.
```

## Sortir un panneau de la fenêtre

Un panneau peut devenir une vraie fenêtre du système. Tu la poses sur un autre écran.

Trois façons :

1. Le bouton **Detach to its own window**, dans le coin du panneau.
2. Tirer la poignée du panneau hors de la fenêtre.
3. **Edit › Window**, puis l'icône devant le nom d'un panneau de l'éditeur.

Pour le ramener : **Attach back to the main window**. Fermer la fenêtre détachée le ramène aussi.

```etape
Ouvre **Edit › Window** et coche **Document in its own window**. Le document sort dans sa propre fenêtre.
```

```etape
Décoche **Document in its own window**. Le document revient.
```

```astuce
Aryadne retient l'écran de chaque fenêtre détachée. Si cet écran est débranché au lancement suivant, la fenêtre revient sur l'écran principal.
```

## Deux fichiers côte à côte

```etape
Ouvre un deuxième fichier avec `Ctrl+P`. Appuie ensuite sur `Ctrl+\`. Les deux fichiers s'affichent côte à côte.
```

```etape
Appuie encore sur `Ctrl+\` pour revenir à un seul fichier.
```

La même commande est dans **Edit › View › Side by side**.

Tu peux aussi sortir un onglet. Clic droit sur l'onglet d'un fichier :

- **Open in a floating panel** ;
- **Open in a panel on the right** ;
- **Open in a panel at the bottom** ;
- **Open in its own window**.

**Put back among the tabs** le remet à sa place.

## Les dispositions

Une disposition est un arrangement de panneaux qui porte un nom. Chaque étape a les siennes. La première s'appelle **Main**.

```etape
Ouvre **Edit › Layout**. Dans le champ de nom, tape `Lecture`. Clique **Save**.
```

Ton arrangement actuel est gardé sous ce nom. Il reste lié à l'étape en cours.

| Action | Effet |
|---|---|
| Clic sur un nom | Passe à cette disposition |
| **Rename to this name** | Renomme la disposition en cours avec le nom tapé |
| **Reset this layout** | Revient à l'arrangement de départ |
| **Delete this layout** | La supprime |
| **Open this step in a new window** | Ouvre l'étape dans une deuxième fenêtre, sur le même coffre |

```etape
Dans **Edit › Layout**, clique **Main** pour revenir à la disposition de départ.
```

```attention
Tu as tout déplacé et tu ne retrouves plus rien ? Ouvre **Edit › Layout**, puis **Reset this layout**.
```

## Garder Aryadne au-dessus

Deux réglages servent quand tu travailles dans un autre logiciel avec tes références sous les yeux. Ils sont dans **Edit › View**.

- **Keep on top** : Aryadne reste au-dessus des autres fenêtres.
- **Compact mode** (`Ctrl+Maj+M`) : cache les panneaux et ne garde que le document.

```quiz
Que garde une disposition ?
- [x] La place des panneaux d'une étape.
- [ ] Le contenu des fichiers ouverts.
- [ ] Les raccourcis clavier.
explication: Une disposition retient l'arrangement des panneaux et des fenêtres ajoutées, pour une étape.
```

```quiz
Comment ramener un panneau détaché dans la fenêtre principale ?
- [x] Avec **Attach back to the main window**.
- [x] En fermant sa fenêtre.
- [ ] En redémarrant l'ordinateur.
explication: Les deux premières réponses marchent. Fermer la fenêtre détachée rattache le panneau.
```

```defi
Dans l'étape 3D, crée une disposition nommée `Modeling`. Reviens ensuite à **Main**.
indice: `Ctrl+3` pour passer en 3D. Les dispositions d'une étape n'apparaissent que dans cette étape.
```

## Récapitulatif

- Un panneau se glisse par son onglet, s'empile, ou sort dans sa propre fenêtre.
- `Ctrl+\` montre deux fichiers côte à côte.
- **Edit › Layout** enregistre, rappelle et remet à zéro les dispositions d'une étape.

Leçon suivante : [Trouver de l'aide](07-trouver-de-l-aide.md)
