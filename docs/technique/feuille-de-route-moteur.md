# Feuille de route du moteur

Cette page résume cinq rapports de recherche sur le moteur d'affichage et de simulation d'Aryadne. Elle ne les recopie pas. Elle dit ce qu'ils concluent, ce que le code a déjà fait, et ce qui reste.

## Les rapports

Au moment où cette page est écrite, les rapports ne sont **pas dans le dépôt**. Ils se trouvent dans le dossier de travail `/home/claude/recherche/`. Pour les garder avec le projet, il faudrait les copier, par exemple dans `docs/recherche/`.

| Fichier | Sujet |
|---|---|
| `CONTEXTE.md` | Le but, l'état de départ, le plan en six étapes, les règles données aux auteurs |
| `1-apercu.md` | Aperçu interactif de très grosses simulations |
| `2-final-solveurs.md` | Résultat final de qualité production, solveurs et mathématiques |
| `3-fluides.md` | Fluides, fumée, feu, explosions, océans, sable, neige |
| `4-solides.md` | Tissu, cheveux, corps mous, muscles, rigides, destruction, contacts |
| `5-hors-piste.md` | Hors piste et architecture |

Chaque rapport suit le même plan : des fiches d'idées (principe, mathématiques, gain estimé, risque, difficulté de 1 à 5, sources), un classement des dix meilleures idées, un ordre d'implémentation, et une section de limites.

### Avec quelle confiance les lire

Les rapports le disent eux-mêmes, et il faut le garder en tête.

- **Presque aucun chiffre n'est mesuré.** Les temps sur carte graphique sont calculés par un modèle de bande passante, recalé sur quelques mesures publiées. Incertitude annoncée : un facteur 2.
- **Les comparaisons avec Houdini et Blender sont des extrapolations** à partir de rapports d'utilisateurs. Aucun banc public rigoureux n'a été trouvé à ces tailles.
- Plusieurs sources sont **citées de mémoire** et marquées comme telles.
- Les **idées originales** sont marquées « non validées ». « Non publié à ma connaissance » ne veut pas dire nouveau.
- Aucune recherche systématique de **brevets** n'a été faite. Une famille de brevets sur le raffinement du feu orienté caméra a été repérée par hasard.

C'est pour remplacer ces estimations par des mesures que le [banc de la carte graphique](../utilisateur/banc-gpu.md) existe.

## Le but

Un « Houdini dans le logiciel » : tissu, corps mous, cheveux, rigides, destruction, fluides, fumée, feu, sable, neige. Avec deux exigences à la fois :

1. un **aperçu interactif** de très grosses simulations ;
2. un **résultat final** de qualité production, calculé le plus vite possible.

## Le plan en six étapes, et où en est le code

| Étape du plan | État dans le code |
|---|---|
| 1. Séparer le tracé (buffer de visibilité « pixols ») et l'éclairage, avec cache | **Fait.** `Pixols`, `raster`, caches d'ombres par pixel, par sommet et du sol. Voir [Moteur 3D](moteur-3d.md) |
| 2. Petits triangles dessinés comme des points, plafond retiré | **Fait en partie.** Les triangles plus petits qu'un pixel sont posés comme des points. Le plafond est passé de 150 000 à 2 millions de triangles, pas retiré |
| 3. Carte graphique : buffer de visibilité, amas de triangles avec niveaux de détail, rasterisation en calcul | **Pas fait.** La vue reste sur le processeur. Seuls le tracé de chemins et les simulations utilisent la carte |
| 4. Simulation XPBD sur carte graphique (tissu d'abord), avec cache disque | **Fait, et dépassé.** Le socle `gpusim` existe. Le tissu est en VBD plutôt qu'en XPBD, comme le recommandaient les rapports. Le cache disque existe, sans compression |
| 5. Fluides FLIP et APIC, fumée et feu, grilles creuses | **Fumée et feu faits** en grille dense : MacCormack, MGPCG sans matrice, demi-précision, combustion, dessin direct (voir [Simulation sur carte graphique](simulation-gpu.md#la-fumée-et-le-feu)). Pas faits : grille creuse, montée en résolution, liquides |
| 6. Graphe de nœuds façon Houdini | **Commencé.** Graphe de géométrie avec évaluation à la demande, coupure anticipée, zones de simulation et trait `Solver`. Voir [Système de nœuds](noeuds.md) |

## Les idées qui reviennent dans tous les rapports

### 1. La mémoire borne tout, pas le calcul

Thèse commune aux cinq rapports. Une carte graphique a beaucoup plus de puissance de calcul que de débit mémoire. Le bon modèle de coût :

> temps = octets lus et écrits par élément × nombre d'éléments ÷ débit utile

Chaque décision se juge en **octets par élément**. Toute idée qui échange des octets contre du calcul gagne : état compressé, passes fusionnées, détail calculé au lieu d'être stocké.

**Dans le code.** C'est le principe écrit en tête de `gpusim.rs` et de `gpucloth.rs`. Le chantier en cours sur le tissu compte ses octets par sommet et par passe. Le banc mesure le débit réel.

### 2. Un seul état en colonnes

Un seul jeu de tampons, un par attribut, pour simuler, afficher, rendre et mettre en cache. Décision gratuite si elle est prise au début, très chère ensuite.

**Dans le code.** `geo::Geo` et `gpusim::Column` sont en colonnes. Ce qui manque : l'affichage direct de l'état du solveur. Aujourd'hui, l'état est relu de la carte vers le processeur pour être montré.

### 3. La loi du pixel

Un écran 1280 × 720 a moins d'un million de pixels. Un aperçu n'a jamais besoin de beaucoup plus d'éléments visibles que cela. Un milliard d'éléments est un problème de stockage et de résultat final, pas d'aperçu.

### 4. Le coût croît très vite avec la résolution

Pour une simulation en grille 3D : diviser la résolution par 2 divise le coût par 16. Par 4 : par 256. C'est le levier le plus fort, à condition que l'aperçu grossier ressemble au résultat fin.

### 5. Aperçu et final : le même solveur

L'aperçu simule un niveau grossier. Le final raffine en réutilisant le cache de l'aperçu. Deux réglages du même code, pas deux moteurs.

### 6. Le format de cache se décide tôt

Sinon le disque devient le goulot. Recommandations : ordre stable des éléments, quantification, différences d'une image à l'autre, compression sur la carte avant la relecture, lecture par niveaux de détail.

**Dans le code.** Le cache `.arygeocache` est simple : des colonnes en `f32` bruts, une image par fichier. Aucune de ces recommandations n'est encore appliquée.

## Rapport par rapport

### 1. Aperçu interactif

Trois thèses : la mémoire borne tout, la loi du pixel, le coût en N⁴.

Meilleures idées selon son classement :

1. un régulateur qui asservit la résolution de la simulation à l'écran, avec des niveaux progressifs ;
2. l'affichage de points par un noyau de calcul, et la surface d'un liquide en espace écran ;
3. un cache à niveaux de détail par préfixe, quantifié ;
4. des points de contrôle pour ne refaire que ce qui suit une retouche ;
5. la reprojection par la vitesse de simulation ;
6. un ordonnanceur qui montre toute la séquence en grossier avant de raffiner.

À ne pas faire maintenant, selon lui : les substituts neuronaux complets, la montée en résolution par réseau de neurones, le calcul sur plusieurs cartes.

### 2. Résultat final et solveurs

Le solveur de pression de référence : un gradient conjugué préconditionné par multigrille, sans matrice, sur des tuiles de 8³ cellules. Son nombre d'itérations ne dépend pas de la résolution.

Les plus gros leviers ne sont pas dans le solveur linéaire, mais dans la réduction du nombre d'inconnues à qualité égale : grilles creuses, bande étroite, cartes de flot.

Pour les solides : VBD puis sa variante augmentée, accélération de Tchebychev, sous-pas plutôt qu'itérations.

Son premier point d'ordre d'implémentation : **un banc de mesure de bande passante**, avec un noyau de copie et un calcul à 7 points. C'est exactement ce que fait `gpubench`.

### 3. Fluides

- Une fumée 512³ dense ne tient en mémoire que si les champs sont en demi-précision, ou si la grille est creuse.
- Le solveur de pression pèse 50 à 90 % du coût.
- 100 millions de particules ne tiennent que si chaque particule est compacte.

Trois leviers changent l'ordre de grandeur : la bande étroite, les grands pas de temps, la montée en résolution au rendu.

Ordre proposé : fumée dense d'abord (elle sert de banc), puis le solveur de pression, la demi-précision, la montée en résolution au rendu, la grille creuse, puis les liquides, l'océan, le sable et la neige.

### 4. Solides

Verdict : le XPBD sur carte graphique est un bon premier jalon et un mauvais point d'arrivée. Un solveur local propage l'information d'une maille par itération. La raideur visible dépend alors du budget d'itérations, pas du matériau.

La base recommandée : **VBD**, avec un contact par décalage géométrique pour l'absence de pénétration. Le vrai gain d'ordre de grandeur vient ensuite du multi-niveaux.

Ses premières idées au classement :

1. accélération de Tchebychev ;
2. positions relatives et tri de Morton ;
3. un champ de distance creux pour le corps et les obstacles ;
4. VBD comme cœur unique.

**Dans le code.** Les points 1, 2 et 4 sont faits dans `gpucloth.rs`. Le point 3 ne l'est pas : les obstacles sont un sol, une boule et une boîte.

Piège annoncé pour wgpu : le coût fixe de chaque lancement. Huit couleurs fois mille itérations font huit mille lancements par image. Le rangement par grappes a été essayé puis retiré (plus lent sur une vraie carte). Ce qui marche : moins de couleurs (quatre sur une grille), des niveaux grossiers en passes de Jacobi, et les petits niveaux traités d'un seul lancement. Voir [Simulation sur carte graphique](simulation-gpu.md).

### 5. Hors piste et architecture

Cinq conclusions :

1. la mémoire borne tout ;
2. un seul état en colonnes ;
3. le **cône d'influence** : ne calculer que ce que l'image demande, ne recalculer que ce qu'une retouche change. C'est l'idée la plus originale du rapport, validée sur un petit cas ;
4. un **cache d'ombrage par sommet** : les ombres ne dépendent pas de la caméra ;
5. wgpu offre plus que le plan ne le supposait, avec des pièges précis.

**Dans le code.**

- Le cache d'ombres par sommet est fait (`Prepared::vshadow`), ainsi que celui du sol.
- Les pièges de wgpu sont traités dans `gpusim` : limites réelles demandées à la carte, colonnes en pages, lancement sur deux dimensions.
- Le cône d'influence n'est pas fait.
- Le partage de la carte entre l'interface et le moteur est fait : l'interface dessine avec wgpu, et les simulations prennent son appareil (`gfx`, `gpusim`). Le tissu s'affiche depuis ses tampons, sans relecture (`gpudraw`).

Le rapport propose de faire **le graphe avant les fluides**, parce que la façon de mettre en cache et de recalculer conditionne l'écriture des solveurs de fluides.

## Ce qui reste, dans un ordre raisonnable

Synthèse des cinq ordres proposés, confrontée à l'état du code. C'est une lecture, pas une décision.

1. **Mesurer.** Lancer le banc sur la machine de travail, et recaler les rapports sur ces chiffres. Les rapports listent ce qui manque encore : débit de relecture, coût fixe d'un lancement, reproductibilité d'un noyau.
2. **Finir le tissu.** L'auto-collision entre sommets est faite ; restent l'auto-collision par triangles (épaisseur fine), la collision avec un personnage (champ de distance), le cache compressé.
3. **Afficher l'état du solveur sans relecture.** Fait pour les surfaces : aperçu de l'onglet Simulation, et géométries de nodes lourdes dans la vue d'une scène. Restent les points et les fils, et la vue 3D elle-même, toujours logicielle (les gros modèles n'en profitent pas encore).
4. **Fixer le format de cache** avant d'écrire d'autres solveurs.
5. **Fumée dense**, puis le solveur de pression, puis la grille creuse.
6. **Rigides et destruction, cheveux**, sur le même socle VBD.
7. **Liquides** en bande étroite, puis sable et neige.
8. **Aperçu grossier et final raffiné** partageant le même cache.
9. **Recherche** : cône d'influence, pas de temps locaux, bases réduites.

## Mettre cette page à jour

Quand une étape est faite :

1. change la ligne du tableau « où en est le code » ;
2. remplace l'estimation du rapport par la mesure du banc, avec la machine et la date ;
3. si une idée « originale, non validée » a été essayée, écris le résultat, bon ou mauvais.
