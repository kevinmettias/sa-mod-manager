use crate::prelude::*;

pub(crate) fn inspect_game(game_root: &Path) -> Result<(), AppError> {
    println!("game: {}", game_root.display());
    println!("exists: {}", game_root.exists());
    println!();
    print_game_infrastructure(game_root);
    print_game_script_inventory(game_root)
}

fn print_game_infrastructure(game_root: &Path) {
    let checks = [
        ("classic exe", "gta_sa.exe"), // literal: allow external interface text or file-format spelling
        ("steam exe", "gta-sa.exe"), // literal: allow external interface text or file-format spelling
        ("modloader asi", "modloader.asi"), // literal: allow external interface text or file-format spelling
        ("modloader dir", "modloader"), // literal: allow external interface text or file-format spelling
        ("cleo asi", "CLEO.asi"), // literal: allow external interface text or file-format spelling
        ("cleo dir", "CLEO"),     // literal: allow external interface text or file-format spelling
        ("silent asi loader candidate", "vorbisFile.dll"), // literal: allow external interface text or file-format spelling
    ];

    for (label, rel) in checks {
        let path = game_root.join(rel);
        println!(
            "{label:28} {}",
            if path.exists() { "present" } else { "missing" }
        );
    }
}

fn print_game_script_inventory(game_root: &Path) -> Result<(), AppError> {
    let asi_files = list_matching(game_root, |path| extension_eq(path, "asi"))?; // literal: allow external interface text or file-format spelling
    let cleo_scripts = collect_cleo_scripts(game_root);

    println!();
    print_root_asi_files(asi_files);
    print_cleo_scripts(cleo_scripts);

    Ok(())
}

fn collect_cleo_scripts(game_root: &Path) -> Vec<PathBuf> {
    list_matching(
        &game_root.join(
            /* literal: allow external interface text or file-format spelling */ "CLEO",
        ),
        |path| {
            // literal: allow external interface text or file-format spelling
            extension_eq(path, "cs") || extension_eq(path, "cleo") // literal: allow external interface text or file-format spelling
        },
    )
    .unwrap_or_default()
}

fn print_root_asi_files(asi_files: Vec<PathBuf>) {
    println!("root ASI files : {}", asi_files.len());
    for path in asi_files {
        println!("  {}", path.display());
    }
}

fn print_cleo_scripts(cleo_scripts: Vec<PathBuf>) {
    println!("CLEO scripts   : {}", cleo_scripts.len());
    for path in cleo_scripts {
        println!("  {}", path.display());
    }
}
