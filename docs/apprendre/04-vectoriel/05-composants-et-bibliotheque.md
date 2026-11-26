---
titre: Composants, bibliothèque et mise en page automatique
niveau: intermédiaire
duree: 15 min
prerequis: 04-vectoriel/04-texte-couleurs-variables
objectifs: Créer un composant et ses instances ; Faire une variante ; Réutiliser les composants d'un autre dessin ; Ranger des formes avec la mise en page automatique ; Fixer des contraintes
---

## Ce que tu vas réussir à faire

Tu vas dessiner un bouton une seule fois et le poser cinq fois. En changeant le maître, les cinq changent. Puis tu vas les aligner dans une barre qui se range toute seule.

```essayer
nouveau: aryvec
nom: boutons
```

```etape
Clique **Try it yourself**. Dessine un bouton : un rectangle arrondi et un texte dessus. Groupe-les avec `Ctrl+G`.
verifier: fichier.ouvert = aryvec
```

## Créer un composant

Un composant est une forme, ou un groupe, que tu réutilises. Ses copies sont des **instances**.

```etape
Choisis le groupe du bouton. Appuie sur `Ctrl+Alt+K` : **Create component**.
```

## Poser des instances

```etape
Tiens `Alt` et glisse le composant maître : tu poses une instance. Fais-le trois fois.
```

Le bouton **New instance** fait la même chose.

```etape
Dans le panneau **Layers (vector)**, déplie le composant maître et choisis son rectangle. Change sa couleur. Les trois instances changent.
```

| Action | Effet |
|---|---|
| **Create component** (`Ctrl+Alt+K`) | La sélection devient un composant maître |
| **New instance** | Pose une copie liée au maître |
| **Go to main component** | Depuis une instance, saute au maître |
| **Detach instance** | L'instance redevient un groupe ordinaire |
| **New variant** | Crée un composant frère : « Nom / Variante 2 » |

```etape
Choisis une instance. Clique **Go to main component** : la vue saute au maître.
```

## Une variante

Une variante est un composant frère. Elle sert aux états d'un même élément : normal, désactivé, danger.

```etape
Choisis le maître. Clique **New variant**. Donne à la variante une couleur grise : c'est le bouton désactivé.
```

## La bibliothèque

Le panneau **Library** montre les composants des autres dessins du coffre. Tu les poses dans le dessin ouvert.

```etape
Enregistre ce dessin avec `Ctrl+S`. Ouvre le panneau **Library**.
```

Dans un autre dessin `.aryvec` du coffre, tes boutons apparaîtront ici. Un clic pose une instance au milieu du plan de travail.

| Bouton | Effet |
|---|---|
| Relire | Relit les composants des dessins du coffre |
| **Update components from the library** | Met à jour les composants venus de la bibliothèque. Leurs instances suivent |

```astuce
Garde un dessin « kit » dans ton coffre, avec tous tes boutons, icônes et cartes. Chaque nouvelle maquette y puise par la bibliothèque.
```

## La mise en page automatique

**Auto layout** range des formes à la suite dans un cadre, comme dans Figma. Ajoute une forme : les autres se poussent.

```etape
Choisis les trois instances. Appuie sur `Maj+A`.
```

| Réglage | Choix |
|---|---|
| Le sens | **Horizontal**, **Vertical** |
| L'alignement | **Align start**, **Center**, **Align end** |
| **Gap** | L'écart entre les formes |
| **Horizontal padding**, **Vertical padding** | Les marges intérieures |
| **Background color**, **Corners** | Le fond et les coins du cadre |

```etape
Règle **Gap** sur 12 et les deux marges sur 16. Donne un fond clair au cadre.
```

```etape
Passe le sens sur **Vertical** : la barre devient un menu. Reviens à **Horizontal**.
```

## Les contraintes

Quand un plan de travail change de taille, chaque forme suit une règle.

| En largeur | En hauteur |
|---|---|
| **Left**, **Right**, **Left and right**, **Center**, **Scale** | **Top**, **Bottom**, **Top and bottom**, **Center**, **Scale** |

```etape
Choisis la barre de boutons. Règle sa contrainte de largeur sur **Left and right**. Élargis le plan de travail : la barre s'étire avec lui.
```

**Adapt…**, sur un plan de travail, fait une copie du plan dans un autre format. Les contraintes décident de la place de chaque forme.

## La grille de colonnes

Sur un plan de travail, **Column grid** affiche des colonnes qui guident et aimantent la mise en page : **Columns**, **Gutter**, **Margin**. Elles ne sont jamais exportées.

```quiz
Tu changes la couleur du composant maître. Que font ses instances ?
- [x] Elles changent toutes.
- [ ] Rien, elles sont indépendantes.
- [ ] Elles sont supprimées.
explication: Une instance suit son maître. Detach instance la libère.
```

```quiz
Que montre le panneau Library ?
- [ ] Les icônes Lucide.
- [x] Les composants des autres dessins du coffre.
- [ ] Les images du dossier refs.
explication: La bibliothèque réunit les composants de tous les dessins vectoriels du coffre. Les icônes Lucide ont leur propre bouton.
```

```defi
Monte une barre de navigation de téléphone : quatre icônes Lucide dans une mise en page automatique horizontale, sur un plan de travail au format iPhone.
indice: Trace le plan avec `F`, puis **Size…** et iPhone. Pose les icônes, choisis-les, `Maj+A`. Contrainte **Left and right** et **Bottom**.
```

## Récapitulatif

- `Ctrl+Alt+K` crée un composant. `Alt` + glisser pose une instance.
- **Library** réunit les composants de tous les dessins du coffre.
- `Maj+A` range la sélection en mise en page automatique. Les contraintes gèrent le changement de taille.

Leçon suivante : [Prototype : survol, liens et présentation](06-prototype-et-presentation.md)
