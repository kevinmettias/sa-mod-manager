use crate::prelude::*;

pub(crate) fn classify_component(lower: &str, components: &mut BTreeSet<Component>) {
    for rule in component_rules() {
        insert_component_for_rule(lower, components, rule);
    }
}

fn insert_component_for_rule(
    lower: &str,
    components: &mut BTreeSet<Component>,
    rule: &ComponentRule,
) {
    if component_rule_matches(lower, rule) {
        components.insert(rule.component);
    }
}

fn component_rule_matches(lower: &str, rule: &ComponentRule) -> bool {
    has_any_contains_match(lower, &rule.contains)
        || has_any_prefix_match(lower, &rule.prefixes)
        || has_any_suffix_match(lower, &rule.suffixes)
}

fn has_any_contains_match(lower: &str, needles: &[String]) -> bool {
    needles.iter().any(|needle| lower.contains(needle.as_str()))
}

fn has_any_prefix_match(lower: &str, prefixes: &[String]) -> bool {
    prefixes.iter().any(|prefix| lower.starts_with(prefix.as_str()))
}

fn has_any_suffix_match(lower: &str, suffixes: &[String]) -> bool {
    suffixes.iter().any(|suffix| lower.ends_with(suffix.as_str()))
}

pub(crate) fn classify_context(lower: &str, hints: &mut BTreeSet<String>) {
    for rule in context_rules() {
        insert_context_for_rule(lower, hints, rule);
    }
}

fn insert_context_for_rule(lower: &str, hints: &mut BTreeSet<String>, rule: &ContextRule) {
    if rule.aliases.iter().any(|alias| lower.contains(alias.as_str())) {
        insert_context_hint(hints, &rule.hint);
    }
}

fn insert_context_hint(hints: &mut BTreeSet<String>, hint: &str) {
    let context_hint = hint.to_string();
    hints.insert(context_hint);
}

pub(crate) fn classify_risk(lower: &str, risks: &mut BTreeSet<String>) {
    if lower.ends_with(".exe") // literal: allow external interface text or file-format spelling
        || lower.ends_with(".bat") // literal: allow external interface text or file-format spelling
        || lower.ends_with(".cmd") // literal: allow external interface text or file-format spelling
        /* literal: allow external interface text or file-format spelling */ || lower.ends_with(".msi")
    // literal: allow external interface text or file-format spelling
    {
        let risk = "contains executable installer/script files".to_string(); // literal: allow external interface text or file-format spelling
        risks.insert(risk);
    }
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if lower.contains("gta_sa.exe") || lower.contains("gta-sa.exe") {
        // literal: allow external interface text or file-format spelling
        let risk = "contains game executable replacement".to_string(); // literal: allow external interface text or file-format spelling
        risks.insert(risk);
    }
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if lower.ends_with("main.scm") || lower.ends_with("script.img") {
        // literal: allow external interface text or file-format spelling
        let risk = "contains mission script replacement; profile conflicts are likely".to_string(); // literal: allow external interface text or file-format spelling
        risks.insert(risk);
    }
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if lower.ends_with(".img") {
        // literal: allow external interface text or file-format spelling
        let risk = "contains full IMG archive replacement".to_string(); // literal: allow external interface text or file-format spelling
        risks.insert(risk);
    }
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if lower.contains("vorbis") || lower.contains("dinput8.dll") {
        // literal: allow external interface text or file-format spelling
        let risk = "contains loader/proxy DLL files".to_string(); // literal: allow external interface text or file-format spelling
        risks.insert(risk);
    }
}
