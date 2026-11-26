---
titre: Les groupes
niveau: intermédiaire
duree: 15 min
prerequis: 12-noeuds/03-ranger-un-graphe
objectifs: Réunir des nodes en un groupe ; Entrer et sortir avec Tab ; Ajouter des prises au groupe ; Réutiliser un groupe en plusieurs exemplaires
---

## Ce que tu vas réussir

Tu vas fabriquer ton propre node : un générateur de nuages, avec ses réglages. Tu vas le poser deux fois, puis le corriger une seule fois pour les deux.

```essayer
nouveau: arycomp
nom: groupes
```

## Construis la matière première

```etape
Pose un node **Noise**, puis un node **Color ramp**. Relie la sortie **Value** de **Noise** à **Factor**, puis **Color ramp** à **Output**.
verifier: graphe.liens >= 2
```

```etape
Sur **Noise**, mets **Scale** à 3 et **Detail** à 6. Tu as des nuages.
```

## Groupe

```etape
Choisis **Noise** et **Color ramp**. Appuie sur Ctrl+G.
verifier: graphe.groupe
```

Les deux nodes ont laissé place à un seul. Un message le confirme : Tab pour y entrer, Ctrl+Alt+G pour dégrouper.

Les liens qui entraient et sortaient de la sélection sont devenus les prises du groupe. Ici, une seule : la sortie de couleur.

```etape
Choisis le groupe et appuie sur F2. Nomme-le « nuages ».
```

## Entre et ressors

```etape
Le groupe choisi, appuie sur Tab. Tu es à l'intérieur.
```

La barre du graphe montre où tu es : **← Graph**, puis le nom du groupe. Tu retrouves tes deux nodes, et deux nodes spéciaux : **Group inputs** et **Group outputs**.

```etape
Appuie sur Tab de nouveau. Tu es revenu au graphe.
```

```raccourci
Tab : entrer dans le groupe choisi, ou en sortir.
```

## Donne des réglages au groupe

Ton groupe n'a aucune entrée. Tu ne peux rien régler sans l'ouvrir. Corrige ça.

```etape
Entre dans le groupe avec Tab. Clique le node **Group inputs**.
```

```etape
Tire un lien depuis l'anneau vide de **Group inputs**, marqué « new socket », jusqu'à l'entrée **Scale** de **Noise**. Une prise est créée.
```

Dans le panneau de côté, chaque prise a un nom, un type et une valeur de départ. Les types proposés : **Decimal number**, **Integer**, **True / false**, **Vector**, **Color**, **Geometry**.

```etape
Dans le panneau de côté, renomme la prise « taille ». Donne-lui 3 comme valeur de départ.
```

```etape
Sors avec Tab. Le node « nuages » a maintenant une entrée « taille », réglable sans l'ouvrir.
```

```astuce
**Add a socket**, dans le panneau de côté, ajoute aussi une prise. Relier à l'anneau vide est plus rapide : la prise prend le bon type toute seule.
```

## Réutilise

```etape
Ouvre le menu **Add** de la barre du graphe, puis **Structure**. Ton groupe « nuages » y figure. Clique-le.
verifier: graphe.noeuds >= 3
```

Tu as posé un second **exemplaire**. Ce n'est pas une copie indépendante.

```etape
Ajoute un node **Mix**. Relie un exemplaire à **A**, l'autre à **B**, puis **Mix** à **Output**.
verifier: graphe.contient = mix
```

```etape
Donne une « taille » de 2 à l'un et de 12 à l'autre. Deux échelles de nuages se mélangent.
```

## Corrige une fois, pour tous

```etape
Entre dans un des deux exemplaires avec Tab. Dans le dégradé de **Color ramp**, change une couleur. Sors.
```

Les deux exemplaires ont changé. Le panneau de côté le rappelait à l'intérieur : le groupe est posé deux fois, le modifier change tous ses exemplaires.

## Dégroupe

```etape
Choisis un exemplaire. Appuie sur Ctrl+Alt+G. Ses nodes reviennent dans le graphe. L'autre exemplaire reste un groupe.
```

## Ce qu'il faut savoir

- Un groupe vit dans son graphe. Copier-coller un node de groupe vers un autre graphe emporte le groupe avec lui.
- Deux groupes ne peuvent pas porter le même nom.
- Un groupe peut contenir d'autres groupes.
- Une zone se groupe en entier : il faut choisir ses deux bouts.

```quiz
Tu modifies l'intérieur d'un groupe posé trois fois. Que se passe-t-il ?
- [ ] Seul l'exemplaire ouvert change.
- [x] Les trois exemplaires changent.
- [ ] Les deux autres sont dégroupés.
explication: Un exemplaire n'est pas une copie. Le contenu du groupe est partagé.
```

```quiz
Comment régler un groupe sans l'ouvrir ?
- [ ] C'est impossible.
- [x] En lui donnant des prises d'entrée, par Group inputs.
- [ ] En le rendant muet.
explication: Chaque prise de Group inputs devient une entrée du node de groupe, réglable sur le node.
```

```defi
Ajoute au groupe une seconde entrée : la couleur des nuages.
indice: Dans le groupe, ajoute un Mix entre le dégradé et la sortie. Relie l'anneau vide de Group inputs à une de ses couleurs.
```

## En trois lignes

- Ctrl+G groupe, Tab entre et sort, Ctrl+Alt+G dégroupe.
- **Group inputs** et **Group outputs** portent les prises du groupe.
- Un groupe posé plusieurs fois se corrige une seule fois.

[Suite : Les zones de répétition](05-zones-de-repetition.md)
