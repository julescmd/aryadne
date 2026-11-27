# Banc de mesure de la carte graphique

Le banc mesure ce que ta carte graphique fait vraiment sur des calculs de simulation. Il sert à deux choses :

- savoir si le tissu sur carte graphique sera rapide sur ta machine ;
- donner des chiffres réels pour décider de la suite du moteur (fumée, fluides).

Les rapports de recherche du projet montrent que la simulation est bornée par la **mémoire** : ce qui compte, c'est le nombre d'octets que la carte lit et écrit par seconde. Le banc mesure donc d'abord ce débit.

## Le lancer

| Depuis | Geste |
|---|---|
| L'application | `File › Help › Measure the graphics card` |
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

### 2 bis. La fumée des nœuds

`Node smoke` : un pas complet de la vraie fumée d'Aryadne ([Fumée et feu](simulation-fumee.md)), en 128³ et 256³. La pression y est résolue par multigrille (MGPCG), pas par Jacobi.

| Ligne | Sens |
|---|---|
| `N³ · X MB on the card` | La taille de la grille et la mémoire qu'elle occupe sur la carte |
| `2 iterations`, `4 iterations`, `8 iterations` | Le temps d'un pas avec ce nombre d'itérations de pression, et la divergence avant → après. La divergence est l'erreur qui reste : plus elle est petite, plus le mouvement est juste |
| `4 iterations, with each frame transferred for the cache` | Le même pas quand chaque image est recopiée vers le processeur. L'écart avec la ligne `4 iterations` est le prix du cache |

À comparer avec la ligne `Smoke step N³ (40 pressure iterations)` : même taille, 40 itérations de Jacobi. La multigrille doit donner une divergence bien plus petite pour un temps du même ordre.

### 3. Le tissu

`VBD cloth (one pass = every vertex updated once)`. Deux tailles : 250 000 et 1 million de sommets.

La première ligne mesure **une passe seule** :

| Valeur | Sens |
|---|---|
| `Vertices` | Nombre de points du tissu |
| `Colors` | Nombre de couleurs (voir [Simulation de tissu](simulation-tissu.md#ce-que-dit-laperçu)). Une grille avec diagonales en a 4 |
| `Per pass` | Temps d'une passe sur la carte |
| `Dispatches per pass` | Lancements de calcul par passe. Sur un petit tissu, ce sont eux qui coûtent : chaque lancement a un coût fixe |
| `Per vertex per pass` | Temps ramené à un sommet |
| `GB/s` | Débit mémoire pendant le calcul |

Puis des **images entières**, découpées comme le solveur le fait :

| Ligne | Sens |
|---|---|
| `One frame at a budget of 16 passes, without levels` | Beaucoup de sous-pas d'une passe |
| `... with coarse levels` | Moins de sous-pas, chacun avec sa correction d'ensemble. C'est le réglage du nœud |
| `One frame at a budget of 32 passes ...` | Les deux mêmes mesures, au double du budget |
| `... with levels and self-collision` | La même image avec l'auto-collision. La fin de la ligne donne **ce qu'elle coûte par sous-pas**, à côté du coût d'un sous-pas sans elle |

Chaque ligne donne le temps de l'image, le nombre de `substeps` et le nombre de `dispatches`.

Le budget compte tout ce que coûte un sous-pas : sa passe, l'amortissement des plis, la correction d'ensemble, l'auto-collision. À budget égal, une image avec niveaux fait donc moins de sous-pas, et une image avec auto-collision encore moins.

Le reste :

| Valeur | Sens |
|---|---|
| `Coarse levels` | Nombre de versions plus grossières du tissu, et coût d'une correction en passes |
| `At 24 frames per second` › `Passes per frame` | Combien de passes tiennent dans une image à 24 images par seconde |
| `Readback` | Temps pour relire les positions depuis la carte |
| `Displayed frame without readback` | Voir plus bas |
| `CPU` › `Per pass` | Le même calcul sur le processeur, pour comparer |
| `Setup` | Préparation au chargement : voisinages, couleurs, rangement, niveaux grossiers |
| `Memory on the card` | Mémoire occupée par ce tissu |

**L'image affichée.** Une ligne mesure ce que tu attends vraiment entre deux images de l'aperçu : un pas au budget de 16 passes, puis de quoi le montrer.

| Valeur | Sens |
|---|---|
| `Displayed frame without readback (16-pass step + direct draw)` | Le pas, puis le dessin du tissu en 1280 × 720 directement depuis la carte. Rien ne la quitte |
| `With the deferred transfer for the cache` | La même chose, plus la copie demandée pour le cache des images. C'est ce que fait l'aperçu. L'écart avec la valeur d'avant est ce que coûte ce cache |
| `With readback as before, not counting its drawing` | Le pas, puis la relecture attendue, comme Aryadne le faisait. Le dessin qui suivait n'est pas compté |
| `Draw only` | Le dessin seul : normales et rasterisation |
| `With shadows, textured material and selection outline` | Comme la première valeur, avec tout ce qu'a le tissu dans une scène 3D : l'ombre du soleil (reçue, avec une boule et un sol comme autres objets, et posée au sol), un matériau cuit en textures de 1024 × 1024 et le contour de sélection. Suivi du dessin seul dans ces conditions |

Sur une vraie carte, la première valeur doit être nettement plus petite que la troisième. Sur un rendu logiciel, le dessin lui-même est lent et la comparaison ne dit rien.

Le banc complet mesure ces lignes sur deux tissus : 250 000 et 1 000 000 de sommets. L'écart entre `Draw only` et le dessin complet est ce que coûtent les ombres, le matériau et la sélection. Il dépend surtout de la taille de l'image, peu du nombre de sommets, sauf pour la carte d'ombre du tissu, refaite à chaque image.

Si un tissu ne tient pas sur la carte : « Cloth of N vertices: it does not fit on this card, measured on the CPU ».

### 4. Raideur et contacts, à budget égal

`VBD, without and with coarse levels, against XPBD, at an equal number of passes`. Les trois méthodes reçoivent le même budget de calcul : 16, 32 puis 64 passes par image. Le banc compare ce qu'elles en font. Cette partie prend moins d'une minute, et tourne sur le processeur (la carte donne le même résultat).

| Essai | Ce qu'il dit |
|---|---|
| Tissu pendu | `Stiffness obtained` : la raideur réellement obtenue, en part de celle demandée. 100 % : le calcul a convergé. `Elongation` : de combien le tissu s'allonge sous son poids |
| `Falling on a ball` | `Worst edge` : le fil le plus étiré pendant la chute. `Mean` : l'étirement moyen à la fin. Plus c'est bas, plus les contacts sont propres |

Chaque ligne donne aussi le nombre de `substeps` et le `real cost` du découpage, en passes.

**Ce qu'il faut regarder.** La ligne `VBD + niveaux` doit donner une raideur bien plus haute que `VBD` seul, et une pire arête qui n'est pas plus mauvaise.

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
- **Relecture.** Elle ne pèse plus sur l'affichage quand Aryadne dessine avec la carte (voir [Réglages](reglages.md#affichage--carte-graphique-ou-mode-compatible)). Elle compte encore pour le rendu final, les exports et le mode compatible.

## Sans carte

Le banc a besoin d'une carte utilisable par wgpu (Vulkan, Metal ou DirectX 12). Sans elle, il s'arrête avec un message. Le tissu d'Aryadne, lui, passe alors sur le processeur.

Pour forcer le repli sur le processeur sans débrancher la carte (essai de développeur), lance Aryadne avec la variable d'environnement `ARY_SANS_GPU` définie.

## Limites

- Le banc mesure le calcul et le dessin du tissu par la carte. Le reste de la vue 3D d'Aryadne (les objets ordinaires) est dessiné par le processeur et n'est pas mesuré ici.
- Les chiffres varient avec la charge de la machine. Ferme les autres logiciels lourds et lance deux fois.
- Sur un rendu logiciel, le banc passe tout seul à de petites tailles : les chiffres ne se comparent pas à ceux d'une vraie carte.

Voir aussi : [Simulation sur carte graphique](../technique/simulation-gpu.md), [Feuille de route du moteur](../technique/feuille-de-route-moteur.md).
