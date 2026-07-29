
// --- CLEO viewer ----------------------------------------------------------

/// A CLEO script (.cs/.cs4/.cleo …) paired with the companion files that ship
/// alongside it (its `.ini`, `.fxt`, data files sharing the same name), so the
/// viewer can present a script as the unit a modder actually installs.
pub(crate) struct CleoScript<'a>
{
    pub(crate) script: &'a ContentEntry,
    pub(crate) companions: Vec<&'a ContentEntry>,
}

/// The CLEO viewer's model: plugin modules (`.cleo`), scripts with their
/// companions, plus loose files in the CLEO folder that belong to no script.
pub(crate) struct CleoView<'a>
{
    pub(crate) plugins: Vec<&'a ContentEntry>,
    pub(crate) scripts: Vec<CleoScript<'a>>,
    pub(crate) loose: Vec<&'a ContentEntry>,
}

/// Split CLEO-category entries into plugins, scripts-with-companions and loose
/// files. A companion is a non-script file sharing a script's directory and base
/// name (e.g. `cleo/speedo.cs` owns `cleo/speedo.ini`). Plugin modules (`.cleo`,
/// which CLEO5 loads from `cleo_plugins/`) are their own unit, not script
/// companions.
pub(crate) fn cleo_view<'a>(entries: &[&'a ContentEntry]) -> CleoView<'a>
{
    let mut plugins: Vec<&ContentEntry> = Vec::new();
    let mut scripts: Vec<&ContentEntry> = Vec::new();
    let mut companions_by_key: BTreeMap<(String, String), Vec<&'a ContentEntry>> = BTreeMap::new();
    for &entry in entries
    {
        if is_cleo_plugin(&entry.target)
        {
            plugins.push(entry);
        }
        else if is_cleo_script(&entry.target)
        {
            scripts.push(entry);
        }
        else
        {
            let key = dir_and_stem(&entry.target);
            companions_by_key
                .entry((key.dir.to_string(), key.stem.to_string()))
                .or_default()
                .push(entry);
        }
    }
    plugins.sort_by(|a, b| a.target.cmp(&b.target));
    scripts.sort_by(|a, b| a.target.cmp(&b.target));

    let script_keys: BTreeSet<(String, String)> = scripts
        .iter()
        .map(|script| {
            let key = dir_and_stem(&script.target);
            (key.dir.to_string(), key.stem.to_string())
        })
        .collect();

    let script_rows = scripts
        .iter()
        .map(|&script| {
            let key = dir_and_stem(&script.target);
            let mut companions = companions_by_key
                .get(&(key.dir.to_string(), key.stem.to_string()))
                .cloned()
                .unwrap_or_default();
            companions.sort_by(|a, b| a.target.cmp(&b.target));
            CleoScript { script, companions }
        })
        .collect();
    let loose = companions_by_key
        .iter()
        .filter(|(key, _)| !script_keys.contains(*key))
        .flat_map(|(_, files)| files.iter().copied())
        .collect();
    return CleoView {
        plugins,
        scripts: script_rows,
        loose,
    };
}

/// A CLEO5 plugin module (`.cleo`), loaded from `cleo/cleo_plugins/`.
fn is_cleo_plugin(target: &str) -> bool
{
    return target.to_ascii_lowercase().ends_with(".cleo");
}

/// A CLEO script: `.cs` (CLEO5), `.cs4` (CLEO4 compat), `.cs3` (CLEO3 compat).
/// Uses the shared extension list so this never drifts from detection.
fn is_cleo_script(target: &str) -> bool
{
    return has_cleo_script_extension(&target.to_ascii_lowercase());
}

/// A target's directory and file stem (name without its final extension), used
/// to pair CLEO companions with their script.
struct CleoTargetKey<'a>
{
    dir: &'a str,
    stem: &'a str,
}

fn dir_and_stem(target: &str) -> CleoTargetKey<'_>
{
    let (dir, name) = match target.rfind('/') {
        Some(index) => (&target[..index], &target[index + 1..]),
        None => ("", target),
    };
    let stem = match name.rfind('.') {
        Some(index) => &name[..index],
        None => name,
    };
    return CleoTargetKey { dir, stem };
}

// --- ASI viewer -----------------------------------------------------------

/// The ASI viewer's model: actual `.asi` plugins, the loader/proxy DLLs that
/// bootstrap them (Ultimate ASI Loader and friends), and anything else.
pub(crate) struct AsiView<'a>
{
    pub(crate) plugins: Vec<&'a ContentEntry>,
    pub(crate) loaders: Vec<&'a ContentEntry>,
    pub(crate) other: Vec<&'a ContentEntry>,
}

/// Sort ASI-category entries into plugins, loader/proxy DLLs, and other files.
pub(crate) fn asi_view<'a>(entries: &[&'a ContentEntry]) -> AsiView<'a>
{
    let mut plugins = Vec::new();
    let mut loaders = Vec::new();
    let mut other = Vec::new();
    for &entry in entries
    {
        if entry.target.to_ascii_lowercase().ends_with(".asi")
        {
            plugins.push(entry);
        }
        else if is_asi_loader(&entry.target)
        {
            loaders.push(entry);
        }
        else
        {
            other.push(entry);
        }
    }
    for bucket in [&mut plugins, &mut loaders, &mut other]
    {
        bucket.sort_by(|a, b| a.target.cmp(&b.target));
    }
    return AsiView {
        plugins,
        loaders,
        other,
    };
}

/// Known ASI loader / proxy DLL file names. These hook the game's startup to
/// load `.asi` plugins; normally exactly one should be present.
fn is_asi_loader(target: &str) -> bool
{
    const LOADERS: [&str; 8] = [
        "dinput8.dll",
        "dinput.dll",
        "vorbisfile.dll",
        "vorbishooked.dll",
        "dsound.dll",
        "d3d9.dll",
        "ddraw.dll",
        "dxwrapper.dll",
    ];
    let name = target
        .rsplit('/')
        .next()
        .unwrap_or(target)
        .to_ascii_lowercase();
    return LOADERS.contains(&name.as_str());
}

#[cfg(test)]
mod tests
{
    include!("part_04_tests_01.rs");
    include!("part_04_tests_02.rs");
    include!("part_04_tests_03.rs");
}


