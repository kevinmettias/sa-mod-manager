
fn target_order(kind: &TargetKind) -> u8
{
    return match kind
    {
        TargetKind::Bootstrap => 0,
        TargetKind::ModLoader => 1,
        // CLEO scripts, then their modules/plugins/text/saves on top, before ASI.
        TargetKind::Cleo => TARGET_ORDER_CLEO,
        TargetKind::CleoModules => TARGET_ORDER_CLEO_MODULES,
        TargetKind::CleoPlugin => TARGET_ORDER_CLEO_PLUGIN,
        TargetKind::CleoText => TARGET_ORDER_CLEO_TEXT,
        TargetKind::CleoSaves => TARGET_ORDER_CLEO_SAVES,
        TargetKind::Asi => TARGET_ORDER_ASI,
        TargetKind::DirectManaged => TARGET_ORDER_DIRECT_MANAGED,
    };
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test] // literal: allow test fixture value is the specimen under judgment
    fn cleo_components_route_to_their_own_folders()
    {
        // literal: allow test fixture value is the specimen under judgment
        let game = Path::new("/game"); // literal: allow test fixture value is the specimen under judgment
        let cases = [
            // literal: allow test fixture value is the specimen under judgment
            (Component::Cleo, game.join("CLEO")), // literal: allow test fixture value is the specimen under judgment
            (Component::CleoText, game.join("CLEO").join("cleo_text")),
            (
                Component::CleoPlugin,
                game.join("CLEO").join("cleo_plugins"), // literal: allow test fixture value is the specimen under judgment
            ), // literal: allow test fixture value is the specimen under judgment
            (
                // literal: allow test fixture value is the specimen under judgment
                Component::CleoModules,
                game.join("CLEO").join("cleo_modules"), // literal: allow test fixture value is the specimen under judgment
            ),
            (Component::CleoSaves, game.join("CLEO").join("cleo_saves")),
        ];
        for (component, expected) in cases
        {
            let kind = target_kind(&candidate_with(component));
            assert_eq!(target_root_for(&kind, game, "pkg"), expected);
        }
    }

    fn candidate_with(component: Component) -> InstallCandidate
    {
        return InstallCandidate {
            source_root: "pkg".to_string(),
            target_strategy: "CLEO".to_string(),
            file_count: 1,
            total_bytes: 1,
            components: BTreeSet::from([component]),
            notes: BTreeSet::new(),
        };
    }

    #[test]
    fn source_stats_index_matches_exact_and_prefix_but_not_lookalikes()
    {
        let index = index_of(&[
            ("data", TEST_DATA_BYTES),
            ("data.bak", TEST_LOOKALIKE_BYTES), // lookalike: sorts between "data" and "data/" but must not match
            ("data/handling.cfg", TEST_HANDLING_BYTES),
            ("data/anim/x", TEST_ANIM_BYTES),
            ("models/a.dff", TEST_MODEL_BYTES),
        ]);

        // exact ("data") + everything under "data/", excluding "data.bak"
        assert_eq!(
            index.stats("data"),
            SourceStats { file_count: EXPECTED_DATA_ENTRY_COUNT, total_bytes: EXPECTED_DATA_BYTES }
        );
        assert_eq!(index.stats("data/anim"), SourceStats { file_count: 1, total_bytes: TEST_ANIM_BYTES });
        assert_eq!(index.stats("models"), SourceStats { file_count: 1, total_bytes: TEST_MODEL_BYTES });
        assert_eq!(index.stats("missing"), SourceStats { file_count: 0, total_bytes: 0 });
        assert_eq!(
            index.stats("."),
            SourceStats { file_count: EXPECTED_ROOT_ENTRY_COUNT, total_bytes: EXPECTED_ROOT_BYTES }
        );
    }

    fn index_of(paths: &[(&str, u64)]) -> SourceStatsIndex
    {
        let mut entries: Vec<(String, u64)> = paths
            .iter()
            .map(|(path, size)| (normalize_path(path), *size))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let total = SourceStats { file_count: entries.len(), total_bytes: entries.iter().map(|(_, size)| size).sum() };
        return SourceStatsIndex { entries, total };
    }
}

