# Client du moteur Aryadne (`aryadne --moteur`) : processus enfant, commandes en lignes JSON,
# images lues dans un fichier projeté en mémoire. Ce module n'importe pas bpy : il se teste
# aussi hors de Blender.
import json
import mmap
import os
import queue
import struct
import subprocess
import sys
import tempfile
import threading
import time

import numpy as np

MAGIE = b"ARYI"
PROTOCOLE = 1


class ErreurMoteur(RuntimeError):
    pass


class Moteur:
    """Le processus du moteur et ses échanges."""

    def __init__(self, exe):
        self.exe = exe
        self.proc = None
        self.n = 0
        self.attente = {}  # n -> [Event, réponse]
        self.libres = queue.Queue()  # réponses que personne n'attend (images)
        self.journal = []  # dernières lignes d'erreur du moteur
        self.verrou = threading.Lock()
        self.carte = None  # (chemin, mmap) du fichier d'images
        self.version = None

    # ---------- processus ----------

    def demarrer(self):
        if self.vivant():
            return
        if not self.exe or not os.path.isfile(self.exe):
            raise ErreurMoteur("Exécutable Aryadne introuvable : %r" % (self.exe,))
        flags = 0
        if sys.platform == "win32":
            flags = getattr(subprocess, "CREATE_NO_WINDOW", 0x08000000)
        self.proc = subprocess.Popen(
            [self.exe, "--moteur"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=0,
            creationflags=flags,
        )
        threading.Thread(target=self._lire, args=(self.proc,), daemon=True).start()
        threading.Thread(target=self._lire_erreurs, args=(self.proc,), daemon=True).start()
        r = self.appeler({"cmd": "bonjour"}, delai=15.0)
        if r.get("protocole") != PROTOCOLE:
            self.arreter()
            raise ErreurMoteur("Version du protocole différente (moteur %s, add-on %s)" % (r.get("protocole"), PROTOCOLE))
        self.version = r.get("version")

    def vivant(self):
        return self.proc is not None and self.proc.poll() is None

    def arreter(self):
        p = self.proc
        if p is None:
            return
        try:
            if p.poll() is None:
                self.envoyer({"cmd": "quitter"})
                try:
                    p.wait(2.0)
                except subprocess.TimeoutExpired:
                    p.kill()
                    p.wait(2.0)
        except Exception:
            try:
                p.kill()
            except Exception:
                pass
        self.proc = None
        self._fermer_carte()
        for ev in list(self.attente.values()):
            ev[0].set()
        self.attente.clear()

    def _lire(self, proc):
        for ligne in proc.stdout:
            try:
                r = json.loads(ligne)
            except ValueError:
                continue
            n = r.get("n")
            with self.verrou:
                ev = self.attente.pop(n, None)
            if ev is not None:
                ev[1] = r
                ev[0].set()
            else:
                self.libres.put(r)

    def _lire_erreurs(self, proc):
        for ligne in proc.stderr:
            self.journal.append(ligne.decode("utf-8", "replace").rstrip())
            del self.journal[:-50]

    # ---------- échanges ----------

    def envoyer(self, cmd, attendre=False):
        """Envoie une commande ; rend son numéro (et un événement à attendre si demandé)."""
        if not self.vivant():
            raise ErreurMoteur("Le moteur n'est pas démarré")
        with self.verrou:
            self.n += 1
            n = self.n
            cmd = dict(cmd, n=n)
            ev = None
            if attendre:
                ev = [threading.Event(), None]
                self.attente[n] = ev
        data = (json.dumps(cmd, separators=(",", ":")) + "\n").encode("utf-8")
        try:
            self.proc.stdin.write(data)
            self.proc.stdin.flush()
        except (BrokenPipeError, OSError) as e:
            raise ErreurMoteur("Le moteur ne répond plus : %s" % e)
        return n, ev

    def appeler(self, cmd, delai=120.0):
        """Commande et réponse (bloquant : pour les envois rares, pas pour le dessin)."""
        _, ev = self.envoyer(cmd, attendre=True)
        if not ev[0].wait(delai) or ev[1] is None:
            raise ErreurMoteur("Pas de réponse du moteur à %s" % cmd.get("cmd"))
        if not ev[1].get("ok"):
            raise ErreurMoteur(ev[1].get("erreur", "erreur inconnue"))
        return ev[1]

    def reponses(self):
        """Réponses arrivées sans que personne ne les attende (les images demandées sans bloquer)."""
        out = []
        while True:
            try:
                out.append(self.libres.get_nowait())
            except queue.Empty:
                return out

    # ---------- images ----------

    def _fermer_carte(self):
        if self.carte is not None:
            try:
                self.carte[1].close()
            except Exception:
                pass
            try:
                self.carte[2].close()
            except Exception:
                pass
            self.carte = None

    def lire_image(self, r):
        """Copie l'image annoncée par la réponse `r` : (rgba uint8 h×w×4, profondeur float32 h×w,
        identifiants uint32 h×w), lignes de bas en haut. La copie libère la case pour le moteur."""
        chemin = r["fichier"]
        if self.carte is None or self.carte[0] != chemin:
            self._fermer_carte()
            f = open(chemin, "rb")
            m = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
            if m[0:4] != MAGIE:
                m.close()
                f.close()
                raise ErreurMoteur("Fichier d'images inattendu")
            self.carte = (chemin, m, f)
        m = self.carte[1]
        w, h, k = r["largeur"], r["hauteur"], r["case"]
        num = struct.unpack_from("<Q", m, 16 + k * 16)[0]
        if num != r["image"]:
            return None  # la case a déjà été réécrite : on attend la suivante
        n = w * h
        rgba = np.frombuffer(m, dtype=np.uint8, count=n * 4, offset=r["decalage_rgba"]).reshape(h, w, 4).copy()
        prof = np.frombuffer(m, dtype="<f4", count=n, offset=r["decalage_profondeur"]).reshape(h, w).copy()
        ids = np.frombuffer(m, dtype="<u4", count=n, offset=r["decalage_ids"]).reshape(h, w).copy()
        return rgba, prof, ids

    # ---------- maillages ----------

    def envoyer_maillage(self, ident, remplir, nv, nt, extras=(), **champs):
        """Écrit un maillage dans un fichier temporaire projeté en mémoire, puis le donne au moteur.
        `remplir(pos, tris, extras)` remplit les vues numpy (pos float32 nv×3, tris uint32 nt×3,
        puis les tableaux demandés dans `extras` : (nom, dtype)). Rend la réponse du moteur."""
        taille_pos = nv * 12
        taille_tris = nt * 12
        decalages = []
        o = taille_pos + taille_tris
        for nom, dt in extras:
            o = (o + 3) & ~3
            decalages.append((nom, dt, o))
            o += nt * np.dtype(dt).itemsize
        fd, chemin = tempfile.mkstemp(prefix="aryadne-maillage-", suffix=".bin")
        try:
            os.ftruncate(fd, max(o, 1))
            m = mmap.mmap(fd, max(o, 1))
            try:
                pos = np.frombuffer(m, dtype=np.float32, count=nv * 3, offset=0)
                tris = np.frombuffer(m, dtype=np.uint32, count=nt * 3, offset=taille_pos)
                vues = [np.frombuffer(m, dtype=dt, count=nt, offset=off) for _, dt, off in decalages]
                remplir(pos, tris, vues)
                del pos, tris, vues
                m.flush()
            finally:
                m.close()
            os.close(fd)
            fd = None
            cmd = {"cmd": "objet", "id": ident, "fichier": chemin, "sommets": nv, "triangles": nt}
            for nom, _, off in decalages:
                cmd["decalage_" + nom] = off
            cmd.update(champs)
            return self.appeler(cmd)
        finally:
            if fd is not None:
                os.close(fd)
            try:
                os.remove(chemin)
            except OSError:
                pass


def matrice(m):
    """Matrice mathutils (ou liste de lignes) en 16 nombres par lignes."""
    return [float(m[i][j]) for i in range(4) for j in range(4)]


def attendre_image(moteur, n, delai=60.0):
    """Attend la réponse d'image numéro `n` (pour les tests et le premier affichage)."""
    t0 = time.time()
    while time.time() - t0 < delai:
        for r in moteur.reponses():
            if r.get("n") == n:
                return r
        time.sleep(0.001)
    raise ErreurMoteur("Pas d'image du moteur")
