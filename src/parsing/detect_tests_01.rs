#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn cleo5_script_and_compat_scripts_route_to_cleo()
    {
        for path in ["cleo/speedo.cs", "cleo/legacy.cs4", "cleo/older.cs3"]
        {
            assert_eq!(
                component_paths(path),
                BTreeSet::from([Component::Cleo]),
                "{path} should be a CLEO script"
            );
        }
    }

    #[test]
    fn cleo_modules_and_saves_route_to_their_own_components()
    {
        let modules = component_paths("cleo/cleo_modules/shared.txt");
        assert!(
            modules.contains(&Component::CleoModules),
            "cleo_modules file should be a CLEO module, got {modules:?}"
        );
        assert!(!modules.contains(&Component::Cleo));

        let saves = component_paths("cleo/cleo_saves/slot1.sav");
        assert!(
            saves.contains(&Component::CleoSaves),
            "cleo_saves file should be CLEO save data, got {saves:?}"
        );
        assert!(!saves.contains(&Component::Cleo));
    }

    #[test]
    fn cleo_plugin_routes_to_plugin_component_not_script()
    {
        // A .cleo plugin, whether loose or already under cleo_plugins/.
        for path in ["cleo/SA.IniFiles.cleo", "cleo/cleo_plugins/SA.Audio.cleo"]
        {
            let found = component_paths(path);
            assert!(
                found.contains(&Component::CleoPlugin),
                "{path} should be a CLEO plugin, got {found:?}"
            );
            assert!(
                !found.contains(&Component::Cleo),
                "{path} must not also be a plain CLEO script"
            );
        }
    }

    #[test]
    fn text_key_and_cleo_text_folder_route_to_cleo_text()
    {
        for path in [
            "cleo/cleo_text/string_list.fxt",
            "cleo_text/lang.fxt",
            "loose.fxt",
        ]
        {
            assert_eq!(
                component_paths(path),
                BTreeSet::from([Component::CleoText]),
                "{path} should be CLEO text"
            );
        }
    }

    fn component_paths(path: &str) -> BTreeSet<Component>
    {
        return detect_install_candidate(path, 100) // literal: allow test fixture value is the specimen under judgment
            .unwrap_or_else(|| panic!("no candidate detected for {path}"))
            .component_paths;
    }
}
