# Aryadne dans Blender : l'affichage lourd

Une extension pour Blender 4.2 à 5.0. Elle fait dessiner les maillages très lourds (des millions de triangles) par le moteur d'Aryadne. L'objet reste un vrai objet Blender : modificateurs, annuler, sauvegarde, sélection, gizmos, tout marche comme d'habitude. Seul son dessin dans la vue 3D change, et va plus vite.

## 1. Avoir Aryadne

- **Télécharger** : prendre `aryadne-windows-x64.exe` (ou `aryadne-linux-x64`) dans les Releases du dépôt GitHub.
- **Ou construire** : `cargo build --release`, le programme est dans `target/release/`.

Le même exécutable sert d'application et de moteur (`aryadne --moteur`). Rien d'autre à installer.

## 2. Installer l'extension

1. Prendre `aryadne_blender-x.y.z.zip` dans les Releases (ou les artefacts de la CI), ou le construire : `blender --command extension build` dans `blender/aryadne_blender/`.
2. Dans Blender : Édition > Préférences > Get Extensions > menu en haut à droite > **Install from Disk…**, choisir le .zip.
3. Dans les préférences de l'extension, **Exécutable Aryadne** : le chemin du .exe (par exemple `C:\Outils\aryadne-windows-x64.exe`). Si `aryadne` est dans le PATH, il est trouvé tout seul.

## 3. Essayer

1. Vue 3D, barre latérale (N), onglet **Aryadne**.
2. **Tester avec une scène lourde** : ajoute une sphère bruitée de 5 millions de triangles (le nombre se change dans le panneau de l'opération, en bas à gauche).
3. Ou, sur n'importe quel maillage : cocher **Affichage lourd (Aryadne)**.

Le moteur démarre tout seul. Le panneau montre le nombre de triangles, le temps d'envoi et le temps d'une image.

Décocher la case, arrêter le moteur ou désactiver l'extension rend la main à Blender : l'objet est redessiné par Blender, sans rien perdre. Le fichier enregistré ne garde aucune trace de l'extension.

## Bon à savoir

- En mode édition, sculpture ou peinture, Blender dessine l'objet lui-même.
- En Material Preview et Rendered, l'objet lourd garde l'ombrage Solid du moteur pour l'instant. Le rendu final (F12) n'est pas touché.
- Les limites et le fonctionnement sont détaillés dans `docs/technique/blender.md`.
