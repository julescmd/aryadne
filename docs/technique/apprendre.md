# Le mode Apprendre

Les leçons sont des pages Markdown de `docs/apprendre/`, embarquées dans le binaire. Un panneau les lit, vérifie les étapes sur l'état de l'application et garde la progression dans le coffre. Le guide pour l'utilisateur est dans [Apprendre](../utilisateur/apprendre.md). Le format des leçons est fixé dans [FORMAT.md](../apprendre/FORMAT.md).

## Les fichiers

| Fichier | Rôle |
|---|---|
| `build.rs` | Embarque `docs/**/*.md` dans `docs::FILES`, et tout autre fichier de `docs/apprendre/` (exercices, images) dans `docs::ASSETS` |
| `src/learn.rs` | La logique, sans interface : en-tête, blocs, catalogue, exemples, vérifications, progression. Tous les tests sont ici |
| `src/learn_ui.rs` | Le panneau (`Panel`) et ses pages, la lecture de l'état de l'application, les actions |
| `src/main.rs` | Le champ `App::learn`, la case `learn` de la disposition, l'icône du rail, le menu, l'accueil |
| `src/layout.rs` | `activate` (passer un onglet devant) et `widen` (donner une largeur lisible à la colonne) |

## Du fichier à l'écran

1. `learn::catalog()` range les pages `apprendre/<parcours>/<leçon>.md` en parcours (`Course`) et leçons (`LessonRef`), triés par nom. `index.md` donne l'en-tête du parcours. Le dossier `fichiers/` n'est pas un parcours.
2. `Lesson::parse` sépare l'en-tête (`split_front`) du corps, puis `parse_blocks` découpe le corps en `Block` : `Texte`, `Essayer`, `Etape`, `Quiz`, `Encadre`, `Defi`.
3. `Panel::lesson_page` dessine chaque bloc. Le texte passe par `preview::show_doc`, le même rendu que les notes en lecture.

Un bloc mal écrit ne casse rien : un bloc jamais refermé, un mot-clé inconnu, un quiz sans bonne réponse ou une étape vide restent du texte. Un `essayer` sans source valide affiche une carte « pas disponible » avec la raison.

## Les vérifications

`learn::Check` est la liste fermée des vérifications (la table est dans `docs/apprendre/FORMAT.md` ; les dernières ajoutées : `scene.simulations` et `scene.materiaux`, remplies par `Viewer::learn_scene_extra`). `parse_check` lit la ligne `verifier:`. Ce qui n'est pas reconnu devient `Check::Inconnue` : l'étape se coche alors à la main.

`Check::holds(&State)` dit si la vérification est vraie. `State` est une photo de l'application, prise par `App::learn_state` :

| Champ de `State` | Lu dans |
|---|---|
| `mode` | le genre de l'espace de travail (`workspace::Workspace::kind`) |
| `tool` | `App::cur_tool()` |
| `ext`, `saved` | le document actif ; `saved` est faux pour un fichier sans titre ou modifié |
| `graph` | `nodes::Editor::learn_state` : nodes et liens de tous les niveaux (racine, groupes, calques) |
| `mat_layers` | la pile de l'Atelier du matériau ouvert |
| `objects`, `selection`, `frame` | `model3d::Viewer::learn_state` ; `frame` vient aussi de la lecture d'une simulation |
| `text` | le texte du fichier ouvert |
| `layers` | les calques de l'image, ou les formes de premier niveau du dessin vectoriel |

`learn::newly_done` applique l'ordre : une étape vérifiée n'est regardée que si les étapes vérifiées d'avant sont faites. Les étapes à cocher à la main ne retiennent pas les suivantes. Une étape placée après un bloc `essayer` attend que son bouton ait servi (`Lesson::step_starts`). Le panneau appelle cette fonction à chaque image où il est affiché.

`GraphStats::contains` accepte le nom interne d'un node (`sim_solver`), sa fin (`solver`), ou son nom affiché en français ou en anglais.

### Ajouter une vérification

1. Ajoute une variante à `Check` et sa lecture dans `parse_check`.
2. Ajoute le champ qu'elle lit à `State`, et remplis-le dans `App::learn_state`.
3. Écris son cas dans `Check::holds`.
4. Ajoute une ligne au tableau de `FORMAT.md`, et un cas aux tests `lecture_des_verifications` et `chaque_verification_sur_un_etat_simule`.

## Les points de départ

`learn::start(coffre, source, leçon)` écrit le fichier dans `<coffre>/Apprendre/` et rend son chemin. Un fichier du même nom n'est jamais écrasé : la copie prend un numéro.

- `Source::Nouveau` : `write_new` écrit le document de départ du type demandé.
- `Source::Exemple` : `examples()` liste les exemples (ceux de `geo::EXAMPLES`, les préréglages de `nodes::MATERIAL_PRESETS`, et quelques documents de départ). `write_example` les écrit.
- `Source::Fichier` : `asset` retrouve le fichier embarqué, dans `ASSETS` ou, pour une note, dans `FILES`.

Le panneau ouvre ensuite le fichier avec `App::open`, qui change d'espace de travail au besoin. Le chemin créé est retenu dans la progression : le bouton rouvre le même fichier tant qu'il existe.

## La progression

`<coffre>/.app/apprendre.json`, lu et écrit par `learn::Progress` :

```json
{
  "version": 1,
  "lecons": {
    "01-demarrer/01-installer-et-lancer": {
      "etapes": [2, 3, 4],
      "quiz": [0],
      "terminee": true,
      "essais": { "0": "Apprendre/premiers-pas.md" }
    }
  },
  "derniere": "01-demarrer/01-installer-et-lancer",
  "accueil_propose": true,
  "panneau": true
}
```

- `etapes` et `quiz` : rangs des étapes cochées et des quiz réussis, comptés depuis 0 dans la leçon.
- `terminee` : posé par `update_done` quand tout est fait, ou par `finish` pour une leçon sans étape ni quiz. Il reste posé.
- `essais` : rang du bloc `essayer` → fichier créé.
- `derniere` : la dernière leçon ouverte. `resume` en tire la leçon à reprendre.
- `accueil_propose` : la carte du premier lancement a reçu une réponse.
- `panneau` : le panneau était ouvert.

L'absence du fichier marque le premier lancement du coffre (`Progress::fresh`).

## Le panneau dans la disposition

La case s'appelle `learn`. `App::tiles` l'ajoute à côté de l'inspecteur dans chaque disposition, et la montre quand `learn.open` est vrai. À l'ouverture, `layout::activate` passe son onglet devant et `layout::widen` donne 380 points à sa colonne. Comme les autres panneaux, il peut sortir dans sa propre fenêtre (`dock`).

## Tests

`cargo test --offline learn::` couvre l'analyse (tous les cas de `FORMAT.md`, blocs mal écrits), chaque vérification sur un état simulé, l'ordre des étapes, le catalogue, la progression et son fichier, la création des points de départ.

Deux tests lisent les vraies leçons du dépôt : `lecons_embarquees_lisibles` (blocs `essayer` valides, prérequis existants) et `verifications_des_lecons_connues` (aucune vérification inconnue, aucun node inconnu). Ils échouent quand une leçon sort du format.
