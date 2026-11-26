# Format des leçons « Apprendre »

Ce fichier fixe le format commun aux leçons. Le moteur d'Aryadne (panneau Apprendre) le lit, et les rédacteurs l'écrivent. Ne pas en sortir : un bloc inconnu s'affiche comme du texte.

## Arborescence

```
docs/apprendre/
  README.md                      sommaire général, ordre conseillé des parcours
  FORMAT.md                      ce fichier
  <nn>-<parcours>/               un dossier par parcours, numéroté (01-demarrer, 02-notes, ...)
    index.md                     présentation du parcours
    <nn>-<lecon>.md              une leçon (01-premiere-note.md, ...)
  fichiers/                      fichiers d'exercice fournis (facultatif, voir « essayer »)
```

## En-tête de chaque fichier (index.md et leçons)

Bloc YAML simple en tête, une clé par ligne, valeurs sur une ligne :

```
---
titre: Ta première simulation de tissu
niveau: débutant            (débutant, intermédiaire, avancé)
duree: 15 min
prerequis: 01-demarrer/02-l-interface, 08-noeuds/01-premier-graphe   (chemins sans .md, séparés par des virgules ; vide si aucun)
objectifs: Créer une zone de simulation ; Régler un solveur de tissu ; Lire le cache
---
```

## Corps

Markdown ordinaire (titres `##`, listes, gras, images relatives, liens relatifs vers d'autres leçons ou vers `../../utilisateur/*.md`). Phrases courtes, tutoiement, un geste par phrase. Les libellés d'interface s'écrivent en **gras**, tels qu'affichés en anglais (l'interface de Jules est en anglais), avec le français entre parenthèses la première fois si utile : **Add** (ajouter).

Blocs spéciaux, en blocs de code clôturés par trois accents graves, avec le mot-clé juste après :

### `essayer` : bouton « Essaie par toi-même »

Ouvre un point de départ dans le coffre, dans un dossier `Apprendre/` (copie : l'original n'est jamais touché). Une seule des trois sources :

```essayer
nouveau: arygeo                       (type de fichier vide : md, aryscene, arymat, arygeo, arycomp, aryimg, aryvec, aryseq, arymotion, aryaudio, arymusic, aryboard, arystory)
nom: drapeau                          (facultatif : nom du fichier créé)
```

```essayer
exemple: tissu sur une boule          (exemple intégré ; liste plus bas)
```

```essayer
fichier: fichiers/scene-depart.aryscene   (fichier fourni dans docs/apprendre/fichiers/)
```

### `etape` : une étape guidée, cochée automatiquement si possible

```etape
Ajoute un node **Solver** dans la zone de simulation.
verifier: graphe.contient = solveur
```

Le texte de l'étape est tout ce qui précède la ligne `verifier:`. Sans ligne `verifier:`, l'étape a une case à cocher à la main. Vérifications reconnues (n'en invente pas d'autres ; si rien ne convient, omets `verifier:`) :

| Vérification | Vraie quand |
|---|---|
| `onglet = <mode>` | le mode affiché est `notes`, `2d`, `3d`, `motion`, `video`, `son` ou `code` |
| `sous_onglet = <nom>` | en 3D : `scene`, `materiaux` ou `simulation` |
| `fichier.ouvert = <extension>` | le document actif a cette extension (`arygeo`, `md`, ...) |
| `fichier.enregistre` | le document actif n'a pas de modification non enregistrée |
| `graphe.contient = <op>` | le graphe actif (tout niveau) contient un node de cette opération (nom interne, voir la liste dans `docs/technique/noeuds.md` ou `src/nodes.rs` / `src/geo.rs`) |
| `graphe.noeuds >= <n>` | le graphe actif compte au moins n nodes |
| `graphe.liens >= <n>` | au moins n liens |
| `graphe.groupe` | le graphe contient au moins un groupe |
| `atelier.calques >= <n>` | le matériau actif a au moins n calques |
| `scene.objets >= <n>` | la scène 3D active a au moins n objets |
| `scene.selection` | un objet est choisi dans la scène 3D |
| `lecture.image >= <n>` | la tête de lecture (timeline 3D ou simulation) est à l'image n ou plus |
| `texte.contient = <mot>` | la note ou le fichier texte actif contient ce mot |
| `calques >= <n>` | l'image à calques ou le dessin vectoriel actif a au moins n calques |

Le moteur affiche les vérifications qu'il ne connaît pas comme une case à cocher.

Précisions du moteur :

- Les étapes vérifiées se cochent **dans l'ordre**. Une étape n'est regardée que si les étapes vérifiées d'avant sont faites. Écris donc les étapes dans l'ordre des gestes. Les étapes sans `verifier:` ne retiennent pas les suivantes.
- `graphe.contient` accepte le nom interne (`sim_solver`), sa fin (`solver`) ou le nom affiché (`Solveur`, `Solver`), sans tenir compte des majuscules ni des accents.
- `graphe.noeuds` et `graphe.liens` comptent tous les niveaux : la racine, l'intérieur des groupes, les graphes des calques d'un matériau.
- `fichier.enregistre` est faux pour un fichier sans titre (`untitled`) : il faut `Ctrl+S` pour lui donner sa place. Un fichier créé par `essayer` a déjà sa place.
- `sous_onglet` accepte aussi les outils des autres espaces : `moodboard`, `retouche`, `vectoriel`, `compositing`, `storyboard`, `montage`, `revue`, `audio`, `musique`, `notes`, `taches`, `cerveau`.
- `calques` compte, pour un dessin vectoriel, les formes et les groupes de premier niveau du panneau **Layers**.

### `quiz` : question à choix

```quiz
Que fait la zone de simulation ?
- [ ] Elle dessine le tissu plus vite.
- [x] Elle garde l'état d'une image à la suivante.
- [ ] Elle exporte la géométrie.
explication: Ce qui entre dans la zone repart de l'état de l'image précédente.
```

Une ou plusieurs bonnes réponses (`[x]`). La ligne `explication:` s'affiche après la réponse.

### `astuce`, `attention`, `raccourci`

```astuce
Maj+A ouvre la recherche de nodes sous la souris.
```

### `defi` : exercice libre en fin de leçon

```defi
Fais tomber le drapeau sur la boîte au lieu de la boule.
indice: Le node Solver a un réglage Box center.
```

## Exemples intégrés utilisables dans `essayer: exemple:`

Géométrie et simulation : `tissu sur une boule`, `tissu sur la carte graphique`, `tissu qui se replie`, `fontaine de particules`, `vagues animées`.

Matériaux (les préréglages de la fenêtre « nouveau fichier ») : `carrelage en relief`, `béton fissuré`, `métal brossé rayé`, `velours`, `cire`, `tissu tissé`, `métal peint usé`, `pierre aux creux sombres`, `bois`, `pierre`, `métal rouillé`, `toon`, `céramique`, `or`, `verre`, `verre dépoli`, `eau`, `toon 3 bandes`, `peinture de voiture`, `plastique`, `néon`.

Documents de départ : `scène de départ`, `morceau de départ`, `session audio de départ`, `dessin de départ`, `storyboard de départ`.

`nouveau:` accepte aussi un fichier de texte ou de code vide : `txt`, `py`, `js`, `html`, `css`, `json`, `glsl`, `wgsl`, `fountain`, `csv`…

## Progression

Le moteur garde dans `<coffre>/.app/apprendre.json` les étapes cochées, les quiz réussis et les leçons terminées. Une leçon est terminée quand toutes ses étapes sont cochées et tous ses quiz réussis.
