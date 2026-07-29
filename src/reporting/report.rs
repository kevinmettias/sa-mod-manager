use crate::prelude::*;

const MAX_README_DOCUMENTS_PRINTED: usize = 6;
const MAX_README_EXCERPT_LINES: usize = 6;
const MAX_README_INSTRUCTIONS_PRINTED: usize = 20;
const MAX_STRING_VALUES_PRINTED: usize = 40;

pub(crate) fn print_report(report: &PackageReport)
{
    print_report_header(report);
    print_report_sections(report);
    print_report_candidates(report);
}

fn print_report_header(report: &PackageReport)
{
    println!("package : {}", report.package.display());
    println!("kind    : {}", report.kind);
    println!("game    : {}", report.game_root.display());
    println!("entries : {}", report.entries.len());
    println!();
}

fn print_report_sections(report: &PackageReport)
{
    let component_paths = report.component_paths.iter();
    print_set("component_paths", component_paths);
    print_vec("readmes", &report.readmes);
    print_readme_documents(&report.readme_documents);
    print_readme_instructions(&report.readme_instructions);
    print_manifest_roots(&report.manifest_roots);
    print_vec("option groups", &report.option_groups);
    let context_hints = report.context_hints.iter();
    print_set("context hints", context_hints);
    let risks = report.risks.iter();
    print_set("risks", risks);
}

fn print_set<'a, Value, Values>(label: &str, values: Values)
where
    Value: fmt::Display + 'a,
    Values: Iterator<Item = &'a Value>,
{
    let values: Vec<String> = values.map(ToString::to_string).collect();
    println!("{label}:");
    if values.is_empty()
    {
        println!("  none");
    }
    else
    {
        for value in values
        {
            println!("  {value}");
        }
    }
    println!();
}

fn print_vec(label: &str, values: &[String])
{
    println!("{label}:");
    if values.is_empty()
    {
        println!("  none");
    }
    else
    {
        for value in values.iter().take(MAX_STRING_VALUES_PRINTED)
        {
            // literal: allow external format or runtime boundary value means itself here
            println!("  {value}");
        }
        if values.len() > MAX_STRING_VALUES_PRINTED
        {
            // literal: allow external format or runtime boundary value means itself here
            println!("  ... {} more", values.len() - MAX_STRING_VALUES_PRINTED); // literal: allow external format or runtime boundary value means itself here
        }
    }
    println!();
}

fn print_readme_documents(documents: &[ReadmeDocument])
{
    println!("readme excerpts:");
    if documents.is_empty()
    {
        println!("  none");
        println!();
        return;
    }
    for document in documents.iter().take(MAX_README_DOCUMENTS_PRINTED)
    {
        // literal: allow external format or runtime boundary value means itself here
        print_readme_document_excerpt(document);
    }
    if documents.len() > MAX_README_DOCUMENTS_PRINTED
    {
        // literal: allow external format or runtime boundary value means itself here
        println!(
            "  ... {} more",
            documents.len() - MAX_README_DOCUMENTS_PRINTED
        ); // literal: allow external format or runtime boundary value means itself here
    }
    println!();
}

fn print_readme_document_excerpt(document: &ReadmeDocument)
{
    println!("  {}", document.path);
    for line in document.text.lines().take(MAX_README_EXCERPT_LINES)
    {
        // literal: allow external format or runtime boundary value means itself here
        let trimmed = line.trim();
        if !trimmed.is_empty()
        {
            println!("    {trimmed}");
        }
    }
}

fn print_readme_instructions(instructions: &[ReadmeInstruction])
{
    println!("readme instructions:");
    if instructions.is_empty()
    {
        println!("  none");
    }
    else
    {
        for instruction in instructions.iter().take(MAX_README_INSTRUCTIONS_PRINTED)
        {
            // literal: allow external format or runtime boundary value means itself here
            println!(
                "  {} {:.0}% {}:{}",
                instruction.action,
                instruction.confidence * 100.0, // literal: allow external format or runtime boundary value means itself here
                instruction.source_readme,
                instruction.line_number
            );
            if let Some(source) = &instruction.source
            {
                println!("    source: {source}");
            }
            if let Some(target) = &instruction.target
            {
                println!("    target: {target}");
            }
            println!("    evidence: {}", instruction.text);
            println!("    normalized: {}", instruction.normalized_text);
            if !instruction.confidence_reasons.is_empty()
            {
                println!("    reasons: {}", instruction.confidence_reasons.join("; "));
            }
        }
        if instructions.len() > MAX_README_INSTRUCTIONS_PRINTED
        {
            // literal: allow external format or runtime boundary value means itself here
            println!(
                "  ... {} more",
                instructions.len() - MAX_README_INSTRUCTIONS_PRINTED
            ); // literal: allow external format or runtime boundary value means itself here
        }
    }
    println!();
}

fn print_manifest_roots(roots: &[ManifestInstallRoot])
{
    println!("manifest install roots:");
    if roots.is_empty()
    {
        println!("  none");
    }
    else
    {
        for root in roots
        {
            println!(
                "  {} -> {} ({}, optional: {}, enabled: {})",
                root.source, root.target, root.kind, root.optional, root.enabled
            );
            for note in &root.notes
            {
                println!("    note: {note}");
            }
        }
    }
    println!();
}

fn print_report_candidates(report: &PackageReport)
{
    println!("install candidates:");
    if report.install_candidates.is_empty()
    {
        println!("  none detected");
    }
    for candidate in &report.install_candidates
    {
        print_report_candidate(candidate);
    }
}

fn print_report_candidate(candidate: &InstallCandidate)
{
    println!(
        "  {} -> {}  ({} files, {})",
        candidate.source_root,
        candidate.target_strategy,
        candidate.file_count,
        human_bytes(candidate.total_bytes)
    );
    if !candidate.component_paths.is_empty()
    {
        let items = candidate
            .component_paths
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        println!("    component_paths: {items}");
    }
    for note in &candidate.notes
    {
        println!("    note: {note}");
    }
}

pub(crate) fn extract_to_staging(package: &Path, game_root: &Path) -> Result<(), AppError>
{
    ensure_state(game_root)?;
    if package.is_dir()
    {
        return Err(usage_error(
            "extract-stage expects an archive, not a folder",
        ));
    }

    let id = package_id(package);
    let target = state_directory(game_root).join("staging").join(&id);
    extract_archive_to_directory(package, &target)?;

    println!("staged: {}", target.display());
    return Ok(());
}
