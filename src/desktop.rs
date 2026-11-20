//! Intégration au système, sans droits admin :
//! - Linux : fichier .desktop + icône dans ~/.local/share (menu des applications, icône
//!   de la fenêtre et de la barre des tâches sous Wayland/KDE/GNOME).
//! - Windows : raccourci dans le menu Démarrer de l'utilisateur.

use std::path::PathBuf;

const ICON: &[u8] = include_bytes!("../assets/icon.png");

#[cfg(target_os = "linux")]
fn paths() -> Option<(PathBuf, PathBuf)> {
    let data = dirs::data_dir()?;
    Some((data.join("applications/aryadne.desktop"), data.join("icons/hicolor/256x256/apps/aryadne.png")))
}

/// Vrai si Aryadne est déjà dans le menu, avec le bon chemin d'exécutable.
pub fn installed() -> bool {
    #[cfg(target_os = "linux")]
    {
        let Some((desk, _)) = paths() else { return false };
        let exe = std::env::current_exe().map(|e| e.display().to_string()).unwrap_or_default();
        return std::fs::read_to_string(desk).is_ok_and(|t| t.contains(&format!("Exec=\"{exe}\"")));
    }
    #[cfg(windows)]
    {
        return start_menu_link().is_some_and(|l| l.exists());
    }
    #[allow(unreachable_code)]
    false
}

#[cfg(windows)]
fn start_menu_link() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("Microsoft/Windows/Start Menu/Programs/Aryadne.lnk"))
}

/// Ajoute (ou met à jour) Aryadne dans le menu des applications.
pub fn install() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(target_os = "linux")]
    {
        let (desk, icon) = paths().ok_or("dossier de données introuvable")?;
        std::fs::create_dir_all(desk.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(icon.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&icon, ICON).map_err(|e| e.to_string())?;
        let text = format!(
            "[Desktop Entry]\nType=Application\nName=Aryadne\nGenericName=Notes, références et moodboards\nComment=Notes, code, références et moodboards\nExec=\"{}\" %F\nIcon=aryadne\nTerminal=false\nCategories=Office;Graphics;Utility;\nStartupWMClass=aryadne\nStartupNotify=true\nKeywords=notes;moodboard;refs;3d;\n",
            exe.display()
        );
        std::fs::write(&desk, text).map_err(|e| e.to_string())?;
        // Le menu se met à jour tout seul ; on le pousse un peu si l'outil existe.
        let _ = std::process::Command::new("update-desktop-database").arg(desk.parent().unwrap()).output();
        return Ok(());
    }
    #[cfg(windows)]
    {
        let link = start_menu_link().ok_or("menu Démarrer introuvable")?;
        let ps = format!(
            "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.WorkingDirectory='{}';$s.IconLocation='{},0';$s.Save()",
            link.display().to_string().replace('\'', "''"),
            exe.display().to_string().replace('\'', "''"),
            exe.parent().map(|p| p.display().to_string()).unwrap_or_default().replace('\'', "''"),
            exe.display().to_string().replace('\'', "''"),
        );
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &ps]);
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let out = cmd.output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).into_owned());
        }
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        let _ = exe;
        Err("non pris en charge sur ce système".into())
    }
}

pub fn uninstall() {
    #[cfg(target_os = "linux")]
    if let Some((d, i)) = paths() {
        let _ = std::fs::remove_file(d);
        let _ = std::fs::remove_file(i);
    }
    #[cfg(windows)]
    if let Some(l) = start_menu_link() {
        let _ = std::fs::remove_file(l);
    }
}
