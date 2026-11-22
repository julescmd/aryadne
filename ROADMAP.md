# Aryadne : feuille de route

Tout ce qui reste à faire pour un logiciel complet. Classé par domaine, puis dans l'ordre où je compte l'attaquer.
Légende : **[P1]** prochain, **[P2]** ensuite, **[P3]** plus tard.

## 0. Bugs connus et dettes
- [P1] Fait : disposition en cases (fichiers, document, structure, assistant, terminal, pages) : glisser un onglet pour diviser, empiler ou déplacer ; propre à chaque espace ; clic droit sur un onglet pour le sortir dans sa fenêtre.
- [P1] Interface entièrement modulable : n'importe quel panneau de n'importe quel espace (outliner, propriétés, timeline 3D, moniteurs du montage, calques de la retouche, panneau Projet…) devient un panneau ancrable comme l'explorateur, avec ses bornes min/max.
- [P1] Découpage libre façon Blender : diviser une zone en deux (glisser un coin), fusionner, échanger deux zones, changer le type d'éditeur d'une zone.
- [P1] Onglets de panneaux : empiler plusieurs panneaux dans une même zone (comme Premiere et Unreal).
- [P1] Dispositions enregistrées par espace de travail, avec « réinitialiser », « enregistrer sous… », import/export d'une disposition.
- [P1] Taille minimum de la fenêtre calculée selon les panneaux ouverts ; un panneau qui n'a plus la place se replie en onglet au lieu de casser.
- [P1] Vérifier toutes les pages à 760×480, 1366×768, 1920×1080 et 4K (aucun texte en colonne, aucun bouton coupé).
- [P2] Mise à l'échelle de l'interface (75 % à 200 %), taille de police, thème clair/sombre/personnalisé.
- [P2] Raccourcis entièrement personnalisables (éditeur de raccourcis, profils Blender / Maya / Premiere / Photoshop).
- [P2] Barre de commande (Ctrl+Maj+P) qui couvre 100 % des actions de tous les éditeurs.
- [P2] Enregistrement automatique et récupération pour tous les formats (pas seulement les fichiers sans titre).
- [P3] Historique global (Ctrl+Z qui sait dans quel éditeur on est) et panneau Historique comme Photoshop.

## 1. 3D (Blender / Maya)
### Rendu
- [P1] Trois moteurs : **Workbench** (actuel, rapide), **Eevee-like** (GPU temps réel : PBR, ombres, SSAO, bloom, réflexions écran) et **Cycles-like** (tracé de chemins progressif : GI, caustiques simples, profondeur de champ, flou de mouvement, débruitage).
- [P1] Rendu GPU avec wgpu (déjà dans egui) pour la vue : des millions de triangles fluides.
- [P1] Fenêtre de rendu (F12), rendu d'animation, passes (beauté, profondeur, normales, AO, ID d'objet, cryptomatte), sortie EXR.
- [P2] Échantillonnage adaptatif, rendu par tuiles, rendu en arrière-plan avec file d'attente.
- [P2] Lumières : point, spot, zone, soleil, émissif, IES ; ombres douces ; volumes (brouillard).
- [P3] Rendu en réseau (plusieurs machines), rendu dans le cloud.
### Modélisation
- [P1] Mode édition : coupe en boucle (Ctrl+R), biseau (Ctrl+B), subdiviser, extruder (E) le long des normales, couteau (K), pont entre boucles, remplir (F), fusionner par distance.
- [P1] Sélection : boucle d'arêtes et anneau (Alt / Ctrl+Alt), sélection par lasso et cercle (C), sélection semblable, faces par matériau.
- [P2] Édition proportionnelle (O), aimantation (sommets, arêtes, grille, surfaces), symétrie X/Y/Z.
- [P2] Modificateurs : booléen, solidifier, réseau (array), biseau, miroir, déformation par courbe, subdivision Catmull-Clark réelle, décimation, remaillage voxel.
- [P2] Sculpture : pinceaux dessin, lisser, gonfler, pincer, saisir, masques, dyntopo, multirésolution.
- [P3] Nœuds de géométrie (procédural, comme Geometry Nodes / Houdini léger).
- [P3] Courbes et surfaces (Bézier, NURBS), texte 3D.
### Animation et rigging
- [P1] Armatures (os), skinning automatique, poids peints, cinématique inverse.
- [P2] Dope sheet, NLA (pistes d'actions), pilotes (drivers), contraintes (suivre, viser, copier).
- [P2] Shape keys (morphes), onion skin, enregistrement de mouvement à la souris.
- [P3] Physique : corps rigides, tissus, particules, cheveux, fluides simples.
### Matériaux et textures : un atelier Painter + Designer
Ouvrir un .arymat, c'est la vue Painter ; double-cliquer un calque, c'est entrer dans sa vue Designer. Les deux sont deux vues du même graphe.
- [P1] **Vue Painter** : modèle 3D au centre, vue UV à côté, pile de calques à droite. Chaque calque est une *instance* d'un matériau intelligent, avec un masque, et se mélange canal par canal (couleur, rugosité, métal, normales, hauteur).
- [P1] **Instances** : un graphe de nodes (le *master*) expose des paramètres (couleur, échelle, usure…). Le calque les règle par curseurs sans ouvrir les nodes ; corriger le master met à jour toutes les instances.
- [P1] **Entrer dans un calque** : double-clic, on est dans le graphe Designer du master, le modèle reste visible et se met à jour en direct, fil d'Ariane pour remonter.
- [P1] **Masques** : peints sur le modèle (pinceau, remplissage par face ou îlot UV, symétrie) ou générés (arêtes usées, saleté dans les creux, coulures, poussière sur le dessus, bruit 3D sans coutures). Un masque peut être un petit graphe.
- [P2] **Peindre les paramètres** : peindre l'intensité d'un paramètre d'instance (plus de rouille ici), carte reçue comme entrée du graphe.
- [P2] **Pile = graphe** : la pile entière s'ouvre comme un graphe (calques et masques en nodes), une touche passe d'une vue à l'autre.
- [P2] Projection d'une photo ou d'un .aryimg sur le modèle comme calque, bibliothèque de matériaux intelligents du coffre, export des textures pour Unreal, Blender, Maya.
- [P1] Matériaux PBR complets liés aux nodes (.arymat) dans les trois moteurs.
- [P2] Peinture de texture directement sur le modèle (comme Substance Painter) : calques, masques, générateurs (usure, saleté, bords), projection, export vers les moteurs de jeu.
- [P2] Cuisson (bake) : normales, AO, courbure, ID, depuis un modèle haute définition.
- [P3] Matériaux procéduraux façon Substance Designer (graphe de textures, export de textures tuilables).
### Import / export
- [P2] USD, glTF complet (animations, morphes), Alembic, FBX avec os et animations.
- [P3] Lien direct avec Blender, Maya, Unreal (envoyer la scène en un clic).

## 2. Montage vidéo (Premiere Pro)
- [P1] Fait : disposition Premiere, pistes V/A, outils V A B N R C Y H Z T, Source et Programme, insérer/recouvrir, propagation, effets à glisser.
- [P1] Lecture fluide (aperçu en cache par segments, lecture à demi et quart de résolution), J/L accélérés (×2, ×4) et lecture arrière.
- [P1] Image clé par paramètre (losanges dans les options d'effet), courbes de vitesse (Bézier) et remappage temporel avec images figées.
- [P1] Masques sur les clips (ellipse, rectangle, plume) avec suivi, et mode de fusion de clip.
- [P2] Titres riches (styles, ombres, fonds, animations d'entrée), modèles graphiques (Essential Graphics).
- [P2] Mixage audio : panneau de mixage, vu-mètres, panoramique, effets (EQ, compresseur, réduction de bruit), enveloppes de volume dans la timeline.
- [P2] Séquences imbriquées, multicaméra, proxys, synchronisation par le son.
- [P2] Couleur Lumetri complète : roues, courbes, LUT, scopes (forme d'onde, vecteurscope, histogramme).
- [P2] Export : préréglages (YouTube, Instagram, ProRes, image par image, GIF), file d'attente, rendu en arrière-plan.
- [P3] Sous-titres automatiques (transcription locale), recadrage automatique 9:16, montage par le texte.

## 3. Retouche et peinture (Photoshop)
- [P1] Sélections : lasso, lasso polygonal, baguette magique, sélection rapide, plage de couleurs, contour progressif, inverser, agrandir/réduire.
- [P1] Masques de calque, masques d'écrêtage, groupes de calques, calques de réglage (niveaux, courbes, teinte/saturation, balance).
- [P1] Transformation libre (Ctrl+T) : échelle, rotation, inclinaison, perspective, déformation.
- [P1] Texte (calque texte modifiable), formes vectorielles.
- [P2] Filtres : flou gaussien, netteté, bruit, flou de mouvement, fluidité, correction de lentille.
- [P2] Pinceaux : dureté, espacement, dynamique de pression (tablette), pinceaux personnalisés, mélangeur, tampon de duplication, correcteur.
- [P2] Effets de calque (ombre, contour, lueur), objets dynamiques (un .aryimg dans un .aryimg).
- [P3] Animation image par image (onion skin), import/export PSD.

## 4. Références (PureRef)
- [P1] Board infini sans bordures : rotation, retournement, recadrage non destructif, alignement et empilement automatiques, niveaux de gris, verrouillage.
- [P2] Mode « toujours au premier plan » et transparence de la fenêtre, comme PureRef.
- [P2] Pipette sur n'importe quelle image, palette extraite.
- [P3] Recherche d'images par similarité dans le coffre.

## 5. Notes et organisation (Notion / Milanote)
- [P1] Blocs Notion : titres, listes de tâches, tableaux, bases de données (vues table, kanban, calendrier, galerie), pages imbriquées, mentions.
- [P1] Board façon Milanote : cartes, colonnes, flèches, notes collantes, liens, fichiers, tâches.
- [P2] Gabarits (brief de projet, planning de production, feuille de personnage, storyboard).
- [P2] Partage et collaboration en temps réel (Cloudflare, sans clés dans le code).
- [P3] Synchronisation mobile (consultation des boards et notes sur téléphone).

## 6. Nodes (Unreal / Substance Designer / Nuke)
- [P1] Matériaux : plus de nodes (bruits, gradients, masques, triplanaire, normales depuis hauteur).
- [P2] Compositing façon Nuke : lecture de séquences d'images, roto, keying (fond vert), suivi 2D.
- [P2] Sous-graphes réutilisables, paramètres exposés, bibliothèque de nodes partagée.
- [P3] Blueprints simples (logique visuelle pour des scènes interactives).

## 7. Ce que seul Aryadne peut faire (tout marche ensemble)
- [P1] Fait : un .aryimg se glisse dans le montage, le board et l'explorateur.
- [P1] Un .aryimg comme texture dans la 3D et les nodes ; il se met à jour en direct quand on le peint.
- [P1] Une scène 3D glissée dans le montage devient un clip (rendu à la volée, mis à jour quand la scène change).
- [P1] Un montage comme texture vidéo dans la 3D (déjà en nodes) et comme clip imbriqué dans un autre montage.
- [P2] Peindre une texture dans la retouche depuis la 3D (« éditer dans la retouche » puis retour automatique).
- [P2] Storyboard : un board de cases qui devient un montage animatique en un clic, et l'inverse.
- [P2] Planche de références liée à un plan : chaque plan du montage garde ses références, notes et tâches.
- [P2] Liens partout : mentionner un fichier dans une note l'affiche en vignette vivante (scène qui tourne, vidéo qui joue).
- [P2] Suivi de production : chaque fichier a un statut (à faire, en cours, validé), un responsable et des notes de révision ; tableau de bord du studio.
- [P3] Assistant IA qui connaît le coffre : « fais une animatique avec ces cases », « trouve toutes les références de ce personnage ».
- [P3] Versions de fichiers et comparaison visuelle (avant/après) pour chaque format.

## 8. Logiciel et diffusion
- [P1] Notes de version claires pour chaque release, mise à jour en un clic (déjà en place, à fiabiliser).
- [P2] Installateurs Windows (MSI), macOS (DMG signé), Linux (AppImage, Flatpak).
- [P2] Système d'extensions (plugins) et API de script (Python ou Lua) pour automatiser.
- [P3] Documentation intégrée, tutoriels interactifs, exemples de projets.
- [P3] Tests d'interface automatiques (captures comparées) pour éviter les régressions de mise en page.
