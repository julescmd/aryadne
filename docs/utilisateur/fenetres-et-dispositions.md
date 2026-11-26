# Fenêtres et dispositions

L'interface est faite de **panneaux**. Chaque panneau peut changer de place, s'empiler avec d'autres en onglets, flotter, ou sortir dans une vraie fenêtre du système que tu poses sur un autre écran.

## Les panneaux de base

| Panneau | Contenu | Raccourci |
|---|---|---|
| `Files` | L'explorateur du coffre | `Ctrl+B` |
| `Document` | Le fichier ouvert. Il ne se ferme pas | |
| `Structure` / `Links` | Titres, symboles, liens entrants | |
| `Assistant` | L'assistant IA | `Ctrl+L` |
| `Terminal` | Le terminal intégré | `` Ctrl+` `` |

Chaque éditeur ajoute les siens. Exemples : `Scene (3D)`, `Properties (3D)`, `Tools (3D)`, `3D timeline` ; `Project (edit)`, `Timeline (edit)`, `Program monitor` ; `Layers (vector)`, `Properties (vector)` ; `Preview and settings (nodes)`.

## Déplacer un panneau

La disposition est en **cases**, comme dans Blender ou Premiere.

- Glisse l'onglet d'un panneau vers le bord d'une case : la case se divise.
- Glisse-le sur les onglets d'une autre case : il s'y empile.
- Glisse la limite entre deux cases : elles changent de taille.
- Clic droit sur un onglet : `Move to its own window`, `Reset layout`.

## Sortir un panneau de la fenêtre

Trois façons :

1. Le bouton **Detach to its own window** dans le coin du panneau.
2. Tirer la poignée du panneau hors de la fenêtre, vers un autre écran. Un message « Release: separate window » s'affiche.
3. `Window › Panels`, puis cocher le panneau.

Pour le rattacher : `Attach back to the main window`, ou ramener sa fenêtre au-dessus de la fenêtre principale et attendre (« Hold here: the panel attaches »). Fermer la fenêtre détachée le rattache aussi.

### Le document dans sa propre fenêtre

`Window › Document in its own window` sort la zone du document (vue 3D, toile, montage, texte) dans une vraie fenêtre. Décoché, ou la fenêtre fermée, elle revient.

### Une étape entière dans une nouvelle fenêtre

Dans le menu des dispositions : **Open this step in a new window**. Exemple donné par l'application : les matériaux sur un écran et la scène sur l'autre.

### Panneaux flottants

Un panneau peut aussi flotter à l'intérieur de la fenêtre. Clic droit sur sa poignée : `Dock left`, `Dock right`, `Dock bottom`, `Floating panel`, `Separate window`.

### Écran débranché

Aryadne retient l'écran de chaque fenêtre détachée. Si cet écran n'est plus là au lancement, la fenêtre revient sur celui de la fenêtre principale. Cette détection existe sous Linux (X11) et Windows. Ailleurs, la liste des écrans est inconnue et rien n'est corrigé.

## Onglets de fichiers

| Geste | Effet |
|---|---|
| Clic | Afficher |
| Clic du milieu, `Ctrl+W` | Fermer |
| `Ctrl+Tab`, `Ctrl+Maj+Tab` | Onglet suivant, précédent |
| Glisser hors de la barre | Sortir le fichier dans son propre panneau |
| Clic droit | `Open in a floating panel`, `Open in a panel on the right`, `Open in a panel at the bottom`, `Open in its own window`, `Close` |

Un fichier sorti en panneau reste ouvert à côté du document principal. `Put back among the tabs` le remet.

### Vue côte à côte

`Ctrl+\` (ou `View › Side by side`) montre deux fichiers. `Swap both sides` les échange. `Close side-by-side view` revient à un seul.

## Dispositions par étape

Une étape peut avoir plusieurs dispositions nommées, par exemple « Modeling » et « Animation » en 3D.

Menu `View › Layout` (ou le bouton de disposition) :

| Action | Effet |
|---|---|
| Champ de nom, puis `Save` | Garde l'arrangement actuel sous ce nom |
| `Rename to this name` | Renomme la disposition en cours |
| `Reset this layout` | Revient à l'arrangement de départ |
| `Delete this layout` | La supprime |
| Clic sur un nom | Passe à cette disposition |

La première s'appelle `Main`.

Ce qui est gardé : la place de chaque panneau, les cases, l'explorateur ouvert ou non, les fenêtres ajoutées. Le tout est écrit dans `workspaces.json`, dans le dossier de configuration (voir [Formats](../technique/formats.md#réglages-de-lutilisateur)).

## Ajouter une page

`Window › Add a window` ouvre une page de plus dans un panneau : `Brain`, `References`, `Board`, `Chat`, `Tasks`, `Search`, ou `The open file, separately`.

## Autres réglages de fenêtre

| Commande | Effet |
|---|---|
| `View › Compact mode` (`Ctrl+Maj+M`) | Cache les panneaux. Pratique pour garder un board de références par-dessus un autre logiciel |
| `View › Keep on top` | Aryadne reste au-dessus des autres fenêtres |
| `View › Full screen` | Plein écran |
| `View › Light theme` / `Dark theme` | Change le thème |
| `Ctrl +`, `Ctrl −`, `Ctrl 0` | Taille de l'interface |

## Limites

- La fenêtre a une taille minimum de 760 × 480.
- La feuille de route prévoit encore : découpage libre des zones en tirant un coin, import et export d'une disposition, repli automatique des panneaux trop étroits. Ce n'est pas fait.
- Un panneau d'un éditeur n'a de contenu que si un fichier de cet éditeur est ouvert. Sinon il affiche « This panel belongs to another editor ».
