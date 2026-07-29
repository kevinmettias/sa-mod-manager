#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn cleo_runtime_detects_version_from_layout()
    {
        let none = temporary_directory("runtime-none");
        assert_eq!(detect_cleo_runtime(&none), CleoRuntime::Absent);

        let four = temporary_directory("runtime-4");
        fs::create_dir_all(four.join("CLEO").join("cleo_modules")).expect("the test fixture is created before this assertion reads it");
        assert_eq!(detect_cleo_runtime(&four), CleoRuntime::Cleo4);

        let five = temporary_directory("runtime-5");
        fs::create_dir_all(five.join("CLEO")).expect("the test fixture is created before this assertion reads it");
        fs::write(five.join("CLEO").join(".cleo_config.ini"), "").expect("the test fixture is created before this assertion reads it");
        assert_eq!(detect_cleo_runtime(&five), CleoRuntime::Cleo5);

        fs::remove_dir_all(&none).expect("the test fixture is created before this assertion reads it");
        fs::remove_dir_all(&four).expect("the test fixture is created before this assertion reads it");
        fs::remove_dir_all(&five).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn cleo_script_version_maps_extension_to_runtime()
    {
        assert_eq!(cleo_script_version(Path::new("a/speedo.cs")), "CLEO5");
        assert_eq!(
            cleo_script_version(Path::new("a/legacy.cs4")),
            "CLEO4 compat"
        );
        assert_eq!(
            cleo_script_version(Path::new("a/older.cs3")),
            "CLEO3 compat"
        );
    }

    #[test]
    fn bundled_plugin_status_marks_installed_and_missing()
    {
        let installed = BTreeSet::from(["sa.inifiles".to_string(), "sa.audio".to_string()]);
        let status = bundled_plugin_status(&installed);
        let present: BTreeSet<&str> = status
            .iter()
            .filter(|(_, ok)| *ok)
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(present, BTreeSet::from(["SA.Audio", "SA.IniFiles"]));
        // Every bundled plugin is accounted for, present or not.
        assert_eq!(status.len(), BUNDLED_CLEO5_PLUGINS.len());
        assert!(status.iter().any(|(name, ok)| *name == "SA.Text" && !ok));
    }

    #[test]
    fn collect_cleo_diagnostics_finds_blacklist_and_text_key_conflicts()
    {
        let base = temporary_directory("diag");
        let game = base.join("game");
        let cleo = game.join("CLEO");
        fs::create_dir_all(cleo.join("cleo_plugins")).expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(cleo.join("cleo_text")).expect("the test fixture is created before this assertion reads it");
        fs::write(cleo.join(".cleo_config.ini"), "").expect("the test fixture is created before this assertion reads it"); // marks CLEO5
        // A blacklisted legacy plugin, and two .fxt defining the same key.
        fs::write(cleo.join("cleo_plugins").join("IniFiles.cleo"), b"MZ..").expect("the test fixture is created before this assertion reads it");
        fs::write(cleo.join("cleo_text").join("left.fxt"), "HELLO hi\nONLY_A x\n").expect("the test fixture is created before this assertion reads it");
        fs::write(cleo.join("cleo_text").join("right.fxt"), "HELLO bye\n").expect("the test fixture is created before this assertion reads it");

        let diag = collect_cleo_diagnostics(&game);
        assert_eq!(diag.blacklisted_plugins, vec!["IniFiles.cleo"]);
        assert_eq!(diag.fxt_conflicts.len(), 1, "only HELLO is shared");
        assert_eq!(diag.fxt_conflicts[0].key, "HELLO");
        assert_eq!(diag.fxt_conflicts[0].files, vec!["left.fxt", "right.fxt"]);
        assert!(!diag.is_empty());
        fs::remove_dir_all(&base).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn cleo_module_check_reads_header_magic()
    {
        let dir = temporary_directory("module");
        let good = dir.join("lib.s");
        fs::write(&good, [0xFF, 0x7F, 0xFE, 0x00, 0x00, 0x01, 0x02]).expect("the test fixture is created before this assertion reads it"); // literal: allow test fixture value is the specimen under judgment
        let bad = dir.join("bad.s");
        fs::write(&bad, b"not a module").expect("the test fixture is created before this assertion reads it");
        assert!(is_cleo_module(&good));
        assert!(!is_cleo_module(&bad));
        fs::remove_dir_all(&dir).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn portable_executable_image_check_distinguishes_library_from_bytecode()
    {
        let dir = temporary_directory("pe");
        let dll = dir.join("plugin.cleo");
        // A PE image starts with the "MZ" DOS header magic.
        fs::write(&dll, b"MZ\x90\x00rest of a dll").expect("the test fixture is created before this assertion reads it");
        let script = dir.join("script.cs");
        // A compiled CLEO script is SCM bytecode, here a goto opcode_bytes 0002.
        fs::write(&script, b"\x02\x00\x01\x00\x00\x00\x00").expect("the test fixture is created before this assertion reads it");
        let empty = dir.join("empty.cleo");
        fs::write(&empty, b"").expect("the test fixture is created before this assertion reads it");

        assert!(is_pe_image(&dll));
        assert!(!is_pe_image(&script));
        assert!(!is_pe_image(&empty));
        assert!(!is_pe_image(&dir.join("missing.cleo")));

        fs::remove_dir_all(&dir).expect("the test fixture is created before this assertion reads it");
    }

    fn temporary_directory(tag: &str) -> PathBuf
    {
        let dir = env::temp_dir().join(format!(
            "sa-mod-manager-cleo-{tag}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        fs::create_dir_all(&dir).expect("the test fixture is created before this assertion reads it");
        return dir;
    }
}
