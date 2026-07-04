use std::collections::{BTreeMap, BTreeSet};

use reviewgate_core::{Finding, SUMMARY_MARKER, SecretString, Severity, extract_summary_state};

pub const GITHUB_TOKEN_ENV: &str = "GITHUB_TOKEN";
pub const INLINE_COMMENT_MARKER_PREFIX: &str = "<!-- reviewgate-finding:";
pub const FINDING_COMMENT_MARKER_PREFIX: &str = "<!-- reviewgate-finding-comment:";

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

pub fn resolve_inline_comment_drafts_to_changed_lines(
    drafts: Vec<InlineCommentDraft>,
    changed_lines: &ChangedLineSet,
) -> InlineCommentAnchorPlan {
    let mut resolved = Vec::new();
    let mut repaired_count = 0u32;
    let mut fallback_count = 0u32;
    let mut skipped_count = 0u32;

    for mut draft in drafts {
        if let Some(line) = changed_lines.resolve_line(&draft.path, draft.line, &draft.body) {
            if line != draft.line {
                repaired_count += 1;
                draft.line = line;
            }
        } else if let Some(line) = changed_lines.first_line(&draft.path) {
            fallback_count += 1;
            draft.line = line;
        } else if let Some((path, line)) = changed_lines.first_anchor() {
            fallback_count += 1;
            draft.path = path;
            draft.line = line;
        } else {
            skipped_count += 1;
            continue;
        }
        resolved.push(draft);
    }

    InlineCommentAnchorPlan {
        drafts: resolved,
        repaired_count,
        fallback_count,
        skipped_count,
    }
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
    if let Some(detail) = &finding.detail
        && !detail.trim().is_empty()
    {
        body.push_str(detail.trim());
        body.push_str("\n\n");
    }
    if let Some(location) = finding_location(finding) {
        body.push_str("Location: ");
        body.push_str(&location);
        body.push_str("\n\n");
    }
    body.push_str("Agent instruction: ");
    body.push_str(finding.agent_instruction.trim());
}

fn finding_comment_heading_prefix(finding: &Finding) -> String {
    match finding.angle_id.as_deref().and_then(format_angle_label) {
        Some(label) => format!("{label} / {}", finding.severity.as_str()),
        None => finding.severity.as_str().to_string(),
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

    for finding in findings {
        if !finding.severity.is_at_or_above(min_severity)
            || existing_ids.contains(&finding.id)
            || !planned_ids.insert(finding.id.as_str())
        {
            continue;
        }

        let body = render_inline_comment_body(finding);
        let Some((path, line, anchor_kind)) =
            resolve_finding_inline_anchor(finding, &body, changed_lines)
        else {
            skipped_count += 1;
            continue;
        };
        match anchor_kind {
            InlineAnchorKind::Exact => {}
            InlineAnchorKind::Repaired => repaired_count += 1,
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
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineAnchorKind {
    Exact,
    Repaired,
    Fallback,
}

fn resolve_finding_inline_anchor(
    finding: &Finding,
    body: &str,
    changed_lines: &ChangedLineSet,
) -> Option<(String, u32, InlineAnchorKind)> {
    if let Some(path) = finding
        .file
        .as_deref()
        .filter(|path| !path.trim().is_empty())
    {
        if let Some(preferred_line) = finding.line
            && let Some(line) = changed_lines.resolve_line(path, preferred_line, body)
        {
            let kind = if line == preferred_line {
                InlineAnchorKind::Exact
            } else {
                InlineAnchorKind::Repaired
            };
            return Some((path.to_string(), line, kind));
        }
        if let Some(line) = changed_lines.first_line(path) {
            return Some((path.to_string(), line, InlineAnchorKind::Fallback));
        }
    }

    changed_lines
        .first_anchor()
        .map(|(path, line)| (path, line, InlineAnchorKind::Fallback))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reviewgate_core::ReviewArtifact;

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
                .contains("**P2: Missing regression test for retry exhaustion**")
        );
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
        assert_eq!(plan.drafts[1].line, 11);
        assert!(
            plan.drafts[1]
                .body
                .contains("**P1: Cross-file release risk**")
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
        assert!(body.contains("**Adversarial / P2: Missing error handling**"));
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
        let drafts = vec![
            InlineCommentDraft {
                finding_id: "fork".to_string(),
                path: ".github/workflows/reviewgate.yml".to_string(),
                line: 6,
                body: "Removal of fork-safety guard enables credential theft via pull_request_target".to_string(),
            },
            InlineCommentDraft {
                finding_id: "permissions".to_string(),
                path: ".github/workflows/reviewgate.yml".to_string(),
                line: 15,
                body: "Elevation of GitHub token from read to write. Revert permissions.contents back to read.".to_string(),
            },
        ];

        let plan = resolve_inline_comment_drafts_to_changed_lines(drafts, &changed_lines);

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
        let drafts = vec![InlineCommentDraft {
            finding_id: "nearest".to_string(),
            path: "src/lib.rs".to_string(),
            line: 20,
            body: "Unrelated text".to_string(),
        }];

        let plan = resolve_inline_comment_drafts_to_changed_lines(drafts, &changed_lines);

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
        let drafts = vec![InlineCommentDraft {
            finding_id: "missing-file".to_string(),
            path: "src/other.rs".to_string(),
            line: 11,
            body: "Changed".to_string(),
        }];

        let plan = resolve_inline_comment_drafts_to_changed_lines(drafts, &changed_lines);

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
            file: Some("src/lib.rs".to_string()),
            line: Some(10),
            title: "Already posted".to_string(),
            detail: None,
            agent_instruction: "No duplicate.".to_string(),
        };
        let low_confidence = Finding {
            id: "rg_low".to_string(),
            confidence: 0.5,
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
        assert_eq!(plan.drafts[1].finding_id, "rg_no_line");
        assert_eq!(plan.drafts[1].line, 10);
    }

    #[test]
    fn falls_back_to_first_pr_anchor_when_finding_file_has_no_changed_lines() {
        let file_scope = Finding {
            id: "rg_file".to_string(),
            angle_id: None,
            scope: reviewgate_core::FindingScope::File,
            severity: Severity::P1,
            confidence: 0.95,
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
