// Icône de l'exécutable Windows (le logo Aryadne), et la documentation embarquée dans le binaire.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    embed_docs();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Aryadne");
        res.set("FileDescription", "Aryadne");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icône Windows non intégrée : {e}");
        }
    }
}

/// La documentation (docs/, en Markdown) part dans le binaire : une liste (chemin relatif, texte)
/// écrite dans `$OUT_DIR/docs_embed.rs`, que `src/docs.rs` inclut. Menu Aide › Documentation.
fn embed_docs() {
    println!("cargo:rerun-if-changed=docs");
    let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("docs");
    let mut files: Vec<std::path::PathBuf> = vec![];
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "md") {
                out.push(p);
            }
        }
    }
    walk(&root, &mut files);
    files.sort();
    let mut code = String::from("pub const FILES: &[(&str, &str)] = &[\n");
    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
        let rel = f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        code.push_str(&format!("    ({rel:?}, include_str!({:?})),\n", f.to_string_lossy()));
    }
    code.push_str("];\n");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("docs_embed.rs");
    std::fs::write(out, code).unwrap();
}
