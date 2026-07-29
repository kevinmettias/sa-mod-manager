// Per-script CLEO plugin dependency analysis.
//
// A compiled CLEO script (`.cs`/`.cs4`/`.cs3`) is SCM bytecode. It references
// plugin functionality by *opcode number*, not by plugin name, so the only way
// to know which plugins a script needs is to walk its instructions and collect
// the opcodes it uses, then map each opcode to the plugin that provides it.
//
// Two data sources make this possible:
//   * the game's `CLEO/.config/sa.json` opcode database gives each opcode's
//     parameter count (needed to advance the instruction pointer correctly);
//   * CLEO5's `source/.OpcodeMap.csv` assigns opcode *ranges* to plugins.
//
// The walker is deliberately conservative: the moment it meets an opcode it
// cannot size (missing from the database) or a parameter byte it does not
// recognize, it stops and marks the disassembly incomplete rather than guessing
// — so a reported dependency is always one actually reached in clean bytecode.

use crate::prelude::*;
const PLUGIN_RANGE_COUNT: usize = 11;
const OPCODE_ID_RADIX: u32 = 16;
const OPCODE_NOT_FLAG_MASK: u16 = 0x7FFF;
const OPCODE_BYTE_WIDTH: usize = 2;

const CLEO_CORE_FIRST_OPCODE: u16 = 0x2000;
const INPUT_FIRST_OPCODE: u16 = 0x2080;
const DEBUG_UTILS_FIRST_OPCODE: u16 = 0x2100;
const IMGUI_FIRST_OPCODE: u16 = 0x2200;
const FILE_OPERATIONS_FIRST_OPCODE: u16 = 0x2300;
const MEMORY_OPERATIONS_FIRST_OPCODE: u16 = 0x2400;
const AUDIO_FIRST_OPCODE: u16 = 0x2500;
const TEXT_FIRST_OPCODE: u16 = 0x2600;
const MATH_FIRST_OPCODE: u16 = 0x2700;
const INI_FILES_FIRST_OPCODE: u16 = 0x2800;
const SPHERE_FIRST_OPCODE: u16 = 0x2880;

const LEGACY_MEMORY_READ_OPCODE: u16 = 0x0A8C;
const LEGACY_MEMORY_WRITE_OPCODE: u16 = 0x0A8D;
const LEGACY_DLL_LOAD_FIRST_OPCODE: u16 = 0x0AA2;
const LEGACY_DLL_LOAD_LAST_OPCODE: u16 = 0x0AA4;
const FILE_OPERATIONS_LAST_OPCODE: u16 = 0x23FF;
const MEMORY_OPERATIONS_LAST_OPCODE: u16 = 0x24FF;

const PARAM_TYPE_INT32: u8 = 0x01;
const PARAM_TYPE_GLOBAL_NUMBER_VAR: u8 = 0x02;
const PARAM_TYPE_LOCAL_NUMBER_VAR: u8 = 0x03;
const PARAM_TYPE_INT8: u8 = 0x04;
const PARAM_TYPE_INT16: u8 = 0x05;
const PARAM_TYPE_FLOAT32: u8 = 0x06;
const PARAM_TYPE_GLOBAL_NUMBER_ARRAY: u8 = 0x07;
const PARAM_TYPE_LOCAL_NUMBER_ARRAY: u8 = 0x08;
const PARAM_TYPE_SHORT_STRING_IMMEDIATE: u8 = 0x09;
const PARAM_TYPE_GLOBAL_SHORT_STRING_VAR: u8 = 0x0A;
const PARAM_TYPE_LOCAL_SHORT_STRING_VAR: u8 = 0x0B;
const PARAM_TYPE_GLOBAL_SHORT_STRING_ARRAY: u8 = 0x0C;
const PARAM_TYPE_LOCAL_SHORT_STRING_ARRAY: u8 = 0x0D;
const PARAM_TYPE_VARIABLE_STRING: u8 = 0x0E;
const PARAM_TYPE_LONG_STRING_IMMEDIATE: u8 = 0x0F;
const PARAM_TYPE_GLOBAL_LONG_STRING_VAR: u8 = 0x10;
const PARAM_TYPE_LOCAL_LONG_STRING_VAR: u8 = 0x11;
const PARAM_TYPE_GLOBAL_LONG_STRING_ARRAY: u8 = 0x12;
const PARAM_TYPE_LOCAL_LONG_STRING_ARRAY: u8 = 0x13;

const PARAM_WIDTH_INT8: usize = 1;
const PARAM_WIDTH_WORD: usize = 2;
const PARAM_WIDTH_DWORD: usize = 4;
const PARAM_WIDTH_ARRAY: usize = 6;
const PARAM_WIDTH_SHORT_STRING: usize = 8;
const PARAM_WIDTH_LONG_STRING: usize = 16;

/// One CLEO5 plugin opcode range. Opcodes at or above `first_opcode` (until the
/// next range) are provided by `plugin`. Transcribed from CLEO5
/// `source/.OpcodeMap.csv` (ids are hex; the table is range-based).
struct PluginRange
{
    first_opcode: u16,
    plugin: &'static str,
}

/// Plugin opcode ranges, ascending by `first_opcode`. `CLEO core` is the engine
/// itself (no plugin file); the rest correspond to bundled `.cleo` modules.
const PLUGIN_RANGES: [PluginRange; PLUGIN_RANGE_COUNT] = [
    PluginRange {
        first_opcode: CLEO_CORE_FIRST_OPCODE,
        plugin: "CLEO core",
    },
    PluginRange {
        first_opcode: INPUT_FIRST_OPCODE,
        plugin: "Input",
    },
    PluginRange {
        first_opcode: DEBUG_UTILS_FIRST_OPCODE,
        plugin: "DebugUtils",
    },
    PluginRange {
        first_opcode: IMGUI_FIRST_OPCODE,
        plugin: "ImGUI",
    },
    PluginRange {
        first_opcode: FILE_OPERATIONS_FIRST_OPCODE,
        plugin: "FileOperations",
    },
    PluginRange {
        first_opcode: MEMORY_OPERATIONS_FIRST_OPCODE,
        plugin: "MemoryOperations",
    },
    PluginRange {
        first_opcode: AUDIO_FIRST_OPCODE,
        plugin: "Audio",
    },
    PluginRange {
        first_opcode: TEXT_FIRST_OPCODE,
        plugin: "Text",
    },
    PluginRange {
        first_opcode: MATH_FIRST_OPCODE,
        plugin: "Math",
    },
    PluginRange {
        first_opcode: INI_FILES_FIRST_OPCODE,
        plugin: "IniFiles",
    },
    PluginRange {
        first_opcode: SPHERE_FIRST_OPCODE,
        plugin: "Sphere",
    },
];

/// Load and parse the opcode database from `CLEO/.config/sa.json`. Tolerant of
/// unknown fields and shapes: any entry missing an id/params is simply skipped.
/// Returns `None` if the file is absent or not valid JSON.
pub(crate) fn load_opcode_db(sa_json_path: &Path) -> Option<ScmOpcodeDb>
{
    let text = fs::read_to_string(sa_json_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let mut opcodes = BTreeMap::new();
    for extension in value.get("extensions")?.as_array()?
    {
        let Some(commands) = extension.get("commands").and_then(|c| c.as_array()) else {
            continue;
        };
        for command in commands
        {
            let Some(id) = command
                .get("id")
                .and_then(|id| id.as_str())
                .and_then(|id| u16::from_str_radix(id, OPCODE_ID_RADIX).ok())
            else {
                continue;
            };
            let num_params = command
                .get("num_params")
                .and_then(|n| n.as_u64())
                .unwrap_or(0) as u16;
            let is_variadic = command
                .get("attrs")
                .and_then(|attrs| attrs.get("is_variadic"))
                .and_then(|flag| flag.as_bool())
                .unwrap_or(false);
            opcodes.insert(
                id & OPCODE_NOT_FLAG_MASK,
                OpcodeSig {
                    num_params,
                    is_variadic,
                },
            );
        }
    }
    return Some(ScmOpcodeDb { opcodes });
}

/// Analyze one script's bytecode against the opcode database.
pub(crate) fn analyze_script(bytes: &[u8], db: &ScmOpcodeDb) -> ScriptDeps
{
    let mut used = BTreeSet::new();
    let complete = collect_opcodes(bytes, db, &mut used);
    let mut plugins = BTreeSet::new();
    let mut unmapped = BTreeSet::new();
    let mut capabilities = BTreeSet::new();
    for opcode in used
    {
        if let Some(capability) = capability_for_opcode(opcode)
        {
            capabilities.insert(capability);
        }
        let Some(extension) = extension_for_opcode(opcode) else {
            continue;
        };
        if extension == "CLEO core"
        {
            continue;
        }
        match plugin_file_for_extension(extension)
        {
            Some(file) => {
                plugins.insert(file.to_string());
            }
            None => {
                unmapped.insert(extension.to_string());
            }
        }
    }
    return ScriptDeps {
        plugins,
        unmapped_extensions: unmapped,
        capabilities,
        complete,
    };
}

/// An opcode's signature, enough to skip its arguments: how many parameters it
/// takes, and whether that count is variable (terminated by a `0x00` byte).
struct OpcodeSig
{
    num_params: u16,
    is_variadic: bool,
}

/// The SCM opcode database, parsed from `sa.json`: masked opcode id → signature.
pub(crate) struct ScmOpcodeDb
{
    opcodes: BTreeMap<u16, OpcodeSig>,
}

impl ScmOpcodeDb
{
    #[cfg(test)]
    fn from_pairs(pairs: &[(u16, u16, bool)]) -> Self
    {
        let opcodes = pairs
            .iter()
            .map(|(id, num_params, is_variadic)| {
                (
                    id & OPCODE_NOT_FLAG_MASK,
                    OpcodeSig {
                        num_params: *num_params,
                        is_variadic: *is_variadic,
                    },
                )
            })
            .collect();
        return Self { opcodes };
    }

    fn get(&self, opcode: u16) -> Option<&OpcodeSig>
    {
        return self.opcodes.get(&opcode);
    }
}

/// Walk the bytecode, recording each opcode used. Returns whether it reached the
/// end cleanly; a `false` means an unknown opcode or parameter type stopped it.
fn collect_opcodes(bytes: &[u8], db: &ScmOpcodeDb, used: &mut BTreeSet<u16>) -> bool
{
    let mut pos = 0usize;
    // A trailing 0/1 byte is a clean end (an opcode needs two bytes).
    while pos + OPCODE_BYTE_WIDTH <= bytes.len()
    {
        let opcode = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
        pos += OPCODE_BYTE_WIDTH;
        // The high bit is the "not" flag on conditional opcodes; mask for lookup.
        let masked = opcode & OPCODE_NOT_FLAG_MASK;
        let Some(sig) = db.get(masked) else {
            return false;
        };
        used.insert(masked);
        if sig.is_variadic
        {
            if !skip_variadic_params(bytes, &mut pos)
            {
                return false;
            }
        }
        else if !skip_fixed_params(bytes, &mut pos, sig.num_params)
        {
            return false;
        }
    }
    return true;
}

/// What a script needs: the bundled plugin files it depends on, any plugin
/// extensions with no known bundled file, the elevated capabilities it exercises,
/// and whether the walk finished cleanly.
pub(crate) struct ScriptDeps
{
    /// Required bundled plugin files, by stem (e.g. `SA.IniFiles`).
    pub(crate) plugins: BTreeSet<String>,
    /// Plugin extensions used but not mapped to a known file (e.g. `ImGUI`).
    pub(crate) unmapped_extensions: BTreeSet<String>,
    /// Elevated capabilities the script uses (memory access, DLL loading, file
    /// I/O) — a trust hint when reviewing an unknown script.
    pub(crate) capabilities: BTreeSet<&'static str>,
    /// False if disassembly stopped early; `plugins` may then be incomplete.
    pub(crate) complete: bool,
}

/// Skip a variadic parameter list, terminated by a `0x00` (EOL) type byte.
fn skip_variadic_params(bytes: &[u8], pos: &mut usize) -> bool
{
    while *pos < bytes.len()
    {
        let type_byte = bytes[*pos];
        *pos += 1;
        if type_byte == 0x00
        {
            return true;
        }
        if !skip_param_data(type_byte, bytes, pos)
        {
            return false;
        }
    }
    return false;
}

/// Skip a fixed number of parameters. Returns false on a truncated or
/// unrecognized parameter.
fn skip_fixed_params(bytes: &[u8], pos: &mut usize, num_params: u16) -> bool
{
    for _ in 0..num_params
    {
        if !skip_one_param(bytes, pos)
        {
            return false;
        }
    }
    return true;
}

/// Read one parameter's type byte and skip its data.
fn skip_one_param(bytes: &[u8], pos: &mut usize) -> bool
{
    let Some(&type_byte) = bytes.get(*pos) else {
        return false;
    };
    *pos += 1;
    return skip_param_data(type_byte, bytes, pos);
}

/// An elevated capability an opcode confers, or `None` for an ordinary opcode.
/// Covers the legacy CLEO memory/DLL opcodes and the CLEO5 MemoryOperations /
/// FileOperations plugin ranges.
fn capability_for_opcode(opcode: u16) -> Option<&'static str>
{
    return match opcode
    {
        LEGACY_MEMORY_READ_OPCODE | LEGACY_MEMORY_WRITE_OPCODE => {
            Some("reads/writes process memory")
        }
        LEGACY_DLL_LOAD_FIRST_OPCODE..=LEGACY_DLL_LOAD_LAST_OPCODE => Some("loads native DLLs"),
        MEMORY_OPERATIONS_FIRST_OPCODE..=MEMORY_OPERATIONS_LAST_OPCODE => Some("memory operations"),
        FILE_OPERATIONS_FIRST_OPCODE..=FILE_OPERATIONS_LAST_OPCODE => Some("file system access"),
        _ => None,
    };
}

/// The plugin extension owning `opcode`, i.e. the range with the greatest
/// `first_opcode` not exceeding it. `None` for opcodes below the plugin block.
fn extension_for_opcode(opcode: u16) -> Option<&'static str>
{
    let mut found = None;
    for range in &PLUGIN_RANGES
    {
        if opcode >= range.first_opcode
        {
            found = Some(range.plugin);
        }
        else
        {
            break;
        }
    }
    return found;
}

/// The `.cleo` plugin file (stem) that provides a CSV extension, or `None` for
/// the CLEO core (no file) or an extension with no known bundled file.
fn plugin_file_for_extension(extension: &str) -> Option<&'static str>
{
    return match extension
    {
        "Input" => Some("SA.Input"),
        "DebugUtils" => Some("SA.DebugUtils"),
        "FileOperations" => Some("SA.FileSystemOperations"),
        "MemoryOperations" => Some("SA.MemoryOperations"),
        "Audio" => Some("SA.Audio"),
        "Text" => Some("SA.Text"),
        "Math" => Some("SA.Math"),
        "IniFiles" => Some("SA.IniFiles"),
        // ImGUI/Sphere/CLEO core have no bundled SA.*.cleo mapping here.
        _ => None,
    };
}

/// Advance `pos` past the data of a parameter whose type byte was already read.
/// Returns false for an unknown type byte or a truncated variable-length string.
fn skip_param_data(type_byte: u8, bytes: &[u8], pos: &mut usize) -> bool
{
    // GTA SA SCM parameter data sizes, keyed by the data-type byte.
    let size = match type_byte {
        PARAM_TYPE_INT32 => PARAM_WIDTH_DWORD, // int32 immediate
        PARAM_TYPE_GLOBAL_NUMBER_VAR | PARAM_TYPE_LOCAL_NUMBER_VAR => PARAM_WIDTH_WORD, // global / local number var
        PARAM_TYPE_INT8 => PARAM_WIDTH_INT8,  // int8 immediate
        PARAM_TYPE_INT16 => PARAM_WIDTH_WORD, // int16 immediate
        PARAM_TYPE_FLOAT32 => PARAM_WIDTH_DWORD, // float32 immediate
        PARAM_TYPE_GLOBAL_NUMBER_ARRAY | PARAM_TYPE_LOCAL_NUMBER_ARRAY => PARAM_WIDTH_ARRAY, // global / local number array
        PARAM_TYPE_SHORT_STRING_IMMEDIATE => PARAM_WIDTH_SHORT_STRING, // immediate 8-byte string
        PARAM_TYPE_GLOBAL_SHORT_STRING_VAR | PARAM_TYPE_LOCAL_SHORT_STRING_VAR => PARAM_WIDTH_WORD, // global / local short-string var
        PARAM_TYPE_GLOBAL_SHORT_STRING_ARRAY | PARAM_TYPE_LOCAL_SHORT_STRING_ARRAY => {
            PARAM_WIDTH_ARRAY
        } // global / local short-string array
        PARAM_TYPE_VARIABLE_STRING => {
            // Variable-length string: one length byte, then that many chars.
            let Some(&len) = bytes.get(*pos) else {
                return false;
            };
            PARAM_WIDTH_INT8 + len as usize
        }
        PARAM_TYPE_LONG_STRING_IMMEDIATE => PARAM_WIDTH_LONG_STRING, // immediate 16-byte string
        PARAM_TYPE_GLOBAL_LONG_STRING_VAR | PARAM_TYPE_LOCAL_LONG_STRING_VAR => PARAM_WIDTH_WORD, // global / local long-string var
        PARAM_TYPE_GLOBAL_LONG_STRING_ARRAY | PARAM_TYPE_LOCAL_LONG_STRING_ARRAY => {
            PARAM_WIDTH_ARRAY
        } // global / local long-string array
        _ => return false, // unknown/unsupported type: bail
    };
    *pos += size;
    return *pos <= bytes.len();
}

#[cfg(test)]
mod tests
{
    include!("part_01_tests_01.rs");
}

