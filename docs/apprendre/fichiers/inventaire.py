# Inventaire du coffre : F5 pour lancer.
# Ce script lit tes notes. Il ne modifie rien tant que la dernière ligne reste en commentaire.
import aryadne

coffre = aryadne.Coffre()

notes = coffre.fichiers(".md")
print(f"{len(notes)} notes dans le coffre")

a_faire = 0
lignes = ["# Inventaire", ""]
for rel in notes:
    texte = coffre.note(rel)
    cases = texte.count("- [ ]")
    a_faire += cases
    if cases:
        lignes.append(f"- {rel} : {cases} tâche(s) à faire")

print(f"{a_faire} tâches à faire")
lignes.append("")
lignes.append(f"Total : {a_faire} tâches à faire dans {len(notes)} notes.")

# Retire le # de la ligne suivante pour écrire le résultat dans une note.
# coffre.note("Apprendre/inventaire.md", "\n".join(lignes))
