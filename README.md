# SA Mod Manager

Rust-based GTA San Andreas mod manager focused on automating installs into the existing mod ecosystem instead of replacing it.

## Goals

- Keep Mod Loader, CLEO, ASI loaders, limit adjusters, and other essentials as the runtime layer.
- Analyze `.zip`, `.7z`, `.rar`, `.wrap`, and extracted folders before installing.
- Detect readmes, install roots, language choices, settings presets, optionals, and compatibility variants.
- Build explicit install plans that users can review and override.
- Support profiles, load order, per-mod toggles, all-mods-off mode, and vanilla restore.
- Track local telemetry such as installed files, conflicts, chosen options, install failures, and profile launch history.

## Current Commands

```powershell
cargo run -- ui
cargo run -- game
cargo run -- init
cargo run -- profile-new vanilla-plus
cargo run -- profiles
cargo run -- scan
cargo run -- analyze "E:\Mods\GTa Sa\Improved_Streaming.zip"
cargo run -- analyze "E:\Mods\GTa Sa\Proper_Fixes.7z"
cargo run -- dry-install "E:\Mods\GTa Sa\Improved_Streaming.zip" --profile vanilla-plus --include "EN/Improved Streaming"
cargo run -- import "E:\Mods\GTa Sa\New\Gravity_Fix.7z"
```

`ui` opens the native desktop manager. It currently covers manager initialization, profile selection
and creation, imported mod library review, package analysis/import, profile membership, per-profile
enable/disable toggles, load order editing, infrastructure status, ephemeral launch, and cleanup from
the generated journal.

`install` exists, but use `dry-install` first. It extracts to `.sa-mod-manager/staging`, copies only planned roots, backs up overwritten files, and writes a transaction journal.

```powershell
cargo run -- install "E:\Mods\GTa Sa\New\Gravity_Fix.7z" --profile vanilla-plus
cargo run -- rollback ".sa-mod-manager\journals\<transaction>.journal"
```

Bootstrap operations such as Mod Loader runtime files, executable replacements, and other root-level essentials are detected but blocked from automatic install for now. They need a dedicated bootstrap flow with stronger prompts.

## Archive Portability

Native support:

- `.zip` packages are listed, read, and extracted by the manager without external tools.
- `.wrap` packages use the same native ZIP backend, then validate their manifest.
- Extracted folders never need external archive tools.

External backend:

- `.7z` and `.rar` require a 7-Zip-compatible executable.
- The manager searches for `7z`, `7zz`, `7za`, `7z.exe`, and `7za.exe` on `PATH`.
- On Windows it also checks the standard `C:\Program Files\7-Zip\7z.exe` locations.
- For a portable install, set `SA_MOD_MANAGER_7Z` to the full path of the 7-Zip executable.

If 7-Zip is missing, `.zip` and `.wrap` packages still work. Only `.7z` and `.rar`
analysis/extraction are blocked.

## Ephemeral Run Workflow

The preferred long-term model is not permanent installation. A profile is materialized into the game folder only for a run, then cleaned up afterward.

```powershell
cargo run -- import "E:\Mods\GTa Sa\New\Gravity_Fix.7z"
cargo run -- profile-json vanilla-plus
cargo run -- profile-add vanilla-plus ".sa-mod-manager\mods\gravity_fix\mod.json"
cargo run -- profile-show vanilla-plus
cargo run -- profile-disable vanilla-plus gravity_fix
cargo run -- profile-enable vanilla-plus gravity_fix
cargo run -- profile-order vanilla-plus gravity_fix 250
cargo run -- prepare-run vanilla-plus

# Launch the game here.

cargo run -- cleanup-run ".sa-mod-manager\journals\<run-transaction>.journal"
```

`import` extracts the package once into `.sa-mod-manager/library/<mod>/source` and writes a matching `.sa-mod-manager/mods/<mod>/mod.json`. `prepare-run` reads `.sa-mod-manager/profiles/<profile>.json`, sorts enabled mods by `load_order`, reads each mod JSON, then copies the configured source roots from the pre-extracted library into the configured game targets. Later mods overwrite earlier mods according to load order. Every file change is journaled so cleanup can remove new files and restore overwritten files.

Legacy configs without `source_root` still work, but they fall back to extraction during `prepare-run`. Imported configs are preferred for speed.

All important choices are editable in JSON:

- Game root and enabled mods: `.sa-mod-manager/profiles/<profile>.json`
- Per-mod package path, source roots, target paths, enabled roots, and optionals: `.sa-mod-manager/mods/<mod>/mod.json`
- Pre-extracted source directory: `source_root` in each mod JSON
- Load order: `load_order` in the profile JSON
- Manual path override: change an `install_roots[].source` or `install_roots[].target`

Profile membership can be managed from the CLI or by editing JSON:

```powershell
cargo run -- profile-show vanilla-plus
cargo run -- profile-add vanilla-plus ".sa-mod-manager\mods\gravity_fix\mod.json"
cargo run -- profile-disable vanilla-plus gravity_fix
cargo run -- profile-enable vanilla-plus gravity_fix
cargo run -- profile-order vanilla-plus gravity_fix 250
cargo run -- profile-remove vanilla-plus gravity_fix
```

`prepare-run` only materializes profile entries where `"enabled": true`, and it applies them in ascending `load_order`. Disabled profile entries remain installed in the manager library but are not injected into the game folder.

Profile safety rules:

- Enabled profile entries are validated before any files are materialized.
- Duplicate enabled mod IDs or duplicate enabled config paths fail the run before copying.
- Missing enabled mod config files fail the run before copying.
- The run journal records each applied profile entry as `profile_mod=<id>|<load_order>|<config>`,
  making overwrite order auditable after a run.
- Profile edit operations normalize entries by load order and mod ID when writing JSON.

Example mod config:

```json
{
  "version": 1,
  "id": "gravity_fix",
  "package": "E:\\Mods\\GTa Sa\\New\\Gravity_Fix.7z",
  "source_root": ".sa-mod-manager\\library\\gravity_fix\\source",
  "enabled": true,
  "default_load_order": 100,
  "install_roots": [
    {
      "source": "Gravity Fix/CLEO",
      "target": "CLEO",
      "kind": "cleo",
      "enabled": true,
      "optional": false
    }
  ]
}
```

This lets a user correct bad automatic detection without rebuilding the manager.

## Install Model

The manager should prefer these targets:

- Mod Loader content: `modloader/<load-order>_<mod-name>/...`
- CLEO scripts: `CLEO/`, with toggles through rename or managed staging.
- ASI plugins: game root when required, otherwise Mod Loader `std.asi` support where valid.
- Direct data/model/text/anim/audio changes: prefer Mod Loader mirrors.
- Executables, proxy DLLs, and downgrade/bootstrap files: explicit bootstrap action only.

Direct overwrite installs must create a journal entry and backup before writing.

## Package Analysis Model

Each package should produce:

- `readmes`: install notes and compatibility docs.
- `components`: Mod Loader, CLEO, ASI, IMG replacement, data, text, anim, audio, script data.
- `install_candidates`: source root, target strategy, files, bytes, notes.
- `option_groups`: language folders, optional folders, settings presets, recommended/bonus content.
- `context_hints`: requirements or compatibility relationships inferred from package names and paths.
- `risks`: executable replacement, proxy DLL, full IMG replacement, mission script replacement.

## `.wrap` Package Schema

`.wrap` packages are ZIP-compatible archives with a required manifest file named
`wrap.json`, `manifest.json`, or `package.wrap.json`. The manifest is authoritative:
when valid install roots are present, they are preferred over readme parsing and
package heuristics.

Minimal manifest:

```json
{
  "install_roots": [
    {
      "source": "files/CLEO",
      "target": "CLEO",
      "kind": "cleo",
      "enabled": true,
      "optional": false,
      "notes": ["declared by package author"]
    }
  ]
}
```

`install_roots` may also be written as `install`. Each install root supports:

- `source` required: relative path inside the package. `.` means the package root.
- `target` required: relative game target such as `CLEO`, `modloader/<mod>`,
  `modloader/<mod>/gta3.img`, `data`, `models`, `text`, `anim`, or `.`.
- `kind` optional, default `modloader`: one of `modloader`, `cleo`, `cleo_text`,
  `asi`, `plugin`, `bootstrap`, `runtime`, `direct`, `directmanaged`, or
  `direct_managed`.
- `enabled` optional, default `true`.
- `optional` optional, default `false`.
- `notes` optional: strings shown in analysis and generated mod config notes.

Validation rules:

- `.wrap` packages must include a manifest and at least one install root.
- `source` and `target` must be relative paths, not absolute paths or `..` escapes.
- `source` must exist in the package.
- `target` must stay inside the selected game root.
- Unsupported `kind` values fail analysis with a specific manifest error.

After import, manifest-derived roots are written to the human-editable
`.sa-mod-manager/mods/<mod>/mod.json`. The UI Mod Library can edit each root's
source, target, kind, enabled state, and optional state.

## Compatibility Model

The next layer should maintain a local rule database, then let package analysis add inferred rules:

- `requires`: install or enable another component first.
- `conflicts`: cannot be enabled in the same profile.
- `recommends`: should be offered when another mod is present.
- `variant_for`: optional component intended for a specific installed mod.
- `load_after`: load order relationship.
- `setting_for`: config preset intended for a package context such as RoSA or large texture packs.

Examples from the current mod inventory:

- Improved Streaming has RoSA/large-pack settings presets.
- Proper Fixes has a separate RoSA variant archive and several optionals.
- Storyline Improvement contains script replacements and should conflict with other mission/story script mods.
- Large texture/model packs should recommend Open Limit Adjuster and appropriate streaming settings.

## Safe Toggle Strategy

- Mod Loader mods: enable/disable through profile membership or directory suffix.
- CLEO scripts: rename `.cs`/`.cleo` or move between managed active/inactive staging directories.
- ASI plugins: rename `.asi` to `.asi.disabled` or move through managed staging.
- Direct overwrites: restore from backup journal, never assume file state.

## Local Telemetry

The UI Telemetry tab is file-based and private to the local manager state. It reads:

- `.sa-mod-manager/library/<mod>/import.json` for import history.
- `.sa-mod-manager/journals/*.journal` for run/install history, copied files,
  new files, overwritten files, blocked bootstrap actions, and missing sources.
- `.sa-mod-manager/pending-runs/*.pending` for cleanup state.

Telemetry is refreshed with the rest of the UI state. It does not send data anywhere
or require a background service.

## Next Implementation Steps

1. Add manifest serialization for packages, profiles, and install journals.
2. Add a dry-run install planner that turns candidates and selected options into copy operations.
3. Add controlled extraction to a manager staging directory.
4. Add profile creation and load-order operations.
5. Add actual install/enable/disable only after backup/journal support exists.
6. Add rollback for install journals.
7. Expand the `egui` desktop UI with richer readme viewing, option-group editing, conflict prompts, and file dialogs.

## Implemented Safety

- Active installs are generated from the same dry-run plan.
- Imported mods are extracted once into `.sa-mod-manager/library/<mod>/source`.
- Ephemeral runs use pre-extracted sources when `source_root` is present, avoiding archive decompression at launch time.
- Ephemeral runs are generated from human-editable JSON configs.
- Existing destination files are copied into `.sa-mod-manager/backups/<transaction>/` before overwrite.
- Every install writes `.sa-mod-manager/journals/<transaction>.journal`.
- Rollback removes files that were new and restores files that had backups.
- Destination paths are checked to stay under the selected game root.
- Bootstrap files are not auto-applied.
