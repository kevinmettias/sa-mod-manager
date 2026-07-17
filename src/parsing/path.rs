use crate::prelude::*;

pub(crate) fn path_from_package_root(source_root: &str) -> Result<PathBuf, AppError> {
    let normalized = normalize_path(source_root);
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if normalized == "." {
        // literal: allow external interface text or file-format spelling
        return Ok(PathBuf::new());
    }
    let mut out = PathBuf::new();
    for part in normalized.split('/') {
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        if part.is_empty() || part == "." {
            // literal: allow external interface text or file-format spelling
            continue;
        }
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        if part == ".." || part.contains(':') {
            // literal: allow external interface text or file-format spelling
            return Err(AppError::Usage(format!(
                "unsafe source root: {source_root}"
            )));
        }
        out.push(part);
    }
    Ok(out)
}

pub(crate) fn ensure_destination_allowed(game_root: &Path, dest: &Path) -> Result<(), AppError> {
    let game_root = game_root.canonicalize()?;
    let parent = dest
        .parent()
        .ok_or_else(|| AppError::Usage(format!("destination has no parent: {}", dest.display())))?;
    let existing_parent = nearest_existing_parent(parent)?;
    let canonical_parent = existing_parent.canonicalize()?;
    if !canonical_parent.starts_with(&game_root) {
        return Err(AppError::Usage(format!(
            "destination escapes game root: {}",
            dest.display()
        )));
    }
    Ok(())
}

fn nearest_existing_parent(path: &Path) -> Result<PathBuf, AppError> {
    let mut current = path.to_path_buf();
    while !current.exists() {
        if !current.pop() {
            return Err(AppError::Usage(format!(
                "no existing parent for {}",
                path.display()
            )));
        }
    }
    Ok(current)
}

pub(crate) fn backup_relative_for_destination(
    game_root: &Path,
    destination: &Path,
) -> Result<PathBuf, AppError> {
    let relative_path = destination.strip_prefix(game_root).map_err(|err| {
        AppError::Usage(format!(
            "destination is not below game root: {} ({err})",
            destination.display()
        ))
    })?;
    Ok(relative_path.to_path_buf())
}
