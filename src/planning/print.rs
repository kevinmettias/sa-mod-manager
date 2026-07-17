use crate::prelude::*;

pub(crate) fn print_install_plan(plan: &InstallPlan) {
    print_install_plan_header(plan);
    print_plan_operations(plan);
    print_plan_skipped_options(plan);
    print_plan_warnings(plan);
}

fn print_install_plan_header(plan: &InstallPlan) {
    println!("package : {}", plan.package.display());
    println!("id      : {}", plan.package_id);
    println!("profile : {}", plan.profile);
    println!("game    : {}", plan.game_root.display());
    println!();
}

fn print_plan_operations(plan: &InstallPlan) {
    println!("operations:");
    if plan.operations.is_empty() {
        println!("  none");
    }
    for (idx, operation) in plan.operations.iter().enumerate() {
        print_plan_operation(idx, operation);
    }
    println!();
}

fn print_plan_operation(idx: usize, operation: &InstallOperation) {
    println!(
        "  {}. {} -> {}",
        idx + 1,
        operation.source_root,
        operation.target_root.display()
    );
    println!(
        "     kind: {}, files: {}, bytes: {}",
        operation.target_kind,
        operation.file_count,
        human_bytes(operation.total_bytes)
    );
    if operation.optional {
        println!("     selected option: yes");
    }
    for note in &operation.notes {
        println!("     note: {note}");
    }
}

fn print_plan_skipped_options(plan: &InstallPlan) {
    println!("skipped selectable options:");
    if plan.skipped_options.is_empty() {
        println!("  none");
    } else {
        for option in plan.skipped_options.iter().take(60) {
            println!("  {option}");
        }
        if plan.skipped_options.len() > 60 {
            println!("  ... {} more", plan.skipped_options.len() - 60);
        }
    }
    println!();
}

fn print_plan_warnings(plan: &InstallPlan) {
    println!("warnings/context:");
    if plan.warnings.is_empty() {
        println!("  none");
    } else {
        for warning in &plan.warnings {
            println!("  {warning}");
        }
    }
}
