---
titre: Aperçu, cache et erreurs
niveau: intermédiaire
duree: 15 min
prerequis: 12-noeuds/03-ranger-un-graphe
objectifs: Regarder la sortie d'un node seul ; Lire ce qui a été recalculé ; Provoquer et réparer trois erreurs courantes ; Savoir où un graphe est enregistré
---

## Ce que tu vas réussir

Tu vas apprendre à enquêter dans un graphe : voir ce qui sort de chaque node, comprendre ce qui est recalculé, et réparer une erreur en lisant son message.

```essayer
nouveau: arycomp
nom: enquete
```

## Construis un petit graphe

```etape
Pose **Noise**, **Color ramp**, **Blur** et **Vignette**, reliés dans cet ordre jusqu'à **Output**. Relie la sortie **Value** de **Noise** à **Factor**.
verifier: graphe.liens >= 4
```

## Trois façons de voir

**L'aperçu.** Le panneau **Preview and settings (nodes)** montre ce qui arrive à la sortie. Dessous, le nom et la catégorie du node choisi.

**Les miniatures.** Sous chaque node, une petite image montre ce qui en sort. P l'affiche ou la cache pour les nodes choisis.

**Un node seul.** Tu peux demander à l'aperçu de montrer un node précis.

```etape
Fais un clic droit sur **Color ramp**. Choisis **Preview node**. L'aperçu montre le dégradé net, avant le flou.
```

```etape
Refais un clic droit sur le même node. Choisis **Back to full material**. L'aperçu revient à la sortie.
```

```astuce
C'est le premier réflexe quand un résultat surprend : regarde chaque node de la chaîne, de gauche à droite, jusqu'à trouver celui où ça déraille.
```

## Rien ne se calcule pour rien

Chaque node porte une empreinte : son type, ses réglages, et l'empreinte de ce qui y entre. Deux nodes de même empreinte donnent le même résultat. Aryadne garde donc en cache tout ce qui n'a pas changé.

```etape
Mets **Radius** du **Blur** à 40. Le calcul prend un instant.
```

```etape
Bouge maintenant **Strength** de **Vignette**. La réponse est immédiate : le flou, en amont, n'est pas refait.
```

La règle : changer un node recalcule ce node et ce qui est **en aval**. Ce qui est **en amont** vient du cache.

Dans un graphe de géométrie, la ligne sous l'aperçu le dit en clair : tant de nodes **recomputed**, tant **from cache**.

```astuce
Place les nodes lourds, comme les grands flous, le plus tôt possible. Garde les réglages que tu retouches souvent pour la fin de la chaîne.
```

## Erreur 1 : plus de sortie

```etape
Choisis **Output** et supprime-le avec Suppr.
```

L'aperçu s'arrête. Dans le panneau de côté, un message marqué d'un triangle : « The output node is missing ».

```etape
Répare : Ctrl+Z, ou ajoute un node **Output** depuis la catégorie **Output** et rebranche-le.
verifier: graphe.contient = comp_out
```

## Erreur 2 : une formule refusée

```etape
Ajoute un node **Expression**. Écris la formule `sin(x * ` et laisse-la incomplète.
verifier: graphe.contient = expr
```

```etape
Relie **Expression** à **Factor** de **Color ramp**. Le graphe passe en erreur, et le message dit pourquoi.
```

```etape
Clique le message d'erreur dans le panneau de côté. Le node en cause est choisi et cadré.
```

```etape
Corrige la formule : `sin(x * 20) * 0.5 + 0.5`. L'aperçu revient, avec des bandes.
```

## Erreur 3 : un lien qui n'a pas de sens

Certains liens sont refusés dès que tu les tires.

- Une géométrie ne se branche que sur une géométrie.
- Un lien ne peut pas faire une boucle : un node ne peut pas dépendre de sa propre sortie.

```etape
Essaie de relier la sortie de **Vignette** à l'entrée de **Blur**, en amont. Le lien est refusé.
```

Certains nodes n'ont de sens que dans un genre de graphe. Un node qui lit un attribut de géométrie, posé dans un matériau, donne par exemple : « This node reads a geometry attribute: it belongs in a geometry graph ». Le menu d'ajout t'évite ces cas : il ne propose que les nodes qui conviennent.

## Enquêter avec M

```etape
Choisis **Blur** et appuie sur M. Le flou est sauté : tu vois le graphe sans lui. Appuie sur M de nouveau.
```

Muet, clic droit et **Preview node**, miniatures : trois outils pour cerner un problème sans rien casser.

## Demander de l'aide

Le bouton **Assistant**, ou Ctrl+L, ouvre l'Assistant à côté du graphe. Devant un graphe, il propose **Explain**, **Frame** et **Improve**. Il voit le graphe ouvert et peut le modifier : sa proposition arrive sur une carte, que tu appliques d'un clic. Un Ctrl+Z annule tout. Il lui faut un modèle branché.

## L'enregistrement

Un graphe est un fichier JSON lisible. Ctrl+S l'enregistre. Rien n'est caché dans une base de données : tu peux le versionner, le copier, le lire.

```etape
Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```quiz
Tu changes le dernier node d'une longue chaîne. Que recalcule Aryadne ?
- [ ] Toute la chaîne.
- [x] Ce node seulement : l'amont vient du cache.
- [ ] Rien avant le prochain enregistrement.
explication: Un changement ne touche que le node et son aval. Tout ce qui est en amont garde son empreinte.
```

```quiz
Un résultat te surprend. Quel est le premier geste ?
- [ ] Supprimer les nodes un par un.
- [x] Regarder la sortie de chaque node, avec Preview node ou les miniatures.
- [ ] Recréer le graphe.
explication: Tu remontes la chaîne jusqu'au node où le résultat cesse d'être celui que tu attends.
```

```defi
Provoque puis répare une quatrième erreur : une zone de répétition dont il manque un bout.
indice: Pose une Repeat zone, supprime son node de fin, et observe. Un début sans fin ne fait rien.
```

## En trois lignes

- **Preview node**, miniatures et M montrent ce que fait chaque node.
- Un changement ne recalcule que l'aval. Mets les nodes lourds en amont.
- Un message d'erreur se clique : il mène au node en cause.

Parcours terminé. Mets tes nodes au travail dans le [Compositing](../13-compositing/index.md) ou la [Simulation](../14-simulation/index.md).
