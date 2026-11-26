---
titre: Le recalcul incrémental
niveau: avancé
duree: 10 min
prerequis: 13-compositing/03-filtres-et-masques, 12-noeuds/08-apercu-cache-erreurs
objectifs: Comprendre ce qui est recalculé après un changement ; Organiser un graphe pour qu'il reste rapide ; Faire varier un compositing dans le temps
---

## Ce que tu vas réussir

Tu vas sentir, la main sur le curseur, pourquoi un réglage répond tout de suite et un autre fait attendre. Puis tu rangeras ton graphe pour que tout réponde vite.

```essayer
nouveau: arycomp
nom: recalcul
```

## Monte une chaîne lourde

```etape
Pose **Noise**, puis **Blur**, puis **Glow**, puis **Hue / saturation**, reliés dans cet ordre jusqu'à **Output**.
verifier: graphe.liens >= 4
```

```etape
Mets **Radius** du **Blur** à 60. C'est un calcul lourd.
```

## L'empreinte

Chaque node porte une **empreinte**. Elle résume trois choses : son type, ses réglages, et l'empreinte de ce qui y entre.

Deux nodes de même empreinte donnent la même image. Aryadne garde donc en cache l'image de chaque node. Tant que l'empreinte ne change pas, l'image est reprise telle quelle.

## Sens-le

```etape
Bouge **Hue** sur **Hue / saturation**, le dernier node. La réponse est immédiate.
```

Le flou et le halo n'ont pas changé d'empreinte. Seul le dernier node est refait.

```etape
Bouge maintenant **Scale** sur **Noise**, le premier node. La réponse est plus lente.
```

L'empreinte du bruit a changé. Celle du flou en dépend, celle du halo aussi, et ainsi de suite. Toute la chaîne est refaite.

| Tu changes… | Est recalculé… |
|---|---|
| Le premier node | Toute la chaîne |
| Un node du milieu | Ce node et tout ce qui le suit |
| Le dernier node | Lui seul |

## Trois règles pour rester rapide

**1. Le lourd d'abord, le fin ensuite.** Place les grands flous tôt. Garde pour la fin ce que tu retouches sans cesse : la couleur, le grain.

**2. Une branche par idée.** Une branche que tu ne touches pas reste en cache.

```etape
Ajoute un node **Mix** avant la sortie. Relie la chaîne à **A**. Pose un second **Noise**, relié à **B**.
verifier: graphe.contient = mix
```

```etape
Bouge **Scale** du second **Noise**. La première chaîne, avec son flou lourd, n'est pas refaite.
```

**3. Fais tes essais sur du léger.** Rends muet un node lourd avec M pendant que tu règles autre chose. Rallume-le à la fin.

```astuce
Un flou de grand rayon sur une image 4K prend du temps : le calcul se fait sur le processeur. Règle sur une image plus petite, puis remets la grande.
```

## Dans le temps

Le node **Time** donne l'image courante par **Frame** et le temps par **Seconds**. Un compositing peut donc varier d'une image à l'autre.

```etape
Ajoute un node **Time**. Relie sa sortie **Seconds** à l'entrée **Hue** de **Hue / saturation**.
verifier: graphe.contient = time
```

Dès qu'un graphe lit le temps, un champ **Frame** apparaît à droite de la barre du graphe. Il choisit l'image montrée par l'aperçu.

```etape
Glisse sur le champ **Frame** de la barre. L'aperçu change d'image, et la teinte tourne avec le temps.
```

```attention
Un node relié au temps change d'empreinte à chaque image. Lui et tout son aval sont refaits. Place ce qui dépend du temps le plus tard possible dans la chaîne.
```

## Le même principe ailleurs

Cette idée traverse tout Aryadne.

- En **géométrie**, la ligne sous l'aperçu compte les nodes **recomputed** et ceux repris **from cache**.
- En **simulation**, changer un node après la zone ne recalcule pas la simulation. Voir [Vent, lecture et cache](../14-simulation/06-vent-lecture-et-cache.md).

```etape
Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```quiz
Tu changes un node au milieu d'une chaîne. Qu'est-ce qui est recalculé ?
- [ ] Toute la chaîne.
- [x] Ce node et ce qui le suit.
- [ ] Ce node seulement.
explication: L'amont garde son empreinte, donc son image en cache. L'aval dépend du node changé.
```

```quiz
Où placer un flou lourd pour garder un graphe réactif ?
- [x] Tôt dans la chaîne, avant ce que tu retouches souvent.
- [ ] Juste avant la sortie.
- [ ] Dans un groupe.
explication: Tant que tu ne touches qu'à ce qui le suit, son image reste en cache.
```

```defi
Réorganise un de tes graphes précédents pour que la retouche de couleur soit instantanée.
indice: Repère le node le plus lent avec M. Fais passer la couleur après lui, ou sur une branche à part.
```

## En trois lignes

- Chaque node a une empreinte. Même empreinte, même image, reprise du cache.
- Un changement recalcule le node et son aval, jamais son amont.
- Le lourd d'abord, le fin ensuite, le temps en dernier.

Parcours terminé. Mets le compositing au service d'une image complète dans les [projets guidés](../16-projets/index.md).
