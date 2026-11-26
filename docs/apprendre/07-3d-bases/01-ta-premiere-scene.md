---
titre: Ta première scène
niveau: débutant
duree: 15 min
prerequis:
objectifs: Reconnaître la vue, la barre et les panneaux ; Tourner, déplacer et zoomer ; Passer d'une vue à l'autre ; Cadrer la scène
---

## Ce que tu vas réussir

Tu vas ouvrir une scène et t'y promener sans te perdre. À la fin, tu sais tourner autour d'un objet, le regarder de face, et retrouver la scène quand tu t'es égaré.

```essayer
nouveau: aryscene
nom: premiere-scene
```

```etape
Vérifie que la scène est ouverte dans l'outil **Scene** de l'étape **3D**.
verifier: sous_onglet = scene
```

La scène de départ contient trois objets : un cube, une lumière et une caméra.

```etape
Compte-les dans le panneau **Scene**, qui liste les objets.
verifier: scene.objets >= 3
```

## L'écran

| Zone | Rôle |
|---|---|
| La vue | La scène en 3D |
| La barre de la vue | Le mode, les menus, les outils |
| Le panneau **Scene** | La liste des objets. L'œil affiche ou cache |
| Le panneau **Properties** | Les réglages de l'objet choisi, en onglets |
| Le panneau **3D timeline** | Le temps, sous la vue |

La barre de la vue se lit de gauche à droite : le mode (**Object mode**), puis **View**, **Select**, **Add**, **Object**, les outils **Move**, **Rotate**, **Scale**, puis **Render**, **Nodes** et **Modifiers**.

## Tourner, déplacer, zoomer

| Geste | Effet |
|---|---|
| Molette enfoncée + glisser | Tourner autour de la scène |
| Maj + molette enfoncée | Déplacer la vue |
| Molette | Zoom |
| Alt + clic gauche glissé | Tourner, sans bouton du milieu |

```etape
Enfonce la molette et glisse. Fais un tour complet du cube.
```

```etape
Tiens Maj, enfonce la molette et glisse. La vue se déplace de côté.
```

```etape
Tourne la molette pour t'approcher du cube, puis t'en éloigner.
```

```astuce
Sur un portable sans bouton du milieu, Alt + clic gauche glissé tourne la vue. Tu peux aussi choisir la navigation façon Maya dans les réglages.
```

## Les vues toutes faites

| Touche | Vue |
|---|---|
| 1 | Face |
| 3 | Droite |
| 7 | Dessus |
| Maj+1, Maj+3, Maj+7 | Les vues opposées |
| 5 | Perspective ou orthographique |
| 0 | Par la caméra de la scène |

```etape
Appuie sur 1, puis 3, puis 7. Tu vois le cube de face, de droite, de dessus.
```

```etape
Appuie sur 5. La perspective disparaît : c'est la vue orthographique, utile pour aligner. Appuie encore sur 5.
```

```etape
Appuie sur 0. Tu regardes par la caméra. Tourne la vue pour en sortir.
```

Le menu **View** de la barre propose les mêmes vues, plus **Turntable**, une rotation automatique.

## Perdu ? Cadre

```etape
Zoome très loin, jusqu'à ne plus rien voir. Appuie sur Origine. Toute la scène revient dans la vue.
```

```etape
Clique le cube, puis appuie sur Maj+F. La vue se cadre sur lui.
verifier: scene.selection
```

```raccourci
Origine : tout cadrer. Maj+F : cadrer la sélection.
```

## La barre du bas

En bas de la fenêtre, une ligne rappelle les raccourcis utiles de l'outil en cours. C'est le premier endroit où regarder quand tu cherches un geste.

## Enregistre

```etape
Enregistre la scène avec Ctrl+S.
verifier: fichier.enregistre
```

Une scène ne contient pas les maillages. Elle pointe vers des fichiers de modèles, qui restent à leur place dans le coffre.

```quiz
Tu ne vois plus rien dans la vue. Quelle touche ramène toute la scène ?
- [ ] 0
- [x] Origine
- [ ] 5
explication: Origine cadre tout. 0 regarde par la caméra, 5 bascule entre perspective et orthographique.
```

```quiz
Comment tourner autour de la scène ?
- [x] Molette enfoncée, puis glisser.
- [ ] Clic gauche glissé.
- [ ] Maj + molette.
explication: Le clic gauche sert à sélectionner. Maj + molette enfoncée déplace la vue de côté.
```

```defi
Place ta vue exactement au-dessus du cube, sans perspective, cadrée sur lui.
indice: 7, puis 5, puis clique le cube et Maj+F.
```

## En trois lignes

- Molette enfoncée : tourner. Maj en plus : déplacer. Molette : zoom.
- 1, 3, 7 donnent les vues de face, de droite, de dessus. 0 regarde par la caméra.
- Origine cadre tout. Maj+F cadre la sélection.

[Suite : Sélectionner et transformer](02-selectionner-et-transformer.md)
