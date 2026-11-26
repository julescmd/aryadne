---
titre: Nettoyer et mixer le son
niveau: intermédiaire
duree: 15 min
prerequis: 01-demarrer/05-fichiers-du-coffre
objectifs: Créer une session audio ; Poser, couper et déplacer des clips ; Nettoyer une voix ; Faire baisser la musique sous la parole ; Mettre au bon niveau et exporter
---

## Ce que tu vas réussir à faire

Tu vas prendre une voix enregistrée dans de mauvaises conditions, la nettoyer, lui glisser une musique dessous, et sortir un fichier au bon niveau pour une plateforme.

Rien n'est détruit. Un clip renvoie à un son du coffre. Il garde ses coupes, son gain, ses fondus et ses effets. Le son d'origine ne change jamais.

Il te faut un enregistrement de voix et une musique.

```essayer
nouveau: aryaudio
nom: voix-et-musique
```

```etape
Clique **Try it yourself**. Une session audio s'ouvre dans l'étape Sound, outil **Audio**.
verifier: onglet = son
```

## L'écran

| Panneau | Contenu |
|---|---|
| **Sounds (audio)** | Les sons du coffre, et les repères |
| **Tracks (audio)** | Les pistes et leurs clips |
| **Transport (audio)** | Lecture, enregistrement, outils, métronome |
| **Inspector (audio)** | Trois onglets : **Effects**, **Clip**, **Spectrum** |
| **Status (audio)** | La sélection, la durée, la fréquence du projet |

## Poser des sons

```etape
Dans **Sounds (audio)**, clique le `+` : **Import sounds…**. Choisis ta voix et ta musique.
```

```etape
Clique la voix dans la liste. Elle se pose sur la piste active, à la tête de lecture.
```

```etape
Choisis une autre piste. Clique la musique : elle se pose sur cette piste.
```

```astuce
Pas de fichier de voix ? Appuie sur `R` : Aryadne enregistre au micro sur la piste choisie. `R` de nouveau arrête la prise.
```

## Lire et couper

| Touche | Effet |
|---|---|
| `Espace` | Lire |
| `Origine` | Retour au début |
| `V` | L'outil de sélection |
| `B` | Le rasoir : coupe un clip au clic |
| `S` | Coupe à la tête de lecture |
| `Ctrl+D` | Duplique le clip |
| `Suppr` | Supprime et resserre la piste |
| `L` | Boucle sur la plage choisie |
| `M` | Pose un repère |

```etape
Lis la voix. Arrête-toi juste avant une erreur ou un blanc. Appuie sur `S`. Avance jusqu'après l'erreur. Appuie encore sur `S`.
```

```etape
Clique le morceau fautif et appuie sur `Suppr`. Il disparaît et la suite se rapproche.
```

Glisser sur une piste, hors d'un clip, choisit une **plage** de temps. `L` la lit en boucle : pratique pour régler un effet.

## Nettoyer la voix

L'onglet **Effects** range les effets par famille. Ils s'appliquent **On the clip** ou **On the range**.

```etape
Choisis le clip de voix. Dans **Effects**, section **Clean up**, clique **Clear voice**.
```

**Clear voice** fait tout en un : coupe-bas, réduction de bruit, présence, compression.

| Effet de **Clean up** | Ce qu'il fait |
|---|---|
| **Clear voice** | Tout en un, pour une voix parlée |
| **Noise reduction** | Retire le souffle et le bourdonnement |
| **Low cut** | Retire les graves sous 100 Hz |
| **Cut silence** | Retire les blancs et resserre la piste |

| Effet de **Tone** | Ce qu'il fait |
|---|---|
| **Equalizer** | Plus de clarté |
| **Warm equalizer** | Plus de chaleur |
| **Compressor** | Un niveau plus égal |
| **De-esser** | Adoucit les « s » qui sifflent |
| **Limiter** | Rien ne dépasse -1 dB |
| **Gate** | Coupe le souffle entre les phrases |

```etape
Écoute le résultat. Si les « s » sifflent, ajoute **De-esser**.
```

```attention
Les effets sont rendus par ffmpeg. Chaque changement relance un calcul : tu n'entends pas l'effet pendant que tu bouges un réglage. Change, puis écoute.
```

### Voir ce que le clip porte

L'onglet **Clip** montre **Gain**, **Fade in**, **Fade out**, puis **Clip effects** : la liste des effets posés. Chacun se règle et se retire.

```etape
Ouvre l'onglet **Clip**. Regarde la liste **Clip effects**. Règle un **Fade in** court au début de la voix.
```

Tu peux enregistrer cette liste comme une **chaîne** : donne-lui un nom, et applique-la d'un clic à d'autres clips.

## La musique sous la voix

Chaque piste a **Mute**, **Solo**, **Volume**, et **Duck under**.

**Duck under** fait baisser une piste toute seule quand une autre joue. C'est la musique qui s'efface sous la voix.

```etape
Sur la piste de la musique, active **Duck under** et choisis la piste de la voix.
```

```etape
Règle **By** : de combien la musique baisse. Règle **Release** : le temps qu'elle met à remonter. Relis.
```

Pour un réglage à la main, la ligne de volume d'une piste porte des points d'automation. Un clic pose un point. Glisse-le pour le régler. Double-clic pour le retirer.

## Réparer un bruit précis

Un clic, une sonnerie, un sifflement : la **Spectral view** montre le son comme une image. Le temps en largeur, les fréquences en hauteur. Le bruit y fait une tache.

1. Ouvre **Spectral view** sur le clip.
2. Peins la tache avec le pinceau (`B`), ou prends une zone au rectangle (`M`).
3. Choisis de réparer ou d'atténuer.
4. Appuie sur `Entrée` pour traiter.
5. **Keep** écrit le son retouché dans un nouveau fichier et le met dans le clip.

L'original reste intact.

## Mettre au bon niveau

Chaque plateforme attend un niveau de sonie, mesuré en LUFS.

| Cible | Usage |
|---|---|
| -14 LUFS | YouTube, Spotify |
| -16 LUFS | Apple Music, podcasts |
| -19 LUFS | Podcast en mono |
| -23 LUFS | Télévision |

```etape
Choisis le clip de voix. Dans **Effects**, section du volume, lance la normalisation de la sonie avec la cible -14 LUFS.
```

**Measure: true peak, correlation** donne la crête vraie, à garder sous -1 dBTP, et la corrélation stéréo. Proche de 1, le son passe bien en mono.

```astuce
Le message « limited by the peak » veut dire qu'une crête empêche d'atteindre la cible. Ajoute un **Limiter** ou un **Compressor**, puis relance.
```

## Exporter

```etape
Clique **Export…**. Choisis WAV ou MP3.
```

| Sortie | Notes |
|---|---|
| WAV, MP3 | Le mixage complet |
| **Separate tracks (stems)** | Un WAV par piste, tous de la durée de la session |
| **Export each region** | Un WAV par région, entre deux repères. Pour une banque de bruitages |

Le fichier exporté se pose ensuite dans ton montage.

```quiz
Tu appliques Clear voice à un clip. Qu'arrive-t-il au fichier d'origine ?
- [x] Rien : il reste intact.
- [ ] Il est remplacé.
- [ ] Il est supprimé.
explication: Un clip porte ses effets. Le son du coffre ne change jamais.
```

```quiz
À quoi sert Duck under ?
- [ ] À couper les graves.
- [x] À faire baisser une piste quand une autre joue.
- [ ] À dupliquer une piste.
explication: C'est le réglage de la musique sous la voix. By règle la baisse, Release la remontée.
```

```quiz
Quelle cible de sonie pour YouTube ?
- [x] -14 LUFS
- [ ] -23 LUFS
- [ ] 0 LUFS
explication: -14 LUFS pour YouTube et Spotify. -23 LUFS est la norme de la télévision.
```

```defi
Fais une intro de podcast de quinze secondes : une musique qui démarre fort, baisse sous une phrase de voix, puis remonte et s'éteint en fondu.
indice: **Duck under** fait la baisse. Le **Fade out** du clip de musique fait la fin. Exporte en MP3 à -16 LUFS.
```

## Récapitulatif

- Un clip porte ses coupes et ses effets. Le son d'origine reste intact.
- **Clear voice** nettoie une voix d'un clic. **Duck under** glisse la musique dessous.
- Normalise la sonie selon la plateforme, puis **Export…**.

Leçon suivante : [Composer une boucle](05-musique.md)
