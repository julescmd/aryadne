---
titre: Sélectionner et transformer
niveau: débutant
duree: 15 min
prerequis: 07-3d-bases/01-ta-premiere-scene
objectifs: Sélectionner un ou plusieurs objets ; Déplacer, tourner et redimensionner au clavier ; Utiliser les poignées ; Entrer des valeurs exactes ; Changer de point de pivot
---

## Ce que tu vas réussir

Tu vas placer un cube exactement où tu veux, à la main puis au chiffre près. Tu connaîtras les trois touches les plus utiles de la 3D : G, R et S.

```essayer
nouveau: aryscene
nom: transformer
```

## Sélectionner

| Geste | Effet |
|---|---|
| Clic | Choisir un objet |
| Maj+clic | Ajouter à la sélection |
| Glisser dans le vide | Rectangle de sélection |
| A | Tout |
| Alt+A | Rien |
| Ctrl+I | Inverser |

```etape
Clique le cube. Il est entouré : c'est l'objet choisi.
verifier: scene.selection
```

```etape
Tiens Maj et clique la lumière. Deux objets sont choisis. Le dernier cliqué est l'objet **actif**.
```

```etape
Appuie sur Alt+A pour ne rien choisir. Puis reclique le cube.
verifier: scene.selection
```

Tu peux aussi cliquer un nom dans le panneau **Scene**. C'est pratique quand un objet est caché derrière un autre.

## Au clavier : G, R, S

Trois lettres.

| Touche | Action |
|---|---|
| G | Déplacer |
| R | Tourner |
| S | Changer la taille |

La marche est toujours la même.

1. Appuie sur la lettre.
2. Bouge la souris.
3. Appuie sur X, Y ou Z pour imposer un axe.
4. Clic ou Entrée valide. Clic droit ou Échap annule.

```etape
Appuie sur G, bouge la souris, puis clique. Le cube a bougé.
```

```etape
Appuie sur G, puis sur X. Le cube ne glisse plus que le long de l'axe X. Clique pour valider.
```

```etape
Appuie sur R, puis sur Y. Tourne le cube d'un quart de tour environ. Valide.
```

```etape
Appuie sur S, bouge la souris pour doubler la taille. Valide.
```

```etape
Appuie sur G, bouge la souris, puis appuie sur Échap. Le cube revient à sa place : rien n'a changé.
```

## Avec les poignées

```etape
Dans la barre de la vue, clique l'outil **Move**. Des flèches apparaissent sur le cube.
```

- Tire une flèche : un seul axe.
- Tire un petit plan entre deux flèches : deux axes.
- Tire le centre : déplacement libre.

```etape
Clique l'outil **Rotate**. Des anneaux remplacent les flèches. Tire un anneau.
```

```etape
Clique l'outil **Scale**. Tire un des petits cubes.
```

Clavier ou poignées : c'est une affaire de goût. Le clavier va plus vite. Les poignées se voient mieux.

## Au chiffre près

```etape
Dans **Properties**, ouvre l'onglet **Object**. Trouve la section **Transform**.
```

Elle donne **Location**, **Rotation** en degrés, et **Scale**.

```etape
Tape 0 dans les trois cases de **Location**. Le cube revient à l'origine.
```

```etape
Clique le nom de la ligne **Rotation**. La rotation est remise à zéro d'un coup.
```

```etape
Coche **Uniform** près de **Scale**, puis tape 1. Les trois axes suivent.
```

```astuce
Un clic droit sur un objet propose **Reset (location, rotation, scale)** : tout revient à zéro en un geste.
```

## Le point de pivot

Quand tu tournes plusieurs objets, autour de quoi tournent-ils ? C'est le pivot.

```etape
Appuie sur la touche point. Un menu circulaire s'ouvre : **Median point**, **3D cursor**, **Individual origins**, **Active element**.
```

| Pivot | Les objets tournent autour… |
|---|---|
| **Median point** | Du milieu de la sélection |
| **3D cursor** | Du curseur 3D |
| **Individual origins** | Chacun de son propre centre |
| **Active element** | De l'objet actif |

Le curseur 3D est un repère que tu poses où tu veux. Maj+S ouvre son menu : **Cursor to selected**, **Selection to cursor**, **Cursor to world origin**.

## Annuler

Ctrl+Z annule le dernier geste. Ctrl+Maj+Z le rétablit.

```quiz
Tu appuies sur G, puis sur Z. Que se passe-t-il ?
- [ ] L'objet tourne autour de Z.
- [x] L'objet ne se déplace plus que le long de l'axe Z.
- [ ] L'objet est supprimé.
explication: G déplace. La lettre d'axe qui suit impose cet axe.
```

```quiz
Comment annuler une transformation en cours ?
- [ ] Ctrl+S
- [x] Échap ou clic droit.
- [ ] Entrée
explication: Entrée et le clic valident. Échap et le clic droit annulent, et l'objet revient à sa place.
```

```defi
Pose le cube à 2 unités de haut, tourné de 45 degrés sur l'axe vertical, à moitié de sa taille.
indice: Tout se tape dans Transform. Vérifie quel axe est vertical en déplaçant d'abord le cube avec G puis une lettre d'axe.
```

## En trois lignes

- G déplace, R tourne, S redimensionne. X, Y ou Z impose un axe.
- Les outils **Move**, **Rotate**, **Scale** font pareil avec des poignées.
- **Transform**, dans l'onglet **Object**, donne les valeurs exactes.

[Suite : Ajouter et organiser des objets](03-ajouter-et-organiser.md)
