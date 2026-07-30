use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use reviewgate_core::{
    SemanticContextExcerpt as SemanticContextExcerptReport, SemanticContextReport,
    SemanticContextStatus,
};
use tree_sitter::{Node, Parser};

use crate::{confined_repo_file, read_bounded_text};

const MAX_CHANGED_SYMBOLS: usize = 24;
const MAX_RG_CALLS: usize = 16;
const MAX_RG_OUTPUT_BYTES_PER_CALL: usize = 32 * 1024;
const MAX_RG_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_SEARCH_TIME: Duration = Duration::from_secs(3);
const MAX_RG_CALL_TIME: Duration = Duration::from_millis(800);
const MAX_SEMANTIC_FILE_BYTES: usize = 256 * 1024;
const MAX_SEMANTIC_EXCERPTS: usize = 16;
const MAX_SEMANTIC_EXCERPT_BYTES: usize = 4 * 1024;
const MAX_SEMANTIC_CONTEXT_BYTES: usize = 40 * 1024;
const EXCERPT_CONTEXT_LINES: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SemanticContext {
    pub(crate) report: SemanticContextReport,
    pub(crate) excerpts: Vec<SemanticExcerpt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SemanticExcerpt {
    pub(crate) path: String,
    pub(crate) start_line: u32,
    pub(crate) end_line: u32,
    pub(crate) reason: String,
    pub(crate) relation: String,
    pub(crate) contents: String,
}

pub(crate) fn unavailable_semantic_context(
    reviewed_sha: &str,
    reason: impl Into<String>,
) -> SemanticContext {
    SemanticContext {
        report: SemanticContextReport {
            status: SemanticContextStatus::Unavailable,
            reviewed_sha: reviewed_sha.to_string(),
            parser: "none".to_string(),
            changed_symbol_count: 0,
            candidate_count: 0,
            selected_count: 0,
            rg_calls: 0,
            rg_output_bytes: 0,
            selected_bytes: 0,
            truncated: false,
            fallback_reason: Some(reason.into()),
            excerpts: vec![],
        },
        excerpts: vec![],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AddedLine {
    line: usize,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SearchMatch {
    path: String,
    line: usize,
    text: String,
    symbol: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RankedExcerpt {
    rank: u8,
    path: String,
    line: usize,
    reason: String,
    relation: String,
    contents: String,
    start_line: usize,
    end_line: usize,
    truncated: bool,
}

pub(crate) fn collect_semantic_context(
    repo: &Path,
    reviewed_sha: &str,
    changed_files: &[String],
    diff: &str,
) -> SemanticContext {
    let added_lines = parse_added_lines(diff);
    let deleted_lines = parse_deleted_lines(diff);
    let changed_set = changed_files.iter().cloned().collect::<BTreeSet<_>>();
    let mut structured_symbols = Vec::new();
    let mut fallback_symbols = Vec::new();
    let mut used_rust_parser = false;
    let mut used_text_fallback = false;

    for relative in changed_files {
        let deleted_symbols = deleted_lines
            .get(relative)
            .map(|lines| extract_deleted_symbols(relative, lines, diff))
            .unwrap_or_default();
        if !deleted_symbols.is_empty() {
            used_text_fallback = true;
            fallback_symbols.extend(deleted_symbols);
        }

        if let (Some(lines), Some(path)) = (
            added_lines.get(relative),
            confined_repo_file(repo, relative),
        ) && let Ok(Some(mut source)) = read_bounded_text(&path, MAX_SEMANTIC_FILE_BYTES)
        {
            if source.len() > MAX_SEMANTIC_FILE_BYTES {
                source.truncate(
                    (0..=MAX_SEMANTIC_FILE_BYTES)
                        .rev()
                        .find(|index| source.is_char_boundary(*index))
                        .unwrap_or(0),
                );
            }
            if relative.ends_with(".rs") {
                let changed_lines = lines.iter().map(|line| line.line).collect::<BTreeSet<_>>();
                let extracted_rust = extract_rust_changed_symbols(&source, &changed_lines);
                if extracted_rust.is_empty() {
                    used_text_fallback = true;
                    fallback_symbols
                        .extend(extract_text_changed_symbols(relative, &source, lines, diff));
                } else {
                    used_rust_parser = true;
                    structured_symbols.extend(extracted_rust);
                }
            } else {
                used_text_fallback = true;
                fallback_symbols
                    .extend(extract_text_changed_symbols(relative, &source, lines, diff));
            }
        }
    }
    structured_symbols.sort();
    structured_symbols.dedup();
    fallback_symbols.sort();
    fallback_symbols.dedup();
    let mut symbols = structured_symbols;
    for symbol in fallback_symbols {
        if symbols.len() >= MAX_CHANGED_SYMBOLS {
            break;
        }
        if !symbols.contains(&symbol) {
            symbols.push(symbol);
        }
    }

    let parser = match (used_rust_parser, used_text_fallback) {
        (true, true) => "tree_sitter_rust+text+rg",
        (true, false) => "tree_sitter_rust+rg",
        (false, true) => "text+rg",
        (false, false) => "text+rg",
    }
    .to_string();
    let fallback_reason = if symbols.is_empty() {
        Some("no stable identifiers were found on added lines".to_string())
    } else if used_text_fallback {
        Some("text identifier fallback was used for unsupported or unparsed files".to_string())
    } else {
        None
    };

    let search_started = Instant::now();
    let mut rg_calls = 0usize;
    let mut rg_output_bytes = 0usize;
    let mut search_truncated = symbols.len() > MAX_RG_CALLS;
    let mut rg_unavailable = false;
    let mut matches = Vec::new();

    for symbol in symbols.iter().take(MAX_RG_CALLS) {
        if search_started.elapsed() >= MAX_SEARCH_TIME || rg_output_bytes >= MAX_RG_OUTPUT_BYTES {
            search_truncated = true;
            break;
        }
        rg_calls += 1;
        let remaining_bytes = MAX_RG_OUTPUT_BYTES - rg_output_bytes;
        let remaining_time = MAX_SEARCH_TIME.saturating_sub(search_started.elapsed());
        match search_symbol(
            repo,
            symbol,
            remaining_bytes.min(MAX_RG_OUTPUT_BYTES_PER_CALL),
            remaining_time.min(MAX_RG_CALL_TIME),
        ) {
            Ok((output, truncated)) => {
                rg_output_bytes = rg_output_bytes.saturating_add(output.len());
                search_truncated |= truncated;
                if !truncated {
                    matches.extend(parse_rg_matches(&output, symbol));
                }
            }
            Err(_) => {
                rg_unavailable = true;
                break;
            }
        }
    }

    let mut seen_matches = BTreeSet::new();
    matches.retain(|candidate| {
        !changed_set.contains(&candidate.path)
            && seen_matches.insert((
                candidate.path.clone(),
                candidate.line,
                candidate.symbol.clone(),
            ))
    });
    let candidate_count = matches.len();
    let mut ranked = matches
        .into_iter()
        .filter_map(|candidate| rank_excerpt(repo, candidate))
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        (
            left.rank,
            left.path.as_str(),
            left.line,
            left.relation.as_str(),
        )
            .cmp(&(
                right.rank,
                right.path.as_str(),
                right.line,
                right.relation.as_str(),
            ))
    });

    let mut selected = Vec::new();
    let mut selected_bytes = 0usize;
    let mut selected_per_path = BTreeMap::<String, usize>::new();
    let mut selected_ranges = BTreeSet::new();
    for excerpt in ranked {
        if selected.len() >= MAX_SEMANTIC_EXCERPTS || selected_bytes >= MAX_SEMANTIC_CONTEXT_BYTES {
            search_truncated = true;
            break;
        }
        if selected_per_path
            .get(&excerpt.path)
            .copied()
            .unwrap_or_default()
            >= 2
            || selected_ranges.iter().any(|(path, start, end)| {
                path == &excerpt.path && excerpt.start_line <= *end && excerpt.end_line >= *start
            })
        {
            continue;
        }
        let remaining = MAX_SEMANTIC_CONTEXT_BYTES - selected_bytes;
        if excerpt.contents.len() > remaining {
            search_truncated = true;
            continue;
        }
        selected_bytes += excerpt.contents.len();
        *selected_per_path.entry(excerpt.path.clone()).or_default() += 1;
        selected_ranges.insert((excerpt.path.clone(), excerpt.start_line, excerpt.end_line));
        selected.push(excerpt);
    }

    let status = if rg_unavailable {
        SemanticContextStatus::Unavailable
    } else if used_text_fallback || symbols.is_empty() {
        SemanticContextStatus::Fallback
    } else {
        SemanticContextStatus::Collected
    };
    let fallback_reason = if rg_unavailable {
        Some("ripgrep was unavailable or failed; review continued without semantic excerpts".into())
    } else {
        fallback_reason
    };
    let excerpts = selected
        .iter()
        .map(|excerpt| SemanticExcerpt {
            path: excerpt.path.clone(),
            start_line: excerpt.start_line.try_into().unwrap_or(u32::MAX),
            end_line: excerpt.end_line.try_into().unwrap_or(u32::MAX),
            reason: excerpt.reason.clone(),
            relation: excerpt.relation.clone(),
            contents: excerpt.contents.clone(),
        })
        .collect::<Vec<_>>();
    let excerpt_reports = selected
        .iter()
        .map(|excerpt| SemanticContextExcerptReport {
            path: excerpt.path.clone(),
            start_line: excerpt.start_line.try_into().unwrap_or(u32::MAX),
            end_line: excerpt.end_line.try_into().unwrap_or(u32::MAX),
            reason: excerpt.reason.clone(),
            relation: excerpt.relation.clone(),
            bytes: excerpt.contents.len().try_into().unwrap_or(u32::MAX),
            truncated: excerpt.truncated,
            reviewed_sha: reviewed_sha.to_string(),
        })
        .collect::<Vec<_>>();
    let report = SemanticContextReport {
        status,
        reviewed_sha: reviewed_sha.to_string(),
        parser,
        changed_symbol_count: symbols.len().try_into().unwrap_or(u32::MAX),
        candidate_count: candidate_count.try_into().unwrap_or(u32::MAX),
        selected_count: excerpt_reports.len().try_into().unwrap_or(u32::MAX),
        rg_calls: rg_calls.try_into().unwrap_or(u32::MAX),
        rg_output_bytes: rg_output_bytes.try_into().unwrap_or(u32::MAX),
        selected_bytes: selected_bytes.try_into().unwrap_or(u32::MAX),
        truncated: search_truncated,
        fallback_reason,
        excerpts: excerpt_reports,
    };
    SemanticContext { report, excerpts }
}

fn extract_deleted_symbols(path: &str, lines: &[AddedLine], diff: &str) -> Vec<String> {
    if !path.ends_with(".rs") {
        return extract_text_changed_symbols(path, "", lines, diff);
    }

    let mut symbols = Vec::new();
    for line in lines {
        let tokens = identifiers(&line.text);
        for (index, token) in tokens.iter().enumerate() {
            if matches!(
                token.as_str(),
                "fn" | "struct" | "enum" | "trait" | "type" | "const" | "static" | "mod"
            ) && let Some(name) = tokens.get(index + 1)
                && stable_identifier(name)
            {
                symbols.push(name.clone());
            }
        }
    }
    symbols.sort();
    symbols.dedup();
    symbols
}

fn extract_rust_changed_symbols(source: &str, changed_lines: &BTreeSet<usize>) -> Vec<String> {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .is_err()
    {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let mut symbols = Vec::new();
    collect_changed_rust_definitions(tree.root_node(), source, changed_lines, &mut symbols);
    symbols.sort();
    symbols.dedup();
    symbols
}

fn collect_changed_rust_definitions(
    node: Node<'_>,
    source: &str,
    changed_lines: &BTreeSet<usize>,
    symbols: &mut Vec<String>,
) {
    const DEFINITION_KINDS: &[&str] = &[
        "function_item",
        "struct_item",
        "enum_item",
        "trait_item",
        "type_item",
        "const_item",
        "static_item",
        "mod_item",
        "macro_definition",
    ];
    let start = node.start_position().row + 1;
    let end = node.end_position().row + 1;
    if DEFINITION_KINDS.contains(&node.kind())
        && changed_lines.range(start..=end).next().is_some()
        && let Some(name) = node.child_by_field_name("name")
        && let Ok(name) = name.utf8_text(source.as_bytes())
        && stable_identifier(name)
    {
        symbols.push(name.to_string());
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_changed_rust_definitions(child, source, changed_lines, symbols);
    }
}

fn extract_text_changed_symbols(
    _path: &str,
    _source: &str,
    added_lines: &[AddedLine],
    _diff: &str,
) -> Vec<String> {
    let mut candidates = BTreeMap::<String, (u8, usize)>::new();
    for added in added_lines {
        for token in identifiers(&added.text) {
            if !stable_identifier(&token) || text_stop_word(&token) {
                continue;
            }
            if !text_identifier_is_specific(&token) {
                continue;
            }
            let rank = if token.contains('_') { 0 } else { 1 };
            candidates
                .entry(token)
                .and_modify(|entry| entry.1 += 1)
                .or_insert((rank, 1));
        }
    }
    let mut candidates = candidates.into_iter().collect::<Vec<_>>();
    candidates.sort_by(
        |(left_name, (left_rank, left_count)), (right_name, (right_rank, right_count))| {
            (
                *left_rank,
                std::cmp::Reverse(*left_count),
                left_name.as_str(),
            )
                .cmp(&(
                    *right_rank,
                    std::cmp::Reverse(*right_count),
                    right_name.as_str(),
                ))
        },
    );
    candidates
        .into_iter()
        .take(MAX_CHANGED_SYMBOLS)
        .map(|(name, _)| name)
        .collect()
}

fn text_identifier_is_specific(identifier: &str) -> bool {
    let has_lowercase = identifier
        .chars()
        .any(|character| character.is_ascii_lowercase());
    let has_uppercase = identifier
        .chars()
        .any(|character| character.is_ascii_uppercase());
    identifier.contains('_') || (has_lowercase && has_uppercase)
}

fn identifiers(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in line.chars() {
        if character == '_' || character.is_alphanumeric() {
            current.push(character);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn stable_identifier(identifier: &str) -> bool {
    identifier.chars().count() >= 4
        && identifier
            .chars()
            .next()
            .is_some_and(|character| character == '_' || character.is_alphabetic())
}

fn text_stop_word(identifier: &str) -> bool {
    matches!(
        identifier.to_ascii_lowercase().as_str(),
        "true"
            | "false"
            | "null"
            | "none"
            | "then"
            | "else"
            | "with"
            | "from"
            | "this"
            | "that"
            | "return"
            | "shell"
            | "steps"
            | "name"
            | "uses"
            | "jobs"
            | "permissions"
            | "contents"
            | "reviewgate"
            | "github"
            | "pull_request"
            | "repository"
    )
}

fn parse_added_lines(diff: &str) -> BTreeMap<String, Vec<AddedLine>> {
    parse_diff_lines(diff, '+')
}

fn parse_deleted_lines(diff: &str) -> BTreeMap<String, Vec<AddedLine>> {
    parse_diff_lines(diff, '-')
}

fn parse_diff_lines(diff: &str, selected_prefix: char) -> BTreeMap<String, Vec<AddedLine>> {
    let mut result = BTreeMap::new();
    let mut path: Option<String> = None;
    let mut old_path: Option<String> = None;
    let mut new_line = 0usize;
    let mut old_line = 0usize;
    let mut in_hunk = false;

    for line in diff.lines() {
        if let Some(relative) = line.strip_prefix("--- a/") {
            old_path = Some(relative.to_string());
            continue;
        }
        if line == "--- /dev/null" {
            old_path = None;
            continue;
        }
        if let Some(relative) = line.strip_prefix("+++ b/") {
            path = Some(relative.to_string());
            in_hunk = false;
            continue;
        }
        if line == "+++ /dev/null" {
            path = None;
            in_hunk = false;
            continue;
        }
        if line.starts_with("@@ ") {
            let Some(new_range) = line.split_whitespace().nth(2) else {
                in_hunk = false;
                continue;
            };
            let Some(old_range) = line.split_whitespace().nth(1) else {
                in_hunk = false;
                continue;
            };
            new_line = new_range
                .trim_start_matches('+')
                .split(',')
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            old_line = old_range
                .trim_start_matches('-')
                .split(',')
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            in_hunk = new_line > 0 || old_line > 0;
            continue;
        }
        if !in_hunk {
            continue;
        }
        if line.starts_with('+') && !line.starts_with("+++") {
            if selected_prefix == '+'
                && let Some(path) = path.as_ref()
            {
                result
                    .entry(path.clone())
                    .or_insert_with(Vec::new)
                    .push(AddedLine {
                        line: new_line,
                        text: line[1..].to_string(),
                    });
            }
            new_line += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            if selected_prefix == '-'
                && let Some(path) = old_path.as_ref()
            {
                result
                    .entry(path.clone())
                    .or_insert_with(Vec::new)
                    .push(AddedLine {
                        line: old_line,
                        text: line[1..].to_string(),
                    });
            }
            old_line += 1;
        } else if !line.starts_with('\\') {
            new_line += 1;
            old_line += 1;
        }
    }
    result
}

fn search_symbol(
    repo: &Path,
    symbol: &str,
    max_output_bytes: usize,
    timeout: Duration,
) -> Result<(Vec<u8>, bool), String> {
    let mut child = Command::new("rg")
        .args([
            "--json",
            "--fixed-strings",
            "--line-number",
            "--color",
            "never",
            "--hidden",
            "--glob",
            "!.git/**",
            "--glob",
            "!target/**",
            "--glob",
            "!node_modules/**",
            "--max-filesize",
            "256K",
            "--",
            symbol,
            ".",
        ])
        .current_dir(repo)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| error.to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or("ripgrep stdout was unavailable")?;
    let reader = thread::spawn(move || {
        let mut output = Vec::new();
        stdout
            .take((max_output_bytes as u64).saturating_add(1))
            .read_to_end(&mut output)
            .map(|_| output)
    });
    let started = Instant::now();
    let mut timed_out = false;
    loop {
        if child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            break;
        }
        if started.elapsed() >= timeout || reader.is_finished() {
            timed_out = started.elapsed() >= timeout;
            child.kill().map_err(|error| error.to_string())?;
            child.wait().map_err(|error| error.to_string())?;
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let mut output = reader
        .join()
        .map_err(|_| "ripgrep reader failed".to_string())?
        .map_err(|error| error.to_string())?;
    let output_truncated = output.len() > max_output_bytes;
    output.truncate(max_output_bytes);
    Ok((output, timed_out || output_truncated))
}

fn parse_rg_matches(output: &[u8], symbol: &str) -> Vec<SearchMatch> {
    String::from_utf8_lossy(output)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event.get("type").and_then(serde_json::Value::as_str) == Some("match"))
        .filter_map(|event| {
            let data = event.get("data")?;
            let path = data.pointer("/path/text")?.as_str()?.strip_prefix("./")?;
            let line = data.get("line_number")?.as_u64()?.try_into().ok()?;
            let text = data
                .pointer("/lines/text")?
                .as_str()?
                .trim_end()
                .to_string();
            Some(SearchMatch {
                path: path.to_string(),
                line,
                text,
                symbol: symbol.to_string(),
            })
        })
        .collect()
}

fn rank_excerpt(repo: &Path, candidate: SearchMatch) -> Option<RankedExcerpt> {
    let path = confined_repo_file(repo, &candidate.path)?;
    let mut source = read_bounded_text(&path, MAX_SEMANTIC_FILE_BYTES).ok()??;
    let file_truncated = source.len() > MAX_SEMANTIC_FILE_BYTES;
    if file_truncated {
        source.truncate(
            (0..=MAX_SEMANTIC_FILE_BYTES)
                .rev()
                .find(|index| source.is_char_boundary(*index))
                .unwrap_or(0),
        );
    }
    let lower_path = candidate.path.to_ascii_lowercase();
    let (rank, reason) = if is_test_path(&lower_path) {
        (0, "test_reference")
    } else if looks_like_definition(&candidate.text, &candidate.symbol) {
        (1, "definition")
    } else if is_configuration_path(&lower_path) {
        (2, "configuration_reference")
    } else {
        (3, "reference")
    };
    let lines = source.lines().collect::<Vec<_>>();
    if candidate.line == 0 || candidate.line > lines.len() {
        return None;
    }
    let start = candidate.line.saturating_sub(EXCERPT_CONTEXT_LINES).max(1);
    let end = (candidate.line + EXCERPT_CONTEXT_LINES).min(lines.len());
    let mut contents = lines[start - 1..end].join("\n");
    contents.push('\n');
    let excerpt_truncated = contents.len() > MAX_SEMANTIC_EXCERPT_BYTES;
    if excerpt_truncated {
        contents.truncate(
            (0..=MAX_SEMANTIC_EXCERPT_BYTES)
                .rev()
                .find(|index| contents.is_char_boundary(*index))
                .unwrap_or(0),
        );
    }
    Some(RankedExcerpt {
        rank,
        path: candidate.path,
        line: candidate.line,
        reason: reason.to_string(),
        relation: format!("symbol:{}", candidate.symbol),
        contents,
        start_line: start,
        end_line: end,
        truncated: file_truncated || excerpt_truncated,
    })
}

fn is_test_path(path: &str) -> bool {
    path.contains("/tests/")
        || path.starts_with("tests/")
        || path.contains("_test.")
        || path.contains(".test.")
        || path.contains(".spec.")
}

fn is_configuration_path(path: &str) -> bool {
    path.starts_with(".github/")
        || [
            ".yml", ".yaml", ".toml", ".json", ".md", ".ini", ".cfg", ".lock",
        ]
        .iter()
        .any(|extension| path.ends_with(extension))
}

fn looks_like_definition(line: &str, symbol: &str) -> bool {
    [
        "fn ",
        "struct ",
        "enum ",
        "trait ",
        "type ",
        "const ",
        "static ",
        "def ",
        "class ",
        "function ",
    ]
    .iter()
    .any(|prefix| line.contains(&format!("{prefix}{symbol}")))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;

    use super::{
        SemanticContextStatus, collect_semantic_context, extract_deleted_symbols,
        extract_rust_changed_symbols, extract_text_changed_symbols, parse_added_lines,
        parse_deleted_lines,
    };

    fn unique_test_dir(label: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "reviewgate-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&path).expect("create temp repo");
        path
    }

    #[test]
    fn rust_parser_selects_only_definitions_touched_by_added_lines() {
        let source = r#"
pub fn unchanged() -> bool {
    true
}

pub fn changed(value: bool) -> bool {
    !value
}
"#;
        let symbols = extract_rust_changed_symbols(source, &BTreeSet::from([7]));

        assert_eq!(symbols, vec!["changed"]);
    }

    #[test]
    fn text_fallback_extracts_stable_identifiers_from_added_lines() {
        let diff = r#"diff --git a/action.yml b/action.yml
--- a/action.yml
+++ b/action.yml
@@ -1,2 +1,4 @@
 queue_ms=""
+created_ms="$(date -d "$created_at" +%s%3N)"
+run_started_ms="$(date -d "$run_started_at" +%s%3N)"
 "#;
        let added = parse_added_lines(diff);
        let symbols = extract_text_changed_symbols("action.yml", "", &added["action.yml"], diff);

        assert!(symbols.contains(&"created_at".to_string()));
        assert!(symbols.contains(&"run_started_at".to_string()));
    }

    #[test]
    fn deleted_identifiers_keep_renamed_symbol_references_discoverable() {
        let diff = r#"diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1 +1 @@
-pub fn old_handler() {}
+pub fn new_handler() {}
"#;
        let deleted = parse_deleted_lines(diff);
        let symbols = extract_deleted_symbols("src/lib.rs", &deleted["src/lib.rs"], diff);

        assert!(symbols.contains(&"old_handler".to_string()));
    }

    #[test]
    fn collector_searches_references_to_a_deleted_rust_symbol() {
        let repo = unique_test_dir("semantic-context-deleted");
        fs::create_dir_all(repo.join("src")).expect("create src");
        fs::write(
            repo.join("src/caller.rs"),
            "pub fn caller() {\n    old_handler();\n}\n",
        )
        .expect("write caller");
        let diff = r#"diff --git a/src/removed.rs b/src/removed.rs
--- a/src/removed.rs
+++ /dev/null
@@ -1 +0,0 @@
-pub fn old_handler() {}
"#;

        let context =
            collect_semantic_context(&repo, "head", &["src/removed.rs".to_string()], diff);

        assert!(
            context
                .excerpts
                .iter()
                .any(|excerpt| excerpt.path == "src/caller.rs"
                    && excerpt.relation == "symbol:old_handler")
        );
        fs::remove_dir_all(repo).ok();
    }

    #[test]
    fn collector_finds_bounded_ranked_references_without_persisting_source() {
        let repo = unique_test_dir("semantic-context");
        fs::create_dir_all(repo.join("src")).expect("create src");
        fs::create_dir_all(repo.join("tests")).expect("create tests");
        fs::write(
            repo.join("src/lib.rs"),
            "pub fn changed(value: bool) -> bool {\n    !value\n}\n",
        )
        .expect("write changed source");
        fs::write(
            repo.join("tests/changed_test.rs"),
            "#[test]\nfn changed_is_inverted() {\n    assert!(!crate::changed(true));\n}\n",
        )
        .expect("write test");
        fs::write(
            repo.join("src/caller.rs"),
            "pub fn caller() -> bool {\n    crate::changed(false)\n}\n",
        )
        .expect("write caller");
        let diff = r#"diff --git a/src/lib.rs b/src/lib.rs
--- /dev/null
+++ b/src/lib.rs
@@ -0,0 +1,3 @@
+pub fn changed(value: bool) -> bool {
+    !value
+}
"#;

        let context =
            collect_semantic_context(&repo, "0123456789abcdef", &["src/lib.rs".to_string()], diff);

        assert_eq!(context.report.status, SemanticContextStatus::Collected);
        assert!(context.report.rg_calls > 0);
        assert!(
            context
                .excerpts
                .iter()
                .any(|excerpt| excerpt.path == "tests/changed_test.rs")
        );
        assert!(
            context
                .excerpts
                .iter()
                .any(|excerpt| excerpt.path == "src/caller.rs")
        );
        assert!(
            context
                .report
                .excerpts
                .iter()
                .all(|excerpt| excerpt.reviewed_sha == "0123456789abcdef")
        );
        fs::remove_dir_all(repo).ok();
    }

    #[cfg(unix)]
    #[test]
    fn collector_rejects_symlinked_search_results() {
        use std::os::unix::fs::symlink;

        let repo = unique_test_dir("semantic-context-symlink");
        let outside = unique_test_dir("semantic-context-outside");
        fs::create_dir_all(repo.join("src")).expect("create src");
        fs::write(
            repo.join("src/lib.rs"),
            "pub fn changed() -> bool {\n    true\n}\n",
        )
        .expect("write source");
        fs::write(
            outside.join("leak.rs"),
            "pub fn leak() { let _ = changed(); }\n",
        )
        .expect("write outside");
        symlink(outside.join("leak.rs"), repo.join("src/leak.rs")).expect("create symlink");
        let diff = r#"diff --git a/src/lib.rs b/src/lib.rs
--- /dev/null
+++ b/src/lib.rs
@@ -0,0 +1,3 @@
+pub fn changed() -> bool {
+    true
+}
"#;

        let context = collect_semantic_context(&repo, "head", &["src/lib.rs".to_string()], diff);

        assert!(
            context
                .excerpts
                .iter()
                .all(|excerpt| excerpt.path != "src/leak.rs")
        );
        fs::remove_dir_all(repo).ok();
        fs::remove_dir_all(outside).ok();
    }
}
