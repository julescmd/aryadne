---
titre: L'éditeur de courbes
niveau: intermédiaire
duree: 20 min
prerequis: 15-animation-rendu/01-cles-lissages-et-rythme
objectifs: Ouvrir l'éditeur de courbes ; Lire une courbe par canal ; Régler clés et tangentes ; Nettoyer une animation ; Faire tourner un mouvement en boucle
---

## Ce que tu vas réussir

Tu vas ouvrir le capot de ton animation. Chaque mouvement devient une courbe que tu peux lire, corriger et répéter à l'infini.

```essayer
nouveau: aryscene
nom: courbes
```

## Une animation à observer

```etape
Choisis le cube. À l'image 1, pose une clé avec I. À l'image 24, déplace-le sur le côté et monte-le, puis pose une clé. À l'image 48, ramène-le au sol plus loin, et pose une clé.
verifier: lecture.image >= 48
```

## Ouvre l'éditeur

```etape
Appuie sur Maj+F6. L'éditeur de courbes s'ouvre. Le bouton en forme de courbe de la timeline fait pareil.
```

```raccourci
Maj+F6 : ouvrir ou fermer l'éditeur de courbes.
```

Le bouton voisin, en forme de fenêtre, ouvre l'éditeur dans sa propre fenêtre. Tu peux la poser sur un second écran.

## Lis une courbe

Le temps va de gauche à droite. La valeur monte et descend. Il y a une courbe par canal : position, rotation et taille, chacune en X, Y et Z.

- Une courbe **plate** : rien ne bouge sur ce canal.
- Une courbe **raide** : ça va vite.
- Une courbe **douce** : ça va lentement.

```etape
Appuie sur F pour cadrer toutes les courbes. Repère celle de la hauteur : elle fait une bosse.
```

## Déplace une clé

| Geste | Effet |
|---|---|
| Clic, rectangle | Choisir des clés |
| Glisser une clé | La déplacer en temps et en valeur |
| Glisser une poignée | Changer la tangente |
| A | Tout choisir |
| F | Cadrer |
| I | Poser une clé à la tête de lecture, sur les courbes affichées |
| Suppr | Supprimer les clés choisies |
| Ctrl+C, Ctrl+V | Copier, coller à la tête de lecture |

```etape
Clique la clé du sommet de la bosse. Glisse-la vers le haut. Le cube saute plus haut, sans que tu touches à la vue.
```

```etape
Glisse-la vers la gauche. Le sommet arrive plus tôt : le saut devient dissymétrique.
```

Les champs **Key frame** et **Key value** donnent l'image et la valeur exactes de la clé choisie.

## Les tangentes

Une tangente règle la façon dont la courbe traverse une clé.

```etape
Choisis la clé du sommet. Tire une de ses poignées. La courbe se creuse d'un côté.
```

La barre d'outils propose des tangentes toutes faites.

| Bouton | Effet |
|---|---|
| **Auto tangents (flat at peaks)** | Douces, et plates aux sommets : pas de dépassement |
| **Spline tangents (smooth)** | Lisses, quitte à dépasser |
| **Linear tangents** | Des lignes droites |
| **Flat tangents** | Horizontales : arrêt marqué sur la clé |
| **Unify tangents** | Les deux côtés restent alignés |

```etape
Choisis toutes les clés avec A. Clique **Linear tangents**. Lis : le mouvement est sec, mécanique.
```

```etape
Clique **Auto tangents (flat at peaks)**. Le mouvement redevient souple.
```

**App easings** applique à la sélection un des lissages de la timeline : **Soft**, **Bounce**, **Spring**…

## Nettoyer

| Outil | Effet |
|---|---|
| **Snap the selected keys to whole frames** | Cale les clés sur des images entières |
| **Smooth the selected keys** | Moyenne avec les voisines |
| **Simplify** | Retire les clés que les voisines redonnent déjà |
| **Euler filter** | Corrige les sauts de 360 degrés dans les rotations |
| **Tweener** | Place les clés choisies entre leur voisine d'avant et celle d'après |

```etape
Choisis la clé du milieu. Bouge le curseur **Tweener**. À 0 %, la clé rejoint la pose d'avant. À 100 %, celle d'après.
```

Le **Tweener** fabrique vite une pose intermédiaire qui penche d'un côté ou de l'autre.

## Comparer

**Buffer curves** garde une copie des courbes. Tu modifies, tu compares. **Swap with the buffer curves** échange les deux versions.

**Normalize** affiche chaque courbe entre -1 et 1. Utile quand une rotation en degrés écrase une position en mètres.

## Avant et après l'animation

Que fait l'objet avant sa première clé et après la dernière ? Par défaut, il garde sa valeur. Tu peux le faire boucler.

| Choix | Effet |
|---|---|
| **Constant** | La valeur tient |
| **Cycle** | L'animation se répète |
| **Cycle with offset** | Elle se répète en repartant d'où elle est arrivée |
| **Oscillate** | Elle fait des allers-retours |

```etape
Dans l'onglet **Scene**, mets **End** à 144. Dans l'éditeur de courbes, règle **After the animation** sur **Cycle**. Lis.
```

Le cube refait son saut, puis revient d'un coup à son point de départ.

```etape
Passe sur **Cycle with offset**. Lis. Le cube enchaîne les sauts en avançant.
```

**Loop both sides** applique le même choix avant et après.

```etape
Ferme l'éditeur avec Maj+F6 et enregistre.
verifier: fichier.enregistre
```

```quiz
Que dit une portion de courbe très raide ?
- [ ] Que rien ne bouge.
- [x] Que la valeur change vite.
- [ ] Que la clé est verrouillée.
explication: La pente est la vitesse. Plate : immobile. Raide : rapide.
```

```quiz
Tu veux qu'un saut se répète en avançant à chaque fois. Quel réglage ?
- [ ] Cycle
- [x] Cycle with offset
- [ ] Oscillate
explication: Cycle revient au départ à chaque tour. Cycle with offset repart d'où le tour précédent est arrivé.
```

```defi
Fais tourner une roue sans fin, à vitesse constante, avec deux clés seulement.
indice: Une clé de rotation à 0, une à 360, des tangentes linéaires, et Cycle with offset après l'animation.
```

## En trois lignes

- Maj+F6 ouvre l'éditeur. Une courbe par canal. La pente est la vitesse.
- Les tangentes règlent la façon de traverser une clé.
- **Before the animation** et **After the animation** font boucler.

[Suite : Outils, poses et calques d'animation](03-outils-poses-calques.md)
