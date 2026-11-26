---
titre: Les sept pinceaux
niveau: intermédiaire
duree: 15 min
prerequis: 09-sculpture/01-preparer-et-premiers-traits
objectifs: Choisir le bon pinceau pour chaque geste ; Poser les grandes masses avec Grab ; Construire avec Clay ; Marquer un pli avec Pinch ; Sculpter en symétrie
---

## Ce que tu vas réussir

Tu vas sculpter une tête stylisée : un crâne, un nez, deux orbites, une bouche. Chaque étape te fait prendre un pinceau différent.

```essayer
nouveau: aryscene
nom: tete-stylisee
```

```etape
Prépare une boule d'argile comme à la leçon précédente : **Sphere**, **Remesh (voxels)**, **Apply**. Passe en mode **Sculpt**.
verifier: scene.selection
```

## Les sept pinceaux

| Pinceau | Touche | Effet |
|---|---|---|
| **Draw** | X | Pousse la surface vers l'extérieur |
| **Clay** | C | Ajoute de la matière à plat, comme des bandes d'argile |
| **Inflate** | I | Gonfle le long des normales |
| **Smooth (Shift)** | S | Adoucit |
| **Grab** | G | Attrape et tire un morceau |
| **Flatten** | T | Aplatit vers un plan moyen |
| **Pinch** | P | Resserre les sommets vers le centre du pinceau |

Survole une icône de la barre : son nom et sa touche s'affichent.

## La symétrie d'abord

```etape
Dans la barre, clique l'icône **X symmetry**. Chaque trait sera répété de l'autre côté.
```

Un visage est symétrique. Tu ne sculptes qu'une moitié.

## Grab : les grandes masses

```etape
Appuie sur G pour prendre **Grab**. Agrandis beaucoup le pinceau avec ].
```

```etape
Attrape le bas de la sphère et tire vers le bas : c'est la mâchoire. Attrape l'arrière et tire : c'est le crâne.
```

**Grab** ne crée rien. Il déplace. C'est l'outil des proportions : on commence toujours par lui.

## Clay : construire

```etape
Appuie sur C pour prendre **Clay**. Pose des bandes de matière au milieu du visage pour faire sortir le nez.
```

```etape
Ajoute de la matière sur l'arcade des sourcils et sur les pommettes.
```

**Clay** monte à plat, par couches. Il construit des volumes francs, là où **Draw** fait des boudins.

## Draw, inversé : creuser

```etape
Appuie sur X pour prendre **Draw**. Tiens Ctrl et creuse les deux orbites, sous l'arcade.
```

La symétrie creuse les deux d'un seul geste.

## Inflate : gonfler

```etape
Appuie sur I pour prendre **Inflate**. Passe sur le bout du nez et sur les joues.
```

**Inflate** pousse chaque point le long de sa normale. Il arrondit ce qui est déjà en volume. **Draw**, lui, pousse tout dans la même direction.

## Flatten : des plans

```etape
Appuie sur T pour prendre **Flatten**. Aplatis le front et l'arête du nez.
```

Un visage lisible est fait de plans. **Flatten** les pose, puis tu adoucis ce qu'il faut.

## Pinch : marquer

```etape
Appuie sur P pour prendre **Pinch**. Avec un petit pinceau, passe le long de la bouche que tu auras d'abord creusée d'un trait de **Draw** inversé.
```

**Pinch** resserre les sommets. Il rend un pli net, une arête vive, une commissure.

## Smooth : nettoyer

```etape
Tiens Maj et passe sur les bosses indésirables. Ou prends **Smooth (Shift)** avec S pour lisser longtemps.
```

```attention
Trop lisser efface tes formes. Lisse par petites touches, avec une force basse.
```

## L'ordre de travail

1. **Grab**, gros pinceau : les proportions.
2. **Clay** : les volumes.
3. **Draw** inversé : les creux.
4. **Flatten** : les plans.
5. **Pinch** : les arêtes et les plis.
6. Maj : nettoyer, tout du long.

```astuce
Si le pinceau semble « sauter », le maillage est trop grossier à cet endroit. Le pinceau trouve la surface par le sommet visible le plus proche du curseur.
```

```etape
Sors avec **Leave sculpting** et enregistre.
verifier: fichier.enregistre
```

```quiz
Par quel pinceau commence-t-on une sculpture ?
- [ ] Pinch, pour les détails.
- [x] Grab, pour poser les proportions.
- [ ] Smooth, pour partir d'une surface lisse.
explication: On va du grand au petit. Grab déplace de grandes masses sans rien ajouter.
```

```quiz
Quelle différence entre Draw et Inflate ?
- [ ] Aucune.
- [x] Draw pousse dans une direction, Inflate gonfle le long des normales.
- [ ] Inflate creuse, Draw ajoute.
explication: Inflate arrondit un volume existant. Draw pousse la zone d'un bloc.
```

```defi
Ajoute deux oreilles et un cou à ta tête.
indice: Grab avec un pinceau moyen pour tirer la matière, Clay pour épaissir, Pinch pour l'ourlet de l'oreille.
```

## En trois lignes

- **Grab** pour les proportions, **Clay** pour les volumes, **Pinch** pour les arêtes.
- Ctrl creuse, Maj lisse, avec tous les pinceaux.
- **X symmetry** sculpte les deux côtés d'un seul geste.

[Suite : La multirésolution](03-la-multiresolution.md)
