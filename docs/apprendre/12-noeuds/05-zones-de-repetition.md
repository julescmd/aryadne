---
titre: Les zones de répétition
niveau: intermédiaire
duree: 15 min
prerequis: 12-noeuds/04-les-groupes
objectifs: Poser une zone de répétition ; Régler le nombre d'itérations ; Comprendre l'état qui passe d'un tour au suivant ; Connaître les règles des zones
---

## Ce que tu vas réussir

Tu vas appliquer le même effet cinq fois de suite, sans poser cinq nodes. Puis tu changeras ce nombre d'un seul réglage.

```essayer
nouveau: arycomp
nom: repetition
```

## Le problème

```etape
Pose un node **Checker**, puis un node **Blur**. Relie **Checker** à **Blur**, puis **Blur** à **Output**. Mets **Radius** du flou à 3.
verifier: graphe.liens >= 2
```

Le damier est un peu flou. Pour le flouter davantage, tu pourrais dupliquer **Blur** quatre fois. Le jour où tu veux huit passes, tu recommences. Une zone règle ça.

## Pose la zone

```etape
Ouvre **Add**, puis **Structure**, puis **Repeat zone**. Deux nodes arrivent : le début de la zone et **Repeat end**.
verifier: graphe.contient = repeat_in
```

Une zone est délimitée par ces deux nodes. Ce qui est branché **entre les deux** s'exécute plusieurs fois.

## Mets le flou dans la zone

```etape
Coupe les deux liens du **Blur** avec Ctrl+clic.
```

```etape
Relie **Checker** à l'entrée du début de la zone.
```

```etape
Relie la sortie du début de la zone à **Blur**. Relie **Blur** à l'entrée de **Repeat end**.
```

```etape
Relie la sortie de **Repeat end** à **Output**.
verifier: graphe.liens >= 4
```

La chaîne se lit ainsi : le damier entre dans la zone, passe dans le flou, ressort, et arrive à la sortie.

## Règle les itérations

```etape
Sur le node de début, mets **Iterations** à 5. Le damier est flouté cinq fois de suite.
```

```etape
Mets **Iterations** à 1, puis à 10. Compare.
```

## L'état de la répétition

Ce qui passe d'un tour au suivant s'appelle l'**état**. Au premier tour, il vient de ce qui est branché à l'entrée du début. Aux tours suivants, il vient de ce qui est arrivé à la fin du tour d'avant.

```etape
Clique le node de début. Dans le panneau de côté, lis la carte **REPEAT STATE**.
```

Par défaut, l'état porte une seule valeur. **Add a socket** en ajoute d'autres : par exemple un compteur, à côté de l'image.

## Un effet qui grandit à chaque tour

```etape
Dans la zone, ajoute un node **Brightness / contrast** entre **Blur** et **Repeat end**. Mets **Contrast** à 0,3.
verifier: graphe.contient = brightcon
```

À chaque tour, l'image est floutée puis contrastée. Les cases du damier s'arrondissent : tu viens de fabriquer un effet que ni le flou ni le contraste ne donnent seuls.

## Les règles

- Le nombre d'itérations est un nombre fixe, réglé sur le node. Il ne peut pas venir d'un lien.
- Des zones de répétition peuvent s'emboîter. Elles ne peuvent pas se croiser.
- Une répétition qui donnerait un graphe trop gros est refusée, avec un message.
- Un début de zone sans sa fin ne fait rien.

```attention
Avant le calcul, la zone est déroulée : son contenu est copié autant de fois qu'il y a d'itérations. Dix itérations d'un flou lourd coûtent dix flous.
```

## Et la zone de simulation ?

Elle ressemble à une zone de répétition, avec une différence : son contenu s'exécute **une fois par image**, et l'état passe d'une image à la suivante. Elle n'existe que dans un graphe de géométrie. Voir [Ta première simulation](../14-simulation/02-premiere-simulation.md).

Une zone de simulation ne peut pas se trouver dans une répétition. L'inverse est permis.

```quiz
D'où vient l'état au troisième tour d'une zone de répétition ?
- [ ] De l'entrée du début de la zone.
- [x] De ce qui est arrivé à la fin du deuxième tour.
- [ ] D'une valeur par défaut.
explication: Seul le premier tour lit l'entrée du début. Ensuite chaque tour reprend le résultat du précédent.
```

```quiz
Peux-tu relier un node Value à Iterations pour le piloter ?
- [ ] Oui, comme toute entrée.
- [x] Non : le nombre d'itérations doit être fixe, réglé sur le node.
- [ ] Oui, mais seulement dans un groupe.
explication: La zone est déroulée avant le calcul. Il faut donc connaître le nombre de tours à l'avance.
```

```defi
Range ta zone dans un groupe nommé « flou doux », réutilisable.
indice: Une zone se groupe en entier. Choisis ses deux bouts et tout ce qu'il y a entre eux, puis Ctrl+G.
```

## En trois lignes

- Une zone a deux bouts. Ce qui est entre eux se répète.
- **Iterations** se règle sur le node de début, sans lien.
- L'état passe d'un tour au suivant. **Add a socket** l'étend.

[Suite : Math et logique](06-math-et-logique.md)
