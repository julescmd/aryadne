# Traductions

L'interface existe en français et en anglais. Le module est `src/i18n.rs`. Il est court : une table et deux fonctions.

## Le principe

**Le français est la langue source.** Le texte français est écrit dans le code. Il sert lui-même de clé. Les autres langues sont des tables qui associent le texte français à sa traduction.

```
assets/i18n/en.json
{
  "fermer": "close",
  "nouvelle note": "new note",
  …
}
```

Un texte absent de la table **reste en français**. Rien ne plante.

La table anglaise compte environ 4 000 entrées. Elle est intégrée à l'exécutable à la compilation (`include_str!`).

## Les deux fonctions

| Fonction | Usage | Exemple |
|---|---|---|
| `crate::i18n::t(texte)` | Un libellé : bouton, titre, entrée de menu. Met la **première lettre en majuscule** | `t("fermer")` rend « Fermer » ou « Close » |
| `crate::i18n::t_mid(texte)` | Un morceau **au milieu d'une phrase**. Pas de majuscule | `format!("3 {}", t_mid("fichier(s) renommé(s)"))` |

La majuscule de `t` saute les symboles du début : `t("✚ ouvrir")` rend « ✚ Ouvrir ».

Conséquence pour la table : **les traductions s'écrivent en minuscules**, comme les clés. C'est `t` qui met la majuscule.

```rust
if ui.button(crate::i18n::t("créer")).clicked() { … }
ui.label(format!("{} {}", count, crate::i18n::t_mid("images figées sur le disque.")));
```

Autres fonctions :

| Fonction | Rôle |
|---|---|
| `i18n::set(code)` | Choisit la langue : `"fr"` ou `"en"` |
| `i18n::current()` | La langue en cours |
| `i18n::en(texte)` | La traduction anglaise quelle que soit la langue affichée. Sert à la recherche : la palette de commandes trouve une commande par son nom français ou anglais |
| `i18n::sentence(texte)` | Met la première lettre en majuscule |

`i18n::LANGS` liste les langues : `[("fr", "Français"), ("en", "English")]`.

## Règle de la maison

**Toute chaîne visible passe par `t` ou `t_mid`.**

Certaines fonctions d'interface traduisent elles-mêmes leur libellé. Tu leur passes le texte français brut :

- `crate::modifiers::row(ui, "nom", …)` et `row_slider(ui, "opacité", …)` ;
- les tables de définitions : `nodes::DEFS` (libellés, catégories, noms de prises, choix), `keymap::ACTIONS`, `formats::ALL`, `workspace::tools`, les listes d'outils (`paint::TOOLS`, `vector_ui::TOOLS`, `sculpt::BRUSHES`…).

Dans ces tables, le texte est en français et traduit au moment de l'affichage.

## Ajouter une traduction

1. Écris le texte français dans le code, à travers `t` ou `t_mid`.
2. Ajoute une ligne dans `assets/i18n/en.json` : la clé est le texte français **exact**, caractère pour caractère.
3. Recompile. La table est intégrée à l'exécutable.

### Pièges

- **La clé doit être identique.** Une espace en trop, une apostrophe différente, et la traduction n'est pas trouvée.
- **Un texte construit par `format!`** ne peut pas être une clé. Traduis les morceaux fixes :
  ```rust
  // Non : la clé change à chaque valeur
  t(&format!("copie de {} Mo", n))
  // Oui : la clé est fixe, les accolades sont remplies après
  t("copie de {} Mo").replace("{}", &n.to_string())
  ```
  Le banc de la carte graphique utilise cette forme : la clé `"copie de {} Mo"` contient les accolades.
- **Un même mot français, deux sens.** La table n'a qu'une traduction par clé. Exemples réels :
  - `"image"` est traduit par « frame ». C'est juste dans une timeline, moins dans un nœud d'image : la prise « image » d'un nœud de compositing s'affiche `Frame` ;
  - `"V"` est traduit par « G » : la lettre du canal vert. Elle ressort ainsi partout où `"V"` passe par la table ;
  - `"plan"` est traduit par « plane », alors que le storyboard l'emploie pour un plan de cinéma.
  
  Pour lever une ambiguïté, change le texte français d'un des deux usages (« image du film », « plan de caméra »).
- **Les valeurs enregistrées dans les fichiers ne se traduisent pas.** Un choix de nœud est écrit en français dans le fichier (`"multiplier"`). Il est traduit seulement à l'affichage. Ne compare jamais une valeur lue dans un fichier à un texte traduit.

## Trouver ce qui manque

Un texte sans entrée dans la table reste en français dans l'interface anglaise. Pour les repérer, cherche les appels à `t` et `t_mid` dont l'argument n'est pas une clé de `en.json`.

Pendant le développement, deux petits scripts Python ont servi à cela. **Ils ne sont pas dans le dépôt**, et ils écrivent dans un chemin fixe : à relire avant de s'en servir.

| Script | Rôle |
|---|---|
| `add_i18n.py` | Lit sur son entrée un objet JSON `{"clé française": "english"}` et l'ajoute à `en.json`, sans écraser l'existant |
| `missing_i18n.py` | Liste les textes passés à `t` ou `t_mid` qui n'ont pas d'entrée. Il ne voit pas les textes des tables de définitions |

### Manques relevés en écrivant cette documentation

Ces libellés n'avaient pas de traduction anglaise au moment de la lecture du code :

- matériaux de la vue : « gris », « chrome », « couleurs du fichier » ;
- environnements : « studio doux », « coucher de soleil », « nuit », « neutre » ;
- projections de texture : « triplanaire » et les autres ;
- type de dégradé : « hauteur (3D) » ;
- harmonies de palette : « analogue », « complément », « triade », « monochrome », « aléatoire » ;
- pinceau de retouche : « flux » ;
- vue spectrale : « Réparer », « Atténuer » ;
- cibles de sonie : « YouTube, Spotify » et les suivantes ;
- valeurs de plan et mouvements de caméra du storyboard, pour la plupart.

La liste n'est pas complète : elle vient d'une recherche à la main sur les libellés rencontrés.

## Ajouter une langue

1. Crée `assets/i18n/<code>.json`, avec les mêmes clés françaises.
2. Dans `i18n.rs` :
   - ajoute la langue à `LANGS` ;
   - ajoute une table statique comme `EN`, chargée par `include_str!` ;
   - ajoute un cas dans le `match` de `t_mid`.

La langue est mémorisée dans un entier atomique : passer de une à trois langues ne change rien d'autre. Le sélecteur de langue des réglages lit `LANGS`.

## Tests

Le test `table_anglaise` de `i18n.rs` vérifie le passage d'une langue à l'autre, la majuscule de `t`, le saut des symboles, et que la table contient plus de 400 entrées.

**Attention.** La langue est un état global du programme. Un test qui appelle `i18n::set("en")` doit la remettre à `"fr"` à la fin, sinon il perturbe les tests qui tournent en même temps.
