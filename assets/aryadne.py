"""Aryadne : piloter le coffre depuis un script Python.

Les fichiers d'Aryadne sont du JSON lisible : ce module les ouvre, les modifie et les réenregistre.
Aryadne recharge un fichier quand il change sur le disque.

    import aryadne
    coffre = aryadne.Coffre()                      # le coffre du script (ou Coffre("/chemin"))
    for f in coffre.fichiers(".aryscene"):
        print(f)

    scene = coffre.scene("3d/Cube.aryscene")
    boule = scene.objet("boule")
    boule["pos"] = [0, 1, 0]
    scene.cle("boule", 24, pos=[0, 2, 0])         # une clé à l'image 24
    scene.enregistrer()

    montage = coffre.montage("montage/Essai.aryseq")
    for c in montage.clips():
        print(c["name"], c["start"], montage.duree_clip(c))
    montage.enregistrer()

    dessin = coffre.dessin("2d/Maquette.aryvec")
    for f in dessin.formes():
        if f["name"].startswith("Bouton"):
            dessin.couleur(f, "#ec6c59")
    dessin.enregistrer()

Écrire hors du coffre est refusé. Une copie de chaque fichier modifié est gardée dans .app/backup.
"""

import json
import os
import shutil
import time

__all__ = ["Coffre", "Scene", "Montage", "Dessin"]


class Coffre:
    """Le coffre : un dossier de notes, d'images, de scènes et de montages."""

    def __init__(self, racine=None):
        racine = racine or os.environ.get("ARYADNE_VAULT") or os.getcwd()
        self.racine = os.path.abspath(racine)

    def chemin(self, rel):
        p = os.path.abspath(os.path.join(self.racine, rel))
        if os.path.commonpath([p, self.racine]) != self.racine:
            raise ValueError(f"hors du coffre : {rel}")
        return p

    def fichiers(self, extension=""):
        """Tous les fichiers du coffre (chemins relatifs), éventuellement d'une extension"""
        out = []
        for d, dirs, files in os.walk(self.racine):
            dirs[:] = [x for x in dirs if not x.startswith(".")]
            for f in files:
                if f.startswith(".") or not f.lower().endswith(extension.lower()):
                    continue
                out.append(os.path.relpath(os.path.join(d, f), self.racine))
        return sorted(out)

    def lire(self, rel):
        with open(self.chemin(rel), encoding="utf-8") as f:
            return f.read()

    def ecrire(self, rel, texte):
        p = self.chemin(rel)
        _copie(self, p)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        tmp = p + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            f.write(texte)
        os.replace(tmp, p)

    def note(self, rel, texte=None):
        """Lit une note (texte=None) ou la remplace"""
        if texte is None:
            return self.lire(rel)
        self.ecrire(rel, texte)

    def scene(self, rel):
        return Scene(self, rel)

    def montage(self, rel):
        return Montage(self, rel)

    def dessin(self, rel):
        return Dessin(self, rel)


def _copie(coffre, p):
    if not os.path.exists(p):
        return
    d = os.path.join(coffre.racine, ".app", "backup")
    os.makedirs(d, exist_ok=True)
    shutil.copy2(p, os.path.join(d, f"{int(time.time())}-{os.path.basename(p)}"))


class _Json:
    def __init__(self, coffre, rel):
        self.coffre, self.rel = coffre, rel
        self.data = json.loads(coffre.lire(rel))

    def enregistrer(self):
        self.coffre.ecrire(self.rel, json.dumps(self.data, ensure_ascii=False, indent=2))


class Scene(_Json):
    """Une scène 3D (.aryscene) : objets, positions, rotations (degrés), tailles, clés."""

    def objets(self):
        return self.data.setdefault("objects", [])

    def objet(self, nom):
        for o in self.objets():
            if o.get("name") == nom:
                return o
        raise KeyError(nom)

    def cle(self, nom, image, pos=None, rot=None, scale=None, lissage="bezier"):
        """Pose (ou remplace) une clé ; les valeurs absentes reprennent celles de l'objet"""
        o = self.objet(nom)
        k = {"frame": int(image), "pos": pos or o.get("pos", [0, 0, 0]), "rot": rot or o.get("rot", [0, 0, 0]), "scale": scale or o.get("scale", [1, 1, 1]), "ease": True, "interp": lissage}
        keys = [x for x in o.setdefault("keys", []) if x.get("frame") != k["frame"]]
        keys.append(k)
        o["keys"] = sorted(keys, key=lambda x: x["frame"])
        return k

    def cadence(self):
        return self.data.get("fps", 24)


class Montage(_Json):
    """Un montage (.aryseq, ou l'ancien .aryav) : clips sur des pistes, en secondes."""

    def clips(self, piste=None, son=None):
        out = self.data.setdefault("clips", [])
        if piste is not None:
            out = [c for c in out if c.get("track") == piste]
        if son is not None:
            out = [c for c in out if bool(c.get("audio")) == son]
        return out

    def duree_clip(self, c):
        return (c.get("src_out", 0) - c.get("src_in", 0)) / max(c.get("speed", 1) or 1, 1e-6)

    def duree(self):
        return max([c.get("start", 0) + self.duree_clip(c) for c in self.clips()] or [0])

    def renommer_plans(self, modele="Plan {n}"):
        """Renomme les plans vidéo dans l'ordre de la timeline : « Plan 1 », « Plan 2 »…"""
        vids = sorted([c for c in self.clips() if not c.get("audio")], key=lambda c: (c.get("start", 0), c.get("track", 0)))
        for n, c in enumerate(vids, 1):
            c["name"] = modele.format(n=n, nom=c.get("name", ""))
        return len(vids)


class Dessin(_Json):
    """Un dessin vectoriel (.aryvec) : plans de travail et formes."""

    def plans(self):
        return self.data.setdefault("boards", [])

    def formes(self):
        return self.data.setdefault("shapes", [])

    def forme(self, nom):
        for s in self.formes():
            if s.get("name") == nom:
                return s
        raise KeyError(nom)

    def couleur(self, forme, couleur):
        """Remplissage uni : « #rrggbb », « #rrggbbaa » ou [r, g, b, a]"""
        if isinstance(couleur, str):
            h = couleur.lstrip("#")
            c = [int(h[i:i + 2], 16) for i in range(0, len(h), 2)]
            if len(c) == 3:
                c.append(255)
        else:
            c = list(couleur) + [255] * (4 - len(couleur))
        fills = forme.setdefault("fills", [])
        solid = {"type": "solid", "c": c}
        if fills:
            fills[0] = solid
        else:
            fills.append(solid)
        forme.pop("fill_var", None)
