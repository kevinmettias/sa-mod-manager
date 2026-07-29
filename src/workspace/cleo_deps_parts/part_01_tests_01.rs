    use super::{AUDIO_FIRST_OPCODE, CLEO_CORE_FIRST_OPCODE, IMGUI_FIRST_OPCODE, INI_FILES_FIRST_OPCODE, INPUT_FIRST_OPCODE, LEGACY_MEMORY_READ_OPCODE, MATH_FIRST_OPCODE, MEMORY_OPERATIONS_FIRST_OPCODE, PARAM_TYPE_INT32, PARAM_TYPE_VARIABLE_STRING, SPHERE_FIRST_OPCODE, ScmOpcodeDb, TEXT_FIRST_OPCODE, analyze_script, extension_for_opcode};
    use crate::prelude::*;
    const BEFORE_PLUGIN_BLOCK_OPCODE: u16 = 0x1FFF;
    const CLEO_CORE_LAST_TEST_OPCODE: u16 = 0x207F;
    const AUDIO_RANGE_TEST_OPCODE: u16 = 0x2599;
    const ABOVE_LAST_PLUGIN_RANGE_OPCODE: u16 = 0x9000;
    const WAIT_OPCODE: u16 = 0x0001;
    const TEST_WAIT_DURATION: i32 = 250;
    const UNKNOWN_TEST_OPCODE: u16 = 0x9999;
    const TEST_STRING_LENGTH: u8 = 2;
    const SECOND_VARIADIC_TEST_PARAM: i32 = 2;


    #[test]
    fn extension_lookup_is_range_based()
    {
        assert_eq!(extension_for_opcode(BEFORE_PLUGIN_BLOCK_OPCODE), None); // below the plugin block
        assert_eq!(
            extension_for_opcode(CLEO_CORE_FIRST_OPCODE),
            Some("CLEO core")
        );
        assert_eq!(
            extension_for_opcode(CLEO_CORE_LAST_TEST_OPCODE),
            Some("CLEO core")
        );
        assert_eq!(extension_for_opcode(INPUT_FIRST_OPCODE), Some("Input"));
        assert_eq!(extension_for_opcode(AUDIO_FIRST_OPCODE), Some("Audio"));
        assert_eq!(extension_for_opcode(AUDIO_RANGE_TEST_OPCODE), Some("Audio")); // within Audio's range
        assert_eq!(
            extension_for_opcode(INI_FILES_FIRST_OPCODE),
            Some("IniFiles")
        );
        assert_eq!(extension_for_opcode(SPHERE_FIRST_OPCODE), Some("Sphere"));
        assert_eq!(
            extension_for_opcode(ABOVE_LAST_PLUGIN_RANGE_OPCODE),
            Some("Sphere")
        ); // above the last start
    }

    #[test]
    fn walker_extracts_plugin_dependencies_from_clean_bytecode()
    {
        // 0x0001 WAIT(1 param); 0x2800 an IniFiles opcode(1 param); 0x2500 Audio(0).
        let db = ScmOpcodeDb::from_pairs(&[
            (WAIT_OPCODE, 1, false),
            (INI_FILES_FIRST_OPCODE, 1, false),
            (AUDIO_FIRST_OPCODE, 0, false),
        ]);
        let mut bytes = Vec::new();
        bytes.extend(opcode(WAIT_OPCODE));
        bytes.extend(int32_param(TEST_WAIT_DURATION));
        bytes.extend(opcode(INI_FILES_FIRST_OPCODE));
        bytes.extend(int32_param(0));
        bytes.extend(opcode(AUDIO_FIRST_OPCODE));

        let deps = analyze_script(&bytes, &db);
        assert!(deps.complete);
        assert_eq!(
            deps.plugins,
            BTreeSet::from(["SA.Audio".to_string(), "SA.IniFiles".to_string()])
        );
        assert!(deps.unmapped_extensions.is_empty());
    }

    #[test]
    fn elevated_capabilities_are_flagged_from_opcodes()
    {
        // 0x0A8C legacy write-memory; 0x2400 a MemoryOperations opcode.
        let db = ScmOpcodeDb::from_pairs(&[
            (LEGACY_MEMORY_READ_OPCODE, 0, false),
            (MEMORY_OPERATIONS_FIRST_OPCODE, 0, false),
        ]);
        let mut bytes = opcode(LEGACY_MEMORY_READ_OPCODE);
        bytes.extend(opcode(MEMORY_OPERATIONS_FIRST_OPCODE));
        let deps = analyze_script(&bytes, &db);
        assert!(deps.capabilities.contains("reads/writes process memory"));
        assert!(deps.capabilities.contains("memory operations"));
        assert!(!deps.capabilities.contains("file system access"));
    }

    #[test]
    fn unmapped_plugin_extension_is_reported_separately()
    {
        // 0x2200 is ImGUI, which has no bundled SA.*.cleo mapping.
        let db = ScmOpcodeDb::from_pairs(&[(IMGUI_FIRST_OPCODE, 0, false)]);
        let deps = analyze_script(&opcode(IMGUI_FIRST_OPCODE), &db);
        assert!(deps.complete);
        assert!(deps.plugins.is_empty());
        assert_eq!(
            deps.unmapped_extensions,
            BTreeSet::from(["ImGUI".to_string()])
        );
    }

    #[test]
    fn unknown_opcode_stops_the_walk_without_guessing()
    {
        // DB knows the first opcode but not the second.
        let db = ScmOpcodeDb::from_pairs(&[(AUDIO_FIRST_OPCODE, 0, false)]);
        let mut bytes = opcode(AUDIO_FIRST_OPCODE);
        bytes.extend(opcode(UNKNOWN_TEST_OPCODE)); // not in the DB
        bytes.extend(opcode(INI_FILES_FIRST_OPCODE)); // would be IniFiles, but unreachable now

        let deps = analyze_script(&bytes, &db);
        assert!(
            !deps.complete,
            "an unknown opcode must mark the walk incomplete"
        );
        assert_eq!(deps.plugins, BTreeSet::from(["SA.Audio".to_string()]));
    }

    #[test]
    fn variadic_opcode_consumes_params_until_eol()
    {
        // 0x2500 variadic, two int params then 0x00 EOL; then 0x2800 IniFiles.
        let db = ScmOpcodeDb::from_pairs(&[
            (AUDIO_FIRST_OPCODE, 0, true),
            (INI_FILES_FIRST_OPCODE, 0, false),
        ]);
        let mut bytes = opcode(AUDIO_FIRST_OPCODE);
        bytes.extend(int32_param(1));
        bytes.extend(int32_param(SECOND_VARIADIC_TEST_PARAM));
        bytes.push(0x00); // EOL
        bytes.extend(opcode(INI_FILES_FIRST_OPCODE));

        let deps = analyze_script(&bytes, &db);
        assert!(deps.complete);
        assert_eq!(
            deps.plugins,
            BTreeSet::from(["SA.Audio".to_string(), "SA.IniFiles".to_string()])
        );
    }

    #[test]
    fn variable_length_string_param_is_skipped_by_its_length_byte()
    {
        // 0x2600 Text opcode with one 0x0E var-length string param "hi".
        let db = ScmOpcodeDb::from_pairs(&[
            (TEXT_FIRST_OPCODE, 1, false),
            (MATH_FIRST_OPCODE, 0, false),
        ]);
        let mut bytes = opcode(TEXT_FIRST_OPCODE);
        bytes.push(PARAM_TYPE_VARIABLE_STRING); // var-length string type
        bytes.push(TEST_STRING_LENGTH); // length
        bytes.extend_from_slice(b"hi");
        bytes.extend(opcode(MATH_FIRST_OPCODE)); // Math, reached only if the string was sized right

        let deps = analyze_script(&bytes, &db);
        assert!(deps.complete);
        assert_eq!(
            deps.plugins,
            BTreeSet::from(["SA.Math".to_string(), "SA.Text".to_string()])
        );
    }

    /// One int32 param: type byte 0x01 + 4 data bytes.
    fn int32_param(value: i32) -> Vec<u8>
    {
        let mut param = vec![PARAM_TYPE_INT32];
        param.extend_from_slice(&value.to_le_bytes());
        return param;
    }

    fn opcode(id: u16) -> Vec<u8>
    {
        return id.to_le_bytes().to_vec();
    }
