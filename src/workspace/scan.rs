use crate::prelude::*;

pub(crate) fn scan_roots(roots: &[PathBuf]) -> Result<(), AppError>
{
    let mut packages = Vec::new();
    collect_packages_from_roots(roots, &mut packages)?;

    packages.sort_by(|left, right| left.path.cmp(&right.path));
    let summary = summarize_packages(&packages);
    return print_package_summary(&summary, packages);
}

fn collect_packages_from_roots(
    roots: &[PathBuf],
    packages: &mut Vec<PackageRef>,
) -> Result<(), AppError>
{
    for root in roots
    {
        if !root.exists()
        {
            println!("missing: {}", root.display());
            continue;
        }
        collect_packages(root, packages)?;
    }
    return Ok(());
}

fn collect_packages(root: &Path, packages: &mut Vec<PackageRef>) -> Result<(), AppError>
{
    for entry in fs::read_dir(root)?
    {
        collect_package_entry(entry?, packages)?;
    }
    return Ok(());
}

fn collect_package_entry(
    entry: fs::DirEntry,
    packages: &mut Vec<PackageRef>,
) -> Result<(), AppError>
{
    let path = entry.path();
    let metadata = entry.metadata()?;
    if metadata.is_dir()
    {
        packages.push(PackageRef {
            path,
            kind: PackageKind::Folder,
            size: 0,
        });
        return Ok(());
    }

    let Some(kind) = PackageKind::from_path(&path) else {
        return Ok(());
    };

    packages.push(PackageRef {
        path,
        kind,
        size: metadata.len(),
    });
    return Ok(());
}

fn summarize_packages(packages: &[PackageRef]) -> PackageSummary
{
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_bytes = 0_u64;
    for package in packages
    {
        let package_kind = package.kind.to_string();
        *by_kind.entry(package_kind).or_default() += 1;
        total_bytes = total_bytes.saturating_add(package.size);
    }
    return PackageSummary {
        count: packages.len(),
        total_bytes,
        by_kind,
    };
}

fn print_package_summary(
    summary: &PackageSummary,
    packages: Vec<PackageRef>,
) -> Result<(), AppError>
{
    println!("packages: {}", summary.count);
    println!("bytes   : {}", human_bytes(summary.total_bytes));
    for (kind, count) in &summary.by_kind
    {
        println!("{kind:8} {count}");
    }
    println!();

    for package in packages
    {
        println!(
            "{:8} {:>10}  {}",
            package.kind,
            human_bytes(package.size),
            package.path.display()
        );
    }

    return Ok(());
}
