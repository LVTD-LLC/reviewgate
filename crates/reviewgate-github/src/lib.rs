use std::collections::{BTreeMap, BTreeSet};

use reviewgate_core::{
    DEFAULT_TARGET_SCORE, Finding, FindingEvidence, SUMMARY_MARKER, SecretString, Severity,
    extract_summary_state,
};

pub const GITHUB_TOKEN_ENV: &str = "GITHUB_TOKEN";
pub const INLINE_COMMENT_MARKER_PREFIX: &str = "<!-- reviewgate-finding:";
pub const FINDING_COMMENT_MARKER_PREFIX: &str = "<!-- reviewgate-finding-comment:";
pub const REREVIEW_STATUS_MARKER_PREFIX: &str = "<!-- reviewgate-rereview:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RereviewTarget {
    pub repository: String,
    pub pull_request_number: u64,
    pub head_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowRunCandidate {
    pub id: u64,
    pub url: String,
    pub repository: String,
    pub event: String,
    pub status: String,
    pub head_sha: String,
    pub pull_request_numbers: Vec<u64>,
    pub created_at: String,
}

pub fn select_rereview_workflow_run<'a>(
    runs: &'a [WorkflowRunCandidate],
    target: &RereviewTarget,
) -> Option<&'a WorkflowRunCandidate> {
    runs.iter()
        .filter(|run| {
            run.repository == target.repository
                && run.event == "pull_request"
                && run.status == "completed"
                && run.head_sha == target.head_sha
                && run
                    .pull_request_numbers
                    .contains(&target.pull_request_number)
        })
        .max_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        })
}

pub fn rereview_status_marker(comment_id: u64) -> String {
    format!("{REREVIEW_STATUS_MARKER_PREFIX}{comment_id} -->")
}

pub fn find_rereview_status_comment(
    comments: &[ExistingSummaryComment],
    comment_id: u64,
) -> Option<&ExistingSummaryComment> {
    let marker = rereview_status_marker(comment_id);
    comments.iter().find(|comment| {
        is_github_actions_author(comment.author_login.as_deref()) && comment.body.contains(&marker)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingSummaryComment {
    pub id: u64,
    pub author_login: Option<String>,
    pub body: String,
}

fn is_github_actions_author(author_login: Option<&str>) -> bool {
    matches!(author_login, Some("github-actions[bot]" | "github-actions"))
}

fn is_reviewgate_summary_comment(comment: &ExistingSummaryComment) -> bool {
    is_github_actions_author(comment.author_login.as_deref())
        && comment.body.contains(SUMMARY_MARKER)
}

pub fn find_summary_comment(
    comments: &[ExistingSummaryComment],
) -> Option<&ExistingSummaryComment> {
    select_primary_summary_comment(comments)
}

fn select_primary_summary_comment(
    comments: &[ExistingSummaryComment],
) -> Option<&ExistingSummaryComment> {
    let reviewgate_comments: Vec<&ExistingSummaryComment> = comments
        .iter()
        .filter(|comment| is_reviewgate_summary_comment(comment))
        .collect();

    reviewgate_comments
        .iter()
        .filter_map(|comment| {
            let state = extract_summary_state(&comment.body).ok().flatten()?;
            Some((*comment, state.run_count, state.reviewed_shas.len() as u32))
        })
        .max_by_key(|(_, run_count, reviewed_count)| (*run_count, *reviewed_count))
        .map(|(comment, _, _)| comment)
        .or_else(|| reviewgate_comments.last().copied())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryCommentAction {
    Create { body: String },
    Update { id: u64, body: String },
    Noop { id: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryCommentPublishPlan {
    pub action: SummaryCommentAction,
    pub duplicate_comment_ids: Vec<u64>,
}

impl SummaryCommentPublishPlan {
    pub fn primary_id(&self) -> Option<u64> {
        match &self.action {
            SummaryCommentAction::Create { .. } => None,
            SummaryCommentAction::Update { id, .. } | SummaryCommentAction::Noop { id } => {
                Some(*id)
            }
        }
    }
}

pub fn plan_summary_comment_publish(
    comments: &[ExistingSummaryComment],
    rendered_summary: impl Into<String>,
) -> SummaryCommentPublishPlan {
    let body = rendered_summary.into();
    let existing = select_primary_summary_comment(comments);
    let duplicate_comment_ids = comments
        .iter()
        .filter(|comment| is_reviewgate_summary_comment(comment))
        .filter(|comment| Some(comment.id) != existing.map(|existing| existing.id))
        .map(|comment| comment.id)
        .collect();

    let action = if let Some(existing) = existing {
        if existing.body == body {
            SummaryCommentAction::Noop { id: existing.id }
        } else {
            SummaryCommentAction::Update {
                id: existing.id,
                body,
            }
        }
    } else {
        SummaryCommentAction::Create { body }
    };

    SummaryCommentPublishPlan {
        action,
        duplicate_comment_ids,
    }
}

pub fn plan_summary_comment_upsert(
    comments: &[ExistingSummaryComment],
    rendered_summary: impl Into<String>,
) -> SummaryCommentAction {
    plan_summary_comment_publish(comments, rendered_summary).action
}

#[derive(Clone, PartialEq, Eq)]
pub struct GitHubAuth {
    token: SecretString,
}

impl GitHubAuth {
    pub fn from_token(token: impl Into<String>) -> Self {
        Self {
            token: SecretString::new(token),
        }
    }

    pub fn authorization_header(&self) -> String {
        format!("Bearer {}", self.token.expose())
    }
}

impl std::fmt::Debug for GitHubAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("GitHubAuth { token: [redacted] }")
    }
}

pub trait SummaryCommentClient {
    type Error;

    fn create_summary_comment(&mut self, body: &str) -> Result<u64, Self::Error>;

    fn update_summary_comment(&mut self, id: u64, body: &str) -> Result<(), Self::Error>;
}

pub fn upsert_summary_comment<C: SummaryCommentClient>(
    client: &mut C,
    comments: &[ExistingSummaryComment],
    rendered_summary: impl Into<String>,
) -> Result<u64, C::Error> {
    match plan_summary_comment_upsert(comments, rendered_summary) {
        SummaryCommentAction::Create { body } => client.create_summary_comment(&body),
        SummaryCommentAction::Update { id, body } => {
            client.update_summary_comment(id, &body)?;
            Ok(id)
        }
        SummaryCommentAction::Noop { id } => Ok(id),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingInlineComment {
    pub id: u64,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineCommentDraft {
    pub finding_id: String,
    pub path: String,
    pub line: u32,
    pub body: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangedLineSet {
    lines_by_path: BTreeMap<String, BTreeMap<u32, String>>,
    fallback_lines_by_path: BTreeMap<String, BTreeMap<u32, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineCommentAnchorPlan {
    pub drafts: Vec<InlineCommentDraft>,
    pub repaired_count: u32,
    pub fallback_count: u32,
    pub skipped_count: u32,
    pub skipped_finding_ids: Vec<String>,
}

impl ChangedLineSet {
    pub fn from_unified_diff(diff: &str) -> Self {
        let mut lines_by_path: BTreeMap<String, BTreeMap<u32, String>> = BTreeMap::new();
        let mut fallback_lines_by_path: BTreeMap<String, BTreeMap<u32, String>> = BTreeMap::new();
        let mut current_path: Option<String> = None;
        let mut new_line: Option<u32> = None;

        for line in diff.lines() {
            if let Some(path) = line.strip_prefix("+++ ") {
                current_path = parse_diff_new_path(path);
                new_line = None;
                continue;
            }

            if line.starts_with("@@") {
                new_line = parse_new_hunk_start(line);
                continue;
            }

            let Some(path) = current_path.as_ref() else {
                continue;
            };
            let Some(line_number) = new_line else {
                continue;
            };

            if let Some(added_line) = line.strip_prefix('+') {
                lines_by_path
                    .entry(path.clone())
                    .or_default()
                    .insert(line_number, added_line.to_string());
                fallback_lines_by_path
                    .entry(path.clone())
                    .or_default()
                    .insert(line_number, added_line.to_string());
                new_line = line_number.checked_add(1);
            } else if let Some(context_line) = line.strip_prefix(' ') {
                fallback_lines_by_path
                    .entry(path.clone())
                    .or_default()
                    .insert(line_number, context_line.to_string());
                new_line = line_number.checked_add(1);
            } else if line.starts_with('-') || line.starts_with('\\') {
                continue;
            }
        }

        Self {
            lines_by_path,
            fallback_lines_by_path,
        }
    }

    pub fn contains(&self, path: &str, line: u32) -> bool {
        self.lines_by_path
            .get(path)
            .is_some_and(|lines| lines.contains_key(&line))
    }

    pub fn resolve_line(&self, path: &str, preferred_line: u32, body: &str) -> Option<u32> {
        if self.contains(path, preferred_line) {
            return Some(preferred_line);
        }

        let candidates = self.lines_by_path.get(path)?;
        let body_tokens = token_set(body);
        let mut best: Option<(u32, usize, u32)> = None;

        for (line, contents) in candidates {
            let score = content_match_score(contents, &body_tokens);
            let distance = line.abs_diff(preferred_line);
            let should_replace = match best {
                None => true,
                Some((best_line, best_score, best_distance)) => {
                    score > best_score
                        || (score == best_score
                            && (distance < best_distance
                                || (distance == best_distance && *line < best_line)))
                }
            };
            if should_replace {
                best = Some((*line, score, distance));
            }
        }

        best.and_then(|(line, score, _)| (score > 0).then_some(line))
    }

    fn first_line(&self, path: &str) -> Option<u32> {
        self.fallback_lines_by_path
            .get(path)
            .and_then(|lines| lines.keys().next().copied())
    }

    fn first_unused_line(&self, path: &str, used: &BTreeSet<(String, u32)>) -> Option<u32> {
        self.fallback_lines_by_path.get(path).and_then(|lines| {
            lines.keys().copied().find(|line| {
                let key = (path.to_string(), *line);
                !used.contains(&key)
            })
        })
    }

    fn first_anchor(&self) -> Option<(String, u32)> {
        self.fallback_lines_by_path
            .iter()
            .find_map(|(path, lines)| {
                lines
                    .keys()
                    .next()
                    .copied()
                    .map(|line| (path.clone(), line))
            })
    }

    fn first_unused_anchor(&self, used: &BTreeSet<(String, u32)>) -> Option<(String, u32)> {
        self.fallback_lines_by_path
            .iter()
            .find_map(|(path, lines)| {
                lines.keys().find_map(|line| {
                    let anchor = (path.clone(), *line);
                    (!used.contains(&anchor)).then_some(anchor)
                })
            })
    }
}

fn parse_diff_new_path(raw_path: &str) -> Option<String> {
    let path = raw_path.split('\t').next().unwrap_or(raw_path).trim();
    if path == "/dev/null" {
        return None;
    }
    Some(path.strip_prefix("b/").unwrap_or(path).to_string())
}

fn parse_new_hunk_start(header: &str) -> Option<u32> {
    header
        .split_whitespace()
        .find_map(|part| part.strip_prefix('+'))
        .and_then(|part| part.split(',').next())
        .and_then(|line| line.parse().ok())
}

fn token_set(value: &str) -> BTreeSet<String> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 3)
        .map(str::to_ascii_lowercase)
        .collect()
}

fn content_match_score(contents: &str, body_tokens: &BTreeSet<String>) -> usize {
    token_set(contents)
        .iter()
        .filter(|token| body_tokens.contains(*token))
        .count()
}

fn encode_marker_payload(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn decode_marker_payload(value: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut index = 0;
    let raw = value.as_bytes();
    while index < raw.len() {
        if raw[index] == b'%' {
            let hi = *raw.get(index + 1)?;
            let lo = *raw.get(index + 2)?;
            let hex = [hi, lo];
            let decoded = u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?;
            bytes.push(decoded);
            index += 3;
        } else {
            bytes.push(raw[index]);
            index += 1;
        }
    }
    String::from_utf8(bytes).ok()
}

pub fn inline_comment_marker(finding_id: &str) -> String {
    format!(
        "{INLINE_COMMENT_MARKER_PREFIX}{} -->",
        encode_marker_payload(finding_id)
    )
}

pub fn finding_comment_marker(finding_id: &str) -> String {
    format!(
        "{FINDING_COMMENT_MARKER_PREFIX}{} -->",
        encode_marker_payload(finding_id)
    )
}

pub fn inline_comment_finding_ids(body: &str) -> Vec<String> {
    marker_finding_ids(body, INLINE_COMMENT_MARKER_PREFIX)
}

pub fn finding_comment_finding_ids(body: &str) -> Vec<String> {
    marker_finding_ids(body, FINDING_COMMENT_MARKER_PREFIX)
}

fn marker_finding_ids(body: &str, marker_prefix: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find(marker_prefix) {
        let payload_start = start + marker_prefix.len();
        let payload_and_rest = &rest[payload_start..];
        let Some(payload_end) = payload_and_rest.find(" -->") else {
            break;
        };
        if let Some(id) = decode_marker_payload(&payload_and_rest[..payload_end]) {
            ids.push(id);
        }
        rest = &payload_and_rest[payload_end + " -->".len()..];
    }
    ids
}

pub fn posted_inline_finding_ids(comments: &[ExistingInlineComment]) -> BTreeSet<String> {
    comments
        .iter()
        .flat_map(|comment| inline_comment_finding_ids(&comment.body))
        .collect()
}

pub fn stale_finding_comment_ids(comments: &[ExistingSummaryComment]) -> Vec<u64> {
    comments
        .iter()
        .filter(|comment| is_github_actions_author(comment.author_login.as_deref()))
        .filter(|comment| !finding_comment_finding_ids(&comment.body).is_empty())
        .map(|comment| comment.id)
        .collect()
}

pub fn render_inline_comment_body(finding: &Finding) -> String {
    let mut body = String::new();
    body.push_str(&inline_comment_marker(&finding.id));
    body.push_str("\n\n");
    append_finding_comment_contents(&mut body, finding);
    body
}

fn append_finding_comment_contents(body: &mut String, finding: &Finding) {
    body.push_str(&format!(
        "**{}: {}**\n\n",
        finding_comment_heading_prefix(finding),
        finding.title
    ));
    body.push_str(&format!(
        "Classification: `{}`\n\nDisposition: `{}`\n\nEvidence gate: `{}`\n\n",
        finding.classification.as_str(),
        if finding.is_blocking(DEFAULT_TARGET_SCORE) {
            "blocking"
        } else {
            "advisory"
        },
        finding.evidence_gate_result.as_str()
    ));
    if let Some(reason) = finding.blocking_reason {
        body.push_str(&format!("Blocking reason: `{}`\n\n", reason.as_str()));
    }
    if let Some(detail) = &finding.detail
        && !detail.trim().is_empty()
    {
        body.push_str(detail.trim());
        body.push_str("\n\n");
    }
    if let Some(grounding) = &finding.grounding {
        body.push_str("Checked claim: ");
        body.push_str(grounding.claim.trim());
        body.push_str("\n\nCausal path: ");
        body.push_str(grounding.causal_path.trim());
        body.push_str("\n\n");
        for evidence in &grounding.evidence {
            body.push_str(&format!(
                "Evidence: `{}` — {}\n\n",
                evidence_location(evidence),
                evidence.reason.trim()
            ));
        }
    }
    if let Some(location) = finding_location(finding) {
        body.push_str("Location: ");
        body.push_str(&location);
        body.push_str("\n\n");
    }
    body.push_str("Agent instruction: ");
    body.push_str(finding.agent_instruction.trim());
}

fn evidence_location(evidence: &FindingEvidence) -> String {
    let location = format!(
        "{}:{}",
        evidence.path.trim().replace('`', "\\`"),
        evidence.line
    );
    match evidence.side {
        reviewgate_core::FindingEvidenceSide::New => location,
        reviewgate_core::FindingEvidenceSide::Old => format!("{location} (deleted line)"),
    }
}

fn finding_comment_heading_prefix(finding: &Finding) -> String {
    let disposition = if finding.is_blocking(DEFAULT_TARGET_SCORE) {
        "Blocking"
    } else {
        "Advisory"
    };
    match finding.angle_id.as_deref().and_then(format_angle_label) {
        Some(label) => format!("{label} / {disposition} / {}", finding.severity.as_str()),
        None => format!("{disposition} / {}", finding.severity.as_str()),
    }
}

fn format_angle_label(angle_id: &str) -> Option<String> {
    let trimmed = angle_id.trim();
    if trimmed.is_empty() {
        return None;
    }
    let label = trimmed
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if label.is_empty() { None } else { Some(label) }
}

fn finding_location(finding: &Finding) -> Option<String> {
    let file = finding.file.as_deref()?;
    if file.trim().is_empty() {
        return None;
    }
    Some(match finding.line {
        Some(line) => format!("`{file}:{line}`"),
        None => format!("`{file}`"),
    })
}

pub fn plan_inline_comment_drafts(
    findings: &[Finding],
    existing_comments: &[ExistingInlineComment],
    min_severity: Severity,
    changed_lines: &ChangedLineSet,
) -> InlineCommentAnchorPlan {
    let existing_ids = posted_inline_finding_ids(existing_comments);
    let mut planned_ids = BTreeSet::new();
    let mut drafts = Vec::new();
    let mut repaired_count = 0u32;
    let mut fallback_count = 0u32;
    let mut skipped_count = 0u32;
    let mut skipped_finding_ids = Vec::new();
    let mut fallback_anchors = FallbackAnchorAllocator::new(changed_lines);

    for finding in findings {
        if !finding.severity.is_at_or_above(min_severity)
            || existing_ids.contains(&finding.id)
            || !planned_ids.insert(finding.id.as_str())
        {
            continue;
        }

        let body = render_inline_comment_body(finding);
        let Some((path, line, anchor_kind)) =
            resolve_finding_inline_anchor(finding, &body, &mut fallback_anchors)
        else {
            skipped_count += 1;
            skipped_finding_ids.push(finding.id.clone());
            continue;
        };
        match anchor_kind {
            InlineAnchorKind::Exact => fallback_anchors.mark_used(&path, line),
            InlineAnchorKind::Repaired => {
                repaired_count += 1;
                fallback_anchors.mark_used(&path, line);
            }
            InlineAnchorKind::Fallback => fallback_count += 1,
        }
        drafts.push(InlineCommentDraft {
            finding_id: finding.id.clone(),
            path,
            line,
            body,
        });
    }

    InlineCommentAnchorPlan {
        drafts,
        repaired_count,
        fallback_count,
        skipped_count,
        skipped_finding_ids,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineAnchorKind {
    Exact,
    Repaired,
    Fallback,
}

struct FallbackAnchorAllocator<'a> {
    changed_lines: &'a ChangedLineSet,
    used: BTreeSet<(String, u32)>,
}

impl<'a> FallbackAnchorAllocator<'a> {
    fn new(changed_lines: &'a ChangedLineSet) -> Self {
        Self {
            changed_lines,
            used: BTreeSet::new(),
        }
    }

    fn file_anchor(&mut self, path: &str) -> Option<(String, u32)> {
        let anchor = self
            .changed_lines
            .first_unused_line(path, &self.used)
            .or_else(|| self.changed_lines.first_line(path))
            .map(|line| (path.to_string(), line))?;
        self.used.insert(anchor.clone());
        Some(anchor)
    }

    fn global_anchor(&mut self) -> Option<(String, u32)> {
        let anchor = self
            .changed_lines
            .first_unused_anchor(&self.used)
            .or_else(|| self.changed_lines.first_anchor())?;
        self.used.insert(anchor.clone());
        Some(anchor)
    }

    fn mark_used(&mut self, path: &str, line: u32) {
        self.used.insert((path.to_string(), line));
    }
}

fn resolve_finding_inline_anchor(
    finding: &Finding,
    body: &str,
    fallback_anchors: &mut FallbackAnchorAllocator<'_>,
) -> Option<(String, u32, InlineAnchorKind)> {
    if let Some(path) = finding
        .file
        .as_deref()
        .filter(|path| !path.trim().is_empty())
    {
        if let Some(preferred_line) = finding.line
            && let Some(line) =
                fallback_anchors
                    .changed_lines
                    .resolve_line(path, preferred_line, body)
        {
            let kind = if line == preferred_line {
                InlineAnchorKind::Exact
            } else {
                InlineAnchorKind::Repaired
            };
            return Some((path.to_string(), line, kind));
        }
        if let Some((path, line)) = fallback_anchors.file_anchor(path) {
            return Some((path, line, InlineAnchorKind::Fallback));
        }
    }

    fallback_anchors
        .global_anchor()
        .map(|(path, line)| (path, line, InlineAnchorKind::Fallback))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reviewgate_core::ReviewArtifact;

    fn rereview_run(
        id: u64,
        repository: &str,
        event: &str,
        status: &str,
        head_sha: &str,
        pull_request_numbers: &[u64],
        created_at: &str,
    ) -> WorkflowRunCandidate {
        WorkflowRunCandidate {
            id,
            url: format!("https://github.com/{repository}/actions/runs/{id}"),
            repository: repository.to_string(),
            event: event.to_string(),
            status: status.to_string(),
            head_sha: head_sha.to_string(),
            pull_request_numbers: pull_request_numbers.to_vec(),
            created_at: created_at.to_string(),
        }
    }

    #[test]
    fn selects_newest_completed_current_head_run_for_exact_pull_request() {
        let runs = vec![
            rereview_run(
                10,
                "LVTD-LLC/reviewgate",
                "pull_request",
                "completed",
                "current",
                &[42],
                "2026-07-28T10:00:00Z",
            ),
            rereview_run(
                11,
                "LVTD-LLC/reviewgate",
                "pull_request",
                "completed",
                "current",
                &[42],
                "2026-07-28T11:00:00Z",
            ),
        ];
        let target = RereviewTarget {
            repository: "LVTD-LLC/reviewgate".to_string(),
            pull_request_number: 42,
            head_sha: "current".to_string(),
        };

        let selected = select_rereview_workflow_run(&runs, &target).expect("eligible run");

        assert_eq!(selected.id, 11);
    }

    #[test]
    fn rejects_foreign_pr_stale_non_pr_in_progress_and_foreign_repository_runs() {
        let runs = vec![
            rereview_run(
                10,
                "LVTD-LLC/reviewgate",
                "pull_request",
                "completed",
                "current",
                &[41],
                "2026-07-28T10:00:00Z",
            ),
            rereview_run(
                11,
                "LVTD-LLC/reviewgate",
                "pull_request",
                "completed",
                "stale",
                &[42],
                "2026-07-28T11:00:00Z",
            ),
            rereview_run(
                12,
                "LVTD-LLC/reviewgate",
                "workflow_dispatch",
                "completed",
                "current",
                &[42],
                "2026-07-28T12:00:00Z",
            ),
            rereview_run(
                13,
                "LVTD-LLC/reviewgate",
                "pull_request",
                "in_progress",
                "current",
                &[42],
                "2026-07-28T13:00:00Z",
            ),
            rereview_run(
                14,
                "other/reviewgate",
                "pull_request",
                "completed",
                "current",
                &[42],
                "2026-07-28T14:00:00Z",
            ),
        ];
        let target = RereviewTarget {
            repository: "LVTD-LLC/reviewgate".to_string(),
            pull_request_number: 42,
            head_sha: "current".to_string(),
        };

        assert!(select_rereview_workflow_run(&runs, &target).is_none());
    }

    #[test]
    fn only_bot_owned_status_marker_suppresses_redelivery() {
        let marker = rereview_status_marker(9001);
        let comments = vec![
            ExistingSummaryComment {
                id: 1,
                author_login: Some("maintainer".to_string()),
                body: marker.clone(),
            },
            ExistingSummaryComment {
                id: 2,
                author_login: Some("github-actions[bot]".to_string()),
                body: marker,
            },
        ];

        assert_eq!(
            find_rereview_status_comment(&comments, 9001).map(|comment| comment.id),
            Some(2)
        );
        assert!(find_rereview_status_comment(&comments, 9002).is_none());
    }

    #[test]
    fn finds_canonical_summary_comment_by_marker() {
        let comments = vec![ExistingSummaryComment {
            id: 1,
            author_login: Some("github-actions[bot]".to_string()),
            body: format!("{}\n# ReviewGate: 4/5", SUMMARY_MARKER),
        }];

        assert_eq!(
            find_summary_comment(&comments).map(|comment| comment.id),
            Some(1)
        );
    }

    #[test]
    fn ignores_user_authored_summary_markers_when_finding_canonical_comment() {
        let comments = vec![
            ExistingSummaryComment {
                id: 1,
                author_login: Some("maintainer".to_string()),
                body: format!("{SUMMARY_MARKER}\n# ReviewGate: forged"),
            },
            ExistingSummaryComment {
                id: 2,
                author_login: Some("github-actions[bot]".to_string()),
                body: format!("{SUMMARY_MARKER}\n# ReviewGate: 5/5"),
            },
        ];

        assert_eq!(
            find_summary_comment(&comments).map(|comment| comment.id),
            Some(2)
        );
    }

    #[test]
    fn plans_duplicate_cleanup_only_for_bot_owned_summary_comments() {
        let comments = vec![
            ExistingSummaryComment {
                id: 1,
                author_login: Some("github-actions[bot]".to_string()),
                body: format!(
                    "{SUMMARY_MARKER}\n\n<!-- reviewgate-state {{\"version\":1,\"last_reviewed_sha\":\"a\",\"reviewed_shas\":[\"a\"],\"run_count\":1,\"cumulative_cost_usd\":0,\"cost_history\":[]}} -->"
                ),
            },
            ExistingSummaryComment {
                id: 2,
                author_login: Some("maintainer".to_string()),
                body: format!("{SUMMARY_MARKER}\nuser-written audit note"),
            },
            ExistingSummaryComment {
                id: 3,
                author_login: Some("github-actions[bot]".to_string()),
                body: format!(
                    "{SUMMARY_MARKER}\n\n<!-- reviewgate-state {{\"version\":1,\"last_reviewed_sha\":\"b\",\"reviewed_shas\":[\"a\",\"b\"],\"run_count\":2,\"cumulative_cost_usd\":0,\"cost_history\":[]}} -->"
                ),
            },
        ];

        let plan = plan_summary_comment_publish(&comments, format!("{SUMMARY_MARKER}\nnew"));

        assert_eq!(plan.primary_id(), Some(3));
        assert_eq!(plan.duplicate_comment_ids, vec![1]);
    }

    #[test]
    fn plans_create_when_summary_comment_is_missing() {
        let action = plan_summary_comment_upsert(&[], format!("{SUMMARY_MARKER}\n# ReviewGate"));

        assert_eq!(
            action,
            SummaryCommentAction::Create {
                body: format!("{SUMMARY_MARKER}\n# ReviewGate")
            }
        );
    }

    #[test]
    fn plans_update_when_summary_comment_exists_with_old_body() {
        let comments = vec![ExistingSummaryComment {
            id: 42,
            author_login: Some("github-actions[bot]".to_string()),
            body: format!("{SUMMARY_MARKER}\n# ReviewGate: 3/5"),
        }];

        let action =
            plan_summary_comment_upsert(&comments, format!("{SUMMARY_MARKER}\n# ReviewGate: 5/5"));

        assert_eq!(
            action,
            SummaryCommentAction::Update {
                id: 42,
                body: format!("{SUMMARY_MARKER}\n# ReviewGate: 5/5")
            }
        );
    }

    #[test]
    fn plans_noop_when_summary_comment_body_matches() {
        let body = format!("{SUMMARY_MARKER}\n# ReviewGate: 5/5");
        let comments = vec![ExistingSummaryComment {
            id: 42,
            author_login: Some("github-actions[bot]".to_string()),
            body: body.clone(),
        }];

        assert_eq!(
            plan_summary_comment_upsert(&comments, body),
            SummaryCommentAction::Noop { id: 42 }
        );
    }

    #[derive(Debug, Default)]
    struct MockSummaryCommentClient {
        created_body: Option<String>,
        updated: Option<(u64, String)>,
    }

    impl SummaryCommentClient for MockSummaryCommentClient {
        type Error = std::convert::Infallible;

        fn create_summary_comment(&mut self, body: &str) -> Result<u64, Self::Error> {
            self.created_body = Some(body.to_string());
            Ok(7)
        }

        fn update_summary_comment(&mut self, id: u64, body: &str) -> Result<(), Self::Error> {
            self.updated = Some((id, body.to_string()));
            Ok(())
        }
    }

    #[test]
    fn upsert_updates_existing_summary_comment() {
        let mut client = MockSummaryCommentClient::default();
        let comments = vec![ExistingSummaryComment {
            id: 42,
            author_login: Some("github-actions[bot]".to_string()),
            body: format!("{SUMMARY_MARKER}\n# ReviewGate: 4/5"),
        }];

        let id = upsert_summary_comment(
            &mut client,
            &comments,
            format!("{SUMMARY_MARKER}\n# ReviewGate: 5/5"),
        )
        .expect("mock update succeeds");

        assert_eq!(id, 42);
        assert_eq!(
            client.updated,
            Some((42, format!("{SUMMARY_MARKER}\n# ReviewGate: 5/5")))
        );
        assert_eq!(client.created_body, None);
    }

    #[test]
    fn github_auth_uses_bearer_header() {
        let auth = GitHubAuth::from_token("ghs_secret");

        assert_eq!(auth.authorization_header(), "Bearer ghs_secret");
        assert_eq!(GITHUB_TOKEN_ENV, "GITHUB_TOKEN");
        assert!(!format!("{auth:?}").contains("ghs_secret"));
    }

    #[test]
    fn plans_inline_comment_for_eligible_line_finding() {
        let finding = Finding {
            id: "rg_001".to_string(),
            angle_id: None,
            scope: reviewgate_core::FindingScope::Line,
            severity: Severity::P1,
            confidence: 0.92,
            classification: reviewgate_core::FindingClassification::Defect,
            evidence_gate_result: reviewgate_core::EvidenceGateResult::Passed,
            blocking_reason: Some(reviewgate_core::BlockingReason::ValidatedDefect),
            grounding: Some(reviewgate_core::FindingGrounding {
                semantic_key: "error_handling.changed_branch".to_string(),
                claim: "The changed branch drops the error.".to_string(),
                causal_path: "request -> changed branch -> silent success".to_string(),
                test_assessment: "No test covers the error branch.".to_string(),
                evidence: vec![FindingEvidence {
                    path: "src/lib.rs".to_string(),
                    side: reviewgate_core::FindingEvidenceSide::New,
                    line: 42,
                    excerpt: "changed".to_string(),
                    reason: "Error is discarded here.".to_string(),
                }],
                related_tests: vec![],
                reproduction: Some("Trigger the error branch.".to_string()),
                proof: None,
                novelty_evidence: None,
                reopening_evidence: None,
            }),
            file: Some("src/lib.rs".to_string()),
            line: Some(42),
            title: "Missing error handling".to_string(),
            detail: Some("The error branch is dropped.".to_string()),
            agent_instruction: "Handle and test the error branch.".to_string(),
        };
        let changed_lines = ChangedLineSet::from_unified_diff(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -40,0 +42 @@\n+changed\n",
        );

        let plan = plan_inline_comment_drafts(&[finding], &[], Severity::P2, &changed_lines);

        assert_eq!(plan.drafts.len(), 1);
        assert_eq!(plan.drafts[0].path, "src/lib.rs");
        assert_eq!(plan.drafts[0].line, 42);
        assert!(
            plan.drafts[0]
                .body
                .contains(&inline_comment_marker("rg_001"))
        );
        assert!(
            plan.drafts[0]
                .body
                .contains("Agent instruction: Handle and test")
        );
        assert!(plan.drafts[0].body.contains("Causal path: request"));
        assert!(plan.drafts[0].body.contains("Evidence: `src/lib.rs:42`"));
    }

    #[test]
    fn labels_deleted_lines_in_published_evidence() {
        let evidence = FindingEvidence {
            path: "src/auth.rs".to_string(),
            side: reviewgate_core::FindingEvidenceSide::Old,
            line: 12,
            excerpt: "assert!(authorized);".to_string(),
            reason: "Deleted authorization guard.".to_string(),
        };

        assert_eq!(
            evidence_location(&evidence),
            "src/auth.rs:12 (deleted line)"
        );
    }

    #[test]
    fn fixture_plans_expected_inline_comment_payloads() {
        let artifact: ReviewArtifact =
            serde_json::from_str(include_str!("../../../fixtures/simple-review.json"))
                .expect("fixture parses");
        let changed_lines = ChangedLineSet::from_unified_diff(
            "diff --git a/app/webhooks/retry.py b/app/webhooks/retry.py\n--- a/app/webhooks/retry.py\n+++ b/app/webhooks/retry.py\n@@ -40,0 +42,2 @@\n+raise RetryExhausted\n+helper_name = 'x'\n",
        );

        let plan =
            plan_inline_comment_drafts(&artifact.findings, &[], Severity::P2, &changed_lines);

        assert_eq!(plan.drafts.len(), 1);
        assert_eq!(plan.drafts[0].finding_id, "rg_001");
        assert_eq!(plan.drafts[0].path, "app/webhooks/retry.py");
        assert_eq!(plan.drafts[0].line, 42);
        assert!(
            plan.drafts[0]
                .body
                .contains(&inline_comment_marker("rg_001"))
        );
        assert!(
            plan.drafts[0]
                .body
                .contains("**Blocking / P2: Missing regression test for retry exhaustion**")
        );
        assert!(
            plan.drafts[0]
                .body
                .contains("Classification: `reliability_risk`")
        );
        assert!(plan.drafts[0].body.contains("Disposition: `blocking`"));
        assert!(!plan.drafts[0].body.contains("rg_002"));
    }

    #[test]
    fn plans_file_and_pr_scope_findings_as_inline_comments_with_fallback_anchors() {
        let findings = vec![
            Finding {
                id: "rg_file".to_string(),
                angle_id: None,
                scope: reviewgate_core::FindingScope::File,
                severity: Severity::P2,
                confidence: 0.72,
                classification: reviewgate_core::FindingClassification::ReliabilityRisk,
                evidence_gate_result: reviewgate_core::EvidenceGateResult::NotRequired,
                blocking_reason: None,
                grounding: None,
                file: Some("src/lib.rs".to_string()),
                line: None,
                title: "Module behavior needs coverage".to_string(),
                detail: Some("The missing case spans multiple lines.".to_string()),
                agent_instruction: "Add module-level coverage.".to_string(),
            },
            Finding {
                id: "rg_pr".to_string(),
                angle_id: None,
                scope: reviewgate_core::FindingScope::Pr,
                severity: Severity::P1,
                confidence: 0.9,
                classification: reviewgate_core::FindingClassification::ReliabilityRisk,
                evidence_gate_result: reviewgate_core::EvidenceGateResult::Passed,
                blocking_reason: Some(reviewgate_core::BlockingReason::ValidatedReliabilityRisk),
                grounding: None,
                file: None,
                line: None,
                title: "Cross-file release risk".to_string(),
                detail: None,
                agent_instruction: "Handle the cross-file release risk.".to_string(),
            },
        ];
        let changed_lines = ChangedLineSet::from_unified_diff(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -10,0 +11,2 @@\n+first lib change\n+second lib change\ndiff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -20,0 +21 @@\n+main change\n",
        );

        let plan = plan_inline_comment_drafts(&findings, &[], Severity::P2, &changed_lines);

        assert_eq!(plan.drafts.len(), 2);
        assert_eq!(plan.fallback_count, 2);
        assert_eq!(plan.skipped_count, 0);
        assert_eq!(plan.drafts[0].finding_id, "rg_file");
        assert_eq!(plan.drafts[0].path, "src/lib.rs");
        assert_eq!(plan.drafts[0].line, 11);
        assert!(plan.drafts[0].body.contains("Location: `src/lib.rs`"));
        assert_eq!(plan.drafts[1].finding_id, "rg_pr");
        assert_eq!(plan.drafts[1].path, "src/lib.rs");
        assert_eq!(plan.drafts[1].line, 12);
        assert!(
            plan.drafts[1]
                .body
                .contains("**Blocking / P1: Cross-file release risk**")
        );
    }

    #[test]
    fn plans_stale_standalone_finding_comments_for_cleanup_only() {
        let comments = vec![
            ExistingSummaryComment {
                id: 7,
                author_login: Some("github-actions[bot]".to_string()),
                body: format!("{}old", finding_comment_marker("rg_file")),
            },
            ExistingSummaryComment {
                id: 8,
                author_login: Some("maintainer".to_string()),
                body: format!("{}forged", finding_comment_marker("rg_user")),
            },
            ExistingSummaryComment {
                id: 9,
                author_login: Some("github-actions[bot]".to_string()),
                body: format!("{}duplicate", finding_comment_marker("rg_file")),
            },
        ];

        assert_eq!(stale_finding_comment_ids(&comments), vec![7, 9]);
    }

    #[test]
    fn renders_finding_comment_with_angle_label_when_present() {
        let raw = r#"{
          "id": "adversarial:rg_001",
          "angle_id": "adversarial",
          "scope": "line",
          "severity": "P2",
          "confidence": 0.9,
          "file": "src/lib.rs",
          "line": 42,
          "title": "Missing error handling",
          "detail": "The error path is dropped.",
          "agent_instruction": "Handle and test the error path."
        }"#;
        let finding: Finding = serde_json::from_str(raw).expect("finding parses");

        let body = render_inline_comment_body(&finding);

        assert!(body.contains(&inline_comment_marker("adversarial:rg_001")));
        assert!(body.contains("**Adversarial / Blocking / P2: Missing error handling**"));
    }

    #[test]
    fn changed_line_set_keeps_only_new_side_added_lines() {
        let diff = r#"diff --git a/crates/reviewgate-cli/src/main.rs b/crates/reviewgate-cli/src/main.rs
index bb299b1..5d4a70e 100644
--- a/crates/reviewgate-cli/src/main.rs
+++ b/crates/reviewgate-cli/src/main.rs
@@ -1630,6 +1630,8 @@ fn build_review_prompt(context: &ReviewContext) -> String {
     prompt.push_str("\nDiff:\n```diff\n");
     prompt.push_str(&context.diff);
+    prompt.push_str("\n\nRepeated diff context:\n");
+    prompt.push_str(&context.diff);
     prompt.push_str("\n```\n");
@@ -1699,7 +1701,7 @@ fn call_openrouter_with_curl(
     let _context = ();
     if !output.status.success() {
         bail!(
-            "OpenRouter request failed: {}",
+            "OpenRouter request failed for key {api_key}: {}",
             String::from_utf8_lossy(&output.stderr).trim()
         );
diff --git a/crates/reviewgate-core/src/lib.rs b/crates/reviewgate-core/src/lib.rs
--- a/crates/reviewgate-core/src/lib.rs
+++ b/crates/reviewgate-core/src/lib.rs
@@ -336,7 +336,7 @@ pub fn compute_score(findings: &[Finding]) -> u8 {
     findings
         .iter()
         .map(|finding| finding.severity.score_ceiling())
-        .min()
+        .max()
         .unwrap_or(5)
}
"#;
        let changed_lines = ChangedLineSet::from_unified_diff(diff);

        assert!(changed_lines.contains("crates/reviewgate-cli/src/main.rs", 1632));
        assert!(changed_lines.contains("crates/reviewgate-cli/src/main.rs", 1633));
        assert!(changed_lines.contains("crates/reviewgate-cli/src/main.rs", 1704));
        assert!(changed_lines.contains("crates/reviewgate-core/src/lib.rs", 339));
        assert!(!changed_lines.contains("crates/reviewgate-cli/src/main.rs", 1630));
        assert!(!changed_lines.contains("crates/reviewgate-core/src/lib.rs", 336));
        assert!(!changed_lines.contains("crates/reviewgate-core/src/lib.rs", 280));
    }

    #[test]
    fn repairs_inline_draft_anchors_to_matching_changed_lines() {
        let changed_lines = ChangedLineSet::from_unified_diff(
            "diff --git a/.github/workflows/reviewgate.yml b/.github/workflows/reviewgate.yml\n--- a/.github/workflows/reviewgate.yml\n+++ b/.github/workflows/reviewgate.yml\n@@ -5,0 +6,2 @@ on:\n+  pull_request_target:\n+    types: [opened, synchronize, reopened, ready_for_review]\n@@ -22 +24 @@ permissions:\n-  contents: read\n+  contents: write\n",
        );
        let findings = vec![
            Finding {
                id: "fork".to_string(),
                angle_id: None,
                scope: reviewgate_core::FindingScope::Line,
                severity: Severity::P1,
                confidence: 0.95,
                classification: reviewgate_core::FindingClassification::Security,
                evidence_gate_result: reviewgate_core::EvidenceGateResult::Passed,
                blocking_reason: Some(reviewgate_core::BlockingReason::ValidatedSecurity),
                grounding: None,
                file: Some(".github/workflows/reviewgate.yml".to_string()),
                line: Some(6),
                title: "Fork-safety guard removed".to_string(),
                detail: Some(
                    "Removal of fork-safety guard enables credential theft via pull_request_target."
                        .to_string(),
                ),
                agent_instruction: "Do not run untrusted fork code with writable credentials."
                    .to_string(),
            },
            Finding {
                id: "permissions".to_string(),
                angle_id: None,
                scope: reviewgate_core::FindingScope::Line,
                severity: Severity::P1,
                confidence: 0.95,
                classification: reviewgate_core::FindingClassification::Security,
                evidence_gate_result: reviewgate_core::EvidenceGateResult::Passed,
                blocking_reason: Some(reviewgate_core::BlockingReason::ValidatedSecurity),
                grounding: None,
                file: Some(".github/workflows/reviewgate.yml".to_string()),
                line: Some(15),
                title: "Token permissions are writable".to_string(),
                detail: Some(
                    "Elevation of GitHub token from read to write. Revert permissions.contents back to read."
                        .to_string(),
                ),
                agent_instruction: "Set contents permission back to read-only.".to_string(),
            },
        ];

        let plan = plan_inline_comment_drafts(&findings, &[], Severity::P2, &changed_lines);

        assert_eq!(plan.repaired_count, 1);
        assert_eq!(plan.skipped_count, 0);
        assert_eq!(plan.drafts.len(), 2);
        assert_eq!(plan.drafts[0].line, 6);
        assert_eq!(plan.drafts[1].line, 24);
    }

    #[test]
    fn falls_back_to_first_file_line_when_text_does_not_match_changed_lines() {
        let changed_lines = ChangedLineSet::from_unified_diff(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -10,0 +11,2 @@\n+first\n+second\n",
        );
        let finding = Finding {
            id: "nearest".to_string(),
            angle_id: None,
            scope: reviewgate_core::FindingScope::Line,
            severity: Severity::P1,
            confidence: 0.95,
            classification: reviewgate_core::FindingClassification::Defect,
            evidence_gate_result: reviewgate_core::EvidenceGateResult::Passed,
            blocking_reason: Some(reviewgate_core::BlockingReason::ValidatedDefect),
            grounding: None,
            file: Some("src/lib.rs".to_string()),
            line: Some(20),
            title: "Unrelated text".to_string(),
            detail: None,
            agent_instruction: "Handle unrelated text.".to_string(),
        };

        let plan = plan_inline_comment_drafts(&[finding], &[], Severity::P2, &changed_lines);

        assert_eq!(plan.repaired_count, 0);
        assert_eq!(plan.fallback_count, 1);
        assert_eq!(plan.skipped_count, 0);
        assert_eq!(plan.drafts.len(), 1);
        assert_eq!(plan.drafts[0].path, "src/lib.rs");
        assert_eq!(plan.drafts[0].line, 11);
    }

    #[test]
    fn falls_back_to_first_pr_line_when_file_has_no_changed_lines() {
        let changed_lines = ChangedLineSet::from_unified_diff(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -10,0 +11 @@\n+changed\n",
        );
        let finding = Finding {
            id: "missing-file".to_string(),
            angle_id: None,
            scope: reviewgate_core::FindingScope::Line,
            severity: Severity::P1,
            confidence: 0.95,
            classification: reviewgate_core::FindingClassification::Defect,
            evidence_gate_result: reviewgate_core::EvidenceGateResult::Passed,
            blocking_reason: Some(reviewgate_core::BlockingReason::ValidatedDefect),
            grounding: None,
            file: Some("src/other.rs".to_string()),
            line: Some(11),
            title: "Changed".to_string(),
            detail: None,
            agent_instruction: "Handle changed behavior.".to_string(),
        };

        let plan = plan_inline_comment_drafts(&[finding], &[], Severity::P2, &changed_lines);

        assert_eq!(plan.repaired_count, 0);
        assert_eq!(plan.fallback_count, 1);
        assert_eq!(plan.skipped_count, 0);
        assert_eq!(plan.drafts.len(), 1);
        assert_eq!(plan.drafts[0].path, "src/lib.rs");
        assert_eq!(plan.drafts[0].line, 11);
    }

    #[test]
    fn skips_duplicates_and_low_severity_without_confidence_filtering() {
        let duplicate = Finding {
            id: "rg_dup".to_string(),
            angle_id: None,
            scope: reviewgate_core::FindingScope::Line,
            severity: Severity::P1,
            confidence: 0.95,
            classification: reviewgate_core::FindingClassification::Defect,
            evidence_gate_result: reviewgate_core::EvidenceGateResult::Passed,
            blocking_reason: Some(reviewgate_core::BlockingReason::ValidatedDefect),
            grounding: None,
            file: Some("src/lib.rs".to_string()),
            line: Some(10),
            title: "Already posted".to_string(),
            detail: None,
            agent_instruction: "No duplicate.".to_string(),
        };
        let low_confidence = Finding {
            id: "rg_low".to_string(),
            confidence: 0.5,
            evidence_gate_result: reviewgate_core::EvidenceGateResult::NotRequired,
            blocking_reason: None,
            ..duplicate.clone()
        };
        let no_line = Finding {
            id: "rg_no_line".to_string(),
            line: None,
            ..duplicate.clone()
        };
        let existing = ExistingInlineComment {
            id: 9,
            body: inline_comment_marker("rg_dup"),
        };
        let changed_lines = ChangedLineSet::from_unified_diff(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -9,0 +10,2 @@\n+first change\n+second change\n",
        );

        let plan = plan_inline_comment_drafts(
            &[duplicate, low_confidence, no_line],
            &[existing],
            Severity::P2,
            &changed_lines,
        );

        assert_eq!(plan.drafts.len(), 2);
        assert_eq!(plan.drafts[0].finding_id, "rg_low");
        assert!(
            plan.drafts[0]
                .body
                .contains("**Advisory / P1: Already posted**")
        );
        assert!(plan.drafts[0].body.contains("Disposition: `advisory`"));
        assert!(!plan.drafts[0].body.contains("Blocking reason:"));
        assert_eq!(plan.drafts[1].finding_id, "rg_no_line");
        assert_eq!(plan.drafts[1].line, 11);
    }

    #[test]
    fn falls_back_to_first_pr_anchor_when_finding_file_has_no_changed_lines() {
        let file_scope = Finding {
            id: "rg_file".to_string(),
            angle_id: None,
            scope: reviewgate_core::FindingScope::File,
            severity: Severity::P1,
            confidence: 0.95,
            classification: reviewgate_core::FindingClassification::Defect,
            evidence_gate_result: reviewgate_core::EvidenceGateResult::Passed,
            blocking_reason: Some(reviewgate_core::BlockingReason::ValidatedDefect),
            grounding: None,
            file: Some("src/lib.rs".to_string()),
            line: Some(10),
            title: "File-level concern".to_string(),
            detail: None,
            agent_instruction: "Handle at file scope.".to_string(),
        };
        let pr_scope = Finding {
            id: "rg_pr".to_string(),
            angle_id: None,
            scope: reviewgate_core::FindingScope::Pr,
            title: "PR-level concern".to_string(),
            agent_instruction: "Handle at PR scope.".to_string(),
            ..file_scope.clone()
        };
        let changed_lines = ChangedLineSet::from_unified_diff(
            "diff --git a/src/other.rs b/src/other.rs\n--- a/src/other.rs\n+++ b/src/other.rs\n@@ -4,0 +5 @@\n+other change\n",
        );

        let plan =
            plan_inline_comment_drafts(&[file_scope, pr_scope], &[], Severity::P2, &changed_lines);

        assert_eq!(plan.drafts.len(), 2);
        assert_eq!(plan.fallback_count, 2);
        assert_eq!(plan.drafts[0].path, "src/other.rs");
        assert_eq!(plan.drafts[0].line, 5);
        assert_eq!(plan.drafts[1].path, "src/other.rs");
        assert_eq!(plan.drafts[1].line, 5);
    }

    #[test]
    fn fallback_anchors_can_use_right_side_context_lines() {
        let finding = Finding {
            id: "rg_file".to_string(),
            angle_id: None,
            scope: reviewgate_core::FindingScope::File,
            severity: Severity::P1,
            confidence: 0.95,
            classification: reviewgate_core::FindingClassification::Defect,
            evidence_gate_result: reviewgate_core::EvidenceGateResult::Passed,
            blocking_reason: Some(reviewgate_core::BlockingReason::ValidatedDefect),
            grounding: None,
            file: Some("src/lib.rs".to_string()),
            line: None,
            title: "File-level concern".to_string(),
            detail: None,
            agent_instruction: "Handle at file scope.".to_string(),
        };
        let changed_lines = ChangedLineSet::from_unified_diff(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -10,3 +10,2 @@\n context before\n-removed line\n context after\n",
        );

        let plan = plan_inline_comment_drafts(&[finding], &[], Severity::P2, &changed_lines);

        assert!(!changed_lines.contains("src/lib.rs", 10));
        assert_eq!(plan.drafts.len(), 1);
        assert_eq!(plan.fallback_count, 1);
        assert_eq!(plan.drafts[0].path, "src/lib.rs");
        assert_eq!(plan.drafts[0].line, 10);
    }

    #[test]
    fn records_skipped_finding_ids_when_no_anchor_exists() {
        let finding = Finding {
            id: "rg_no_anchor".to_string(),
            angle_id: None,
            scope: reviewgate_core::FindingScope::Pr,
            severity: Severity::P1,
            confidence: 0.95,
            classification: reviewgate_core::FindingClassification::Defect,
            evidence_gate_result: reviewgate_core::EvidenceGateResult::Passed,
            blocking_reason: Some(reviewgate_core::BlockingReason::ValidatedDefect),
            grounding: None,
            file: None,
            line: None,
            title: "No anchor available".to_string(),
            detail: None,
            agent_instruction: "Keep this visible in logs.".to_string(),
        };
        let changed_lines = ChangedLineSet::default();

        let plan = plan_inline_comment_drafts(&[finding], &[], Severity::P2, &changed_lines);

        assert!(plan.drafts.is_empty());
        assert_eq!(plan.skipped_count, 1);
        assert_eq!(plan.skipped_finding_ids, vec!["rg_no_anchor".to_string()]);
    }

    #[test]
    fn inline_marker_payload_round_trips_schema_valid_ids() {
        assert_eq!(
            inline_comment_marker("missing auth check"),
            "<!-- reviewgate-finding:missing%20auth%20check -->"
        );
        assert_eq!(
            inline_comment_marker("A-->B\nC"),
            "<!-- reviewgate-finding:A--%3EB%0AC -->"
        );
    }

    #[test]
    fn extracts_posted_inline_finding_ids_from_markers() {
        let comments = vec![
            ExistingInlineComment {
                id: 1,
                body: render_inline_comment_body(&Finding {
                    id: "missing auth check".to_string(),
                    angle_id: None,
                    scope: reviewgate_core::FindingScope::Line,
                    severity: Severity::P1,
                    confidence: 0.95,
                    classification: reviewgate_core::FindingClassification::Defect,
                    evidence_gate_result: reviewgate_core::EvidenceGateResult::Passed,
                    blocking_reason: Some(reviewgate_core::BlockingReason::ValidatedDefect),
                    grounding: None,
                    file: Some("src/lib.rs".to_string()),
                    line: Some(10),
                    title: "Already posted".to_string(),
                    detail: None,
                    agent_instruction: "No duplicate.".to_string(),
                }),
            },
            ExistingInlineComment {
                id: 2,
                body: "unrelated".to_string(),
            },
        ];

        let ids = posted_inline_finding_ids(&comments);

        assert!(ids.contains("missing auth check"));
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn dedupes_inline_comments_with_encoded_markers() {
        let finding = Finding {
            id: "A-->B\nC".to_string(),
            angle_id: None,
            scope: reviewgate_core::FindingScope::Line,
            severity: Severity::P1,
            confidence: 0.95,
            classification: reviewgate_core::FindingClassification::Defect,
            evidence_gate_result: reviewgate_core::EvidenceGateResult::Passed,
            blocking_reason: Some(reviewgate_core::BlockingReason::ValidatedDefect),
            grounding: None,
            file: Some("src/lib.rs".to_string()),
            line: Some(10),
            title: "Already posted".to_string(),
            detail: None,
            agent_instruction: "No duplicate.".to_string(),
        };
        let existing = ExistingInlineComment {
            id: 9,
            body: inline_comment_marker(&finding.id),
        };

        let changed_lines = ChangedLineSet::from_unified_diff(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -9,0 +10 @@\n+changed\n",
        );

        let plan =
            plan_inline_comment_drafts(&[finding], &[existing], Severity::P2, &changed_lines);

        assert!(plan.drafts.is_empty());
    }
}
