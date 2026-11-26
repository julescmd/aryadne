# Banc de mesure de la carte graphique

Le banc mesure ce que ta carte graphique fait vraiment sur des calculs de simulation. Il sert à deux choses :

- savoir si le tissu sur carte graphique sera rapide sur ta machine ;
- donner des chiffres réels pour décider de la suite du moteur (fumée, fluides).

Les rapports de recherche du projet montrent que la simulation est bornée par la **mémoire** : ce qui compte, c'est le nombre d'octets que la carte lit et écrit par seconde. Le banc mesure donc d'abord ce débit.

## Le lancer

| Depuis | Geste |
|---|---|
| L'application | `Help › Measure the graphics card` |
| Un terminal | `aryadne --banc-gpu` |

Dans l'application, une fenêtre s'ouvre : « Measuring, a few seconds ». Elle peut sortir de la fenêtre principale. À la fin, le rapport s'affiche. `Copy the report` le met dans le presse-papiers.

Dans un terminal, l'interface ne s'ouvre pas. L'avancement s'écrit ligne par ligne, puis le rapport complet.

## Ce qui est mesuré

### 1. Le débit brut

| Mesure | Ce qu'elle dit |
|---|---|
| `Copy of N MB` | Copier un gros tampon d'un endroit à l'autre. C'est le plafond de débit : rien n'ira plus vite |
| `Pressure iteration N³` | Une itération de pression sur une grille 3D, où chaque case lit ses 6 voisines et elle-même (7 points). C'est le cœur d'une simulation de fumée ou de fluide |

Chaque ligne donne un temps par passe et un débit en `GB/s`.

### 2. Un pas de fumée

`Smoke step N³ (40 pressure iterations)` : un pas complet d'une fumée simple. Déplacement de la fumée par le courant, flottabilité, puis 40 itérations de pression.

Ce n'est **pas** une fumée utilisable dans Aryadne. C'est un essai de charge, pour savoir combien de temps coûterait un tel pas.

Si la fumée donne des valeurs non finies, le banc le signale : c'est une erreur du calcul, pas de ta carte.

### 3. Le tissu

`VBD cloth (one pass = every vertex updated once)`. Deux tailles : 250 000 et 1 million de sommets.

| Colonne | Sens |
|---|---|
| `Vertices` | Nombre de points du tissu |
| `Colors` | Nombre de couleurs (voir [Simulation de tissu](simulation-tissu.md#ce-que-dit-laperçu)) |
| `Per pass` | Temps d'une passe sur la carte |
| `Passes per second` | Combien de passes par seconde |
| `Per vertex per pass` | Temps ramené à un sommet |
| `GB/s` | Débit mémoire pendant le calcul |
| `At 24 frames per second` › `Passes per frame` | Combien de passes tiennent dans une image à 24 images par seconde |
| `One frame of 32 passes` | Temps d'une image type : 4 sous-pas de 8 itérations |
| `Readback` | Temps pour relire les positions depuis la carte |
| `CPU` › `Per pass` | Le même calcul sur le processeur, pour comparer |
| `Setup` | Préparation au chargement : voisinages, couleurs, rangement |
| `Memory on the card` | Mémoire occupée par ce tissu |

Si un tissu ne tient pas sur la carte : « Cloth of N vertices: it does not fit on this card, measured on the CPU ».

### 4. VBD contre XPBD

`VBD versus XPBD, at an equal number of passes`. Les deux méthodes reçoivent le même budget de calcul. Le banc compare ce qu'elles en font. Cette partie prend moins d'une minute.

| Essai | Mesures |
|---|---|
| `Hanging cloth, fully converged` | `Elongation` (de combien le tissu s'allonge sous son poids), `Elastic energy` |
| `Falling on a ball` | `Worst edge` (le fil le plus étiré), `Mean` |

Colonnes : `Passes`, `Stiffness obtained`, `Elongation`, `Energy`.

Un tissu qui s'allonge moins, à budget égal, est une méthode qui converge mieux.

### 5. Contrôles

| Ligne | Sens |
|---|---|
| `Difference with the CPU (check)` | Écart entre le résultat de la carte et celui du processeur sur une itération de pression. Doit être minuscule |
| `Cloth gap between the card and the CPU (check)` | Le même contrôle pour un petit tissu, en mètres |

Un écart trop grand fait échouer le banc avec un message : « Cloth: gap with the CPU too large ». Cela signale un pilote ou une carte qui calcule mal.

### 6. Estimations

`What this means next (estimate)` : à partir du débit mesuré, le banc estime le coût de calculs qui n'existent pas encore.

| Ligne | Sens |
|---|---|
| `Dense 512³ smoke, multigrid pressure (8 iterations), per step` | Ce que coûterait un pas de fumée dense en 512³ |
| `512³ advection and forces, per step` | La part du déplacement et des forces |

Ce sont des extrapolations. Elles disent un ordre de grandeur, pas une promesse.

## Lire le résultat

- **Rendu logiciel.** Si le rapport dit « Software rendering (no real graphics card) », Aryadne n'a pas trouvé de vraie carte. Les chiffres disent seulement que le code marche. Vérifie tes pilotes.
- **Le débit de copie** est ta référence. Un calcul qui atteint une bonne part de ce débit est bien écrit pour ta carte.
- **Passes par image.** Si le tissu d'un million de sommets fait plus de 32 passes par image à 24 i/s, il se simule en temps réel sur ta machine. En dessous, il faudra moins de points, ou mettre la simulation en cache.
- **Relecture.** Si elle pèse lourd devant une passe, c'est le retour des positions vers l'affichage qui limite, pas le calcul.

## Sans carte

Le banc a besoin d'une carte utilisable par wgpu (Vulkan, Metal ou DirectX 12). Sans elle, il s'arrête avec un message. Le tissu d'Aryadne, lui, passe alors sur le processeur.

Pour forcer le repli sur le processeur sans débrancher la carte (essai de développeur), lance Aryadne avec la variable d'environnement `ARY_SANS_GPU` définie.

## Limites

- Le banc mesure le calcul, pas l'affichage. La vue 3D d'Aryadne reste sur le processeur.
- Les chiffres varient avec la charge de la machine. Ferme les autres logiciels lourds et lance deux fois.
- Sur un rendu logiciel, le banc passe tout seul à de petites tailles : les chiffres ne se comparent pas à ceux d'une vraie carte.

Voir aussi : [Simulation sur carte graphique](../technique/simulation-gpu.md), [Feuille de route du moteur](../technique/feuille-de-route-moteur.md).
