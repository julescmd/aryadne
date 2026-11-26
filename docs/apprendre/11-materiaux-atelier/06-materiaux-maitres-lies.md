---
titre: Matériaux maîtres liés
niveau: intermédiaire
duree: 20 min
prerequis: 11-materiaux-atelier/04-entrer-dans-un-calque
objectifs: Fabriquer un matériau maître avec des paramètres exposés ; L'ajouter comme calque lié ; Corriger le maître pour tout mettre à jour ; Détacher un calque de son maître
---

## Ce que tu vas réussir

Tu vas fabriquer une peinture maîtresse, réglable par deux curseurs. Tu vas t'en servir deux fois dans un jouet. Puis tu vas corriger le maître une seule fois, et voir les deux calques suivre.

## L'idée

Un **matériau maître** est un `.arymat` ordinaire de ton coffre. Un autre matériau peut s'en servir comme calque **lié**. Le calque ne copie pas le graphe : il le lit dans le fichier du maître.

- Tu corriges le maître : tous les calques liés changent.
- Chaque calque garde ses propres valeurs de paramètres : sa couleur, son usure.

C'est le principe des instances de matériau.

## Partie 1 : fabrique le maître

```essayer
nouveau: arymat
nom: maitre-peinture
```

Le matériau s'ouvre avec un seul calque, **Graph**.

```etape
Double-clique le calque **Graph** pour entrer dans ses nodes.
```

Tu y trouves un **Noise**, une rampe et la sortie. Tu vas remplacer la couleur par un paramètre.

```etape
Ajoute un node **Exposed parameter**. Laisse **Type** sur **color**. Appuie sur F2 et nomme-le « teinte ».
verifier: graphe.contient = param
```

```etape
Relie sa sortie **Color** à l'entrée **Color** de **Material output**. Ce lien remplace celui de la rampe.
```

```etape
Ajoute un second **Exposed parameter**. Choisis **Type** : **value**. Nomme-le « brillance » avec F2.
verifier: graphe.noeuds >= 5
```

```etape
Relie sa sortie **Value** à l'entrée **Roughness** de **Material output**.
```

Garde le **Noise** : tu vas t'en servir pour un léger relief.

```etape
Relie la sortie **Value** de **Noise** à l'entrée **Bump** de **Material output**. Mets **Scale** du bruit à 40 et **Bump strength** à 0,2.
verifier: graphe.liens >= 3
```

```etape
Reviens à la pile avec **← Layers**. Enregistre avec Ctrl+S.
verifier: fichier.enregistre
```

```attention
Un maître doit tenir en un seul graphe : une pile d'un seul calque **Graph**, posé partout et opaque. N'ajoute pas d'autre calque à ce fichier.
```

## Partie 2 : sers-toi du maître

```essayer
nouveau: arymat
nom: jouet
```

```etape
Dans la bibliothèque, clique le filtre **Vault master materials**. Ton « maitre-peinture » y figure.
```

```etape
Clique-le. Un calque s'ajoute, marqué d'un petit maillon : il est lié.
verifier: atelier.calques >= 2
```

```etape
Choisis ce calque. Dans la carte **Layer**, lis la ligne **Master material** : elle nomme le fichier du maître.
```

```etape
Dans la carte **Parameters**, règle « teinte » sur un rouge et « brillance » sur 0,3.
```

### Une seconde fois, autrement

```etape
Clique de nouveau « maitre-peinture » dans la bibliothèque. Un second calque lié arrive.
verifier: atelier.calques >= 3
```

```etape
Règle sa « teinte » sur un jaune. Dans **Mask**, choisis **Edge wear**.
```

Deux calques, un seul graphe, deux réglages. Le jouet est rouge, avec des arêtes jaunes.

## Partie 3 : corrige le maître

```etape
Double-clique un des deux calques liés. Tu entres dans le graphe du maître.
```

Le fil d'Ariane porte un maillon. Au survol, il prévient : ce graphe est celui du maître, partagé par tous les matériaux qui s'en servent.

```etape
Monte **Bump strength** à 1 sur **Material output**. Le relief devient très visible.
```

```etape
Reviens à la pile avec **← Layers**.
```

Les **deux** calques ont pris le relief. Le changement a été écrit dans le fichier « maitre-peinture ». Tout autre matériau du coffre qui s'en sert le reçoit aussi.

```astuce
Un bon maître expose peu de paramètres, bien nommés. Trois curseurs clairs valent mieux que dix obscurs.
```

## Détacher un calque

Parfois un calque doit vivre sa vie.

```etape
Choisis le calque jaune. Dans la carte **Layer**, sur la ligne **Master material**, clique le maillon brisé.
```

Le calque garde une copie du graphe, à lui seul. Le maillon a disparu de sa ligne. Les corrections du maître ne l'atteignent plus.

## Trois sortes de calques

| Calque | D'où vient son graphe | Signe dans la pile |
|---|---|---|
| Remplissage | Créé par **+**. Trois paramètres | |
| Graphe | Son propre graphe, rangé dans le matériau | |
| Lié | Le fichier d'un maître du coffre | Un maillon |

```quiz
Tu modifies le graphe d'un calque lié. Où le changement est-il écrit ?
- [ ] Dans le matériau ouvert seulement.
- [x] Dans le fichier du maître, donc pour tous ceux qui s'en servent.
- [ ] Nulle part tant que tu ne détaches pas le calque.
explication: Un calque lié ouvre le graphe de son maître. Ce que tu y changes est partagé.
```

```quiz
Deux calques liés au même maître peuvent-ils avoir des couleurs différentes ?
- [x] Oui : chaque calque garde ses propres valeurs de paramètres.
- [ ] Non : ils sont identiques en tout.
- [ ] Seulement après les avoir détachés.
explication: Le graphe est partagé. Les valeurs des paramètres exposés appartiennent à chaque calque.
```

```defi
Ajoute au maître un paramètre « métal », puis fais un jouet moitié plastique, moitié métal.
indice: Un Exposed parameter de type value, relié à Metal. Les deux calques liés reçoivent le nouveau curseur.
```

## En trois lignes

- Un maître est un `.arymat` du coffre qui tient en un seul graphe.
- Un calque lié lit le graphe du maître et garde ses propres paramètres.
- Corriger le maître change tous ses calques. Le maillon brisé détache.

[Suite : Réutiliser, appliquer, cuire](07-reutiliser-appliquer-cuire.md)
