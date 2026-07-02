use std::collections::BTreeSet;

use reviewgate_core::{
    Finding, FindingScope, SUMMARY_MARKER, SecretString, Severity, extract_summary_state,
};

pub const GITHUB_TOKEN_ENV: &str = "GITHUB_TOKEN";
pub const INLINE_COMMENT_MARKER_PREFIX: &str = "<!-- reviewgate-finding:";

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
    comment.body.contains(SUMMARY_MARKER)
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
    lines: BTreeSet<(String, u32)>,
}

impl ChangedLineSet {
    pub fn from_unified_diff(diff: &str) -> Self {
        let mut lines = BTreeSet::new();
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

            if line.starts_with('+') {
                lines.insert((path.clone(), line_number));
                new_line = line_number.checked_add(1);
            } else if line.starts_with(' ') {
                new_line = line_number.checked_add(1);
            } else if line.starts_with('-') || line.starts_with('\\') {
                continue;
            }
        }

        Self { lines }
    }

    pub fn contains(&self, path: &str, line: u32) -> bool {
        self.lines.contains(&(path.to_string(), line))
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

pub fn filter_inline_comment_drafts_to_changed_lines(
    drafts: Vec<InlineCommentDraft>,
    changed_lines: &ChangedLineSet,
) -> Vec<InlineCommentDraft> {
    drafts
        .into_iter()
        .filter(|draft| changed_lines.contains(&draft.path, draft.line))
        .collect()
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
    format!("{INLINE_COMMENT_MARKER_PREFIX}{finding_id} -->")
}

pub fn inline_comment_finding_ids(body: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find(INLINE_COMMENT_MARKER_PREFIX) {
        let payload_start = start + INLINE_COMMENT_MARKER_PREFIX.len();
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

pub fn render_inline_comment_body(finding: &Finding) -> String {
    let mut body = String::new();
    body.push_str(&inline_comment_marker(&finding.id));
    body.push_str("\n\n");
    body.push_str(&format!(
        "**{}: {}**\n\n",
        finding.severity.as_str(),
        finding.title
    ));
    if let Some(detail) = &finding.detail
        && !detail.trim().is_empty()
    {
        body.push_str(detail.trim());
        body.push_str("\n\n");
    }
    body.push_str("Agent instruction: ");
    body.push_str(finding.agent_instruction.trim());
    body
}

pub fn plan_inline_comment_drafts(
    findings: &[Finding],
    existing_comments: &[ExistingInlineComment],
    inline_min_severity: Severity,
    min_confidence: f64,
) -> Vec<InlineCommentDraft> {
    findings
        .iter()
        .filter(|finding| finding.confidence >= min_confidence)
        .filter(|finding| finding.scope == FindingScope::Line)
        .filter(|finding| finding.severity.is_at_or_above(inline_min_severity))
        .filter_map(|finding| {
            let path = finding.file.as_ref()?;
            let line = finding.line?;
            let marker = inline_comment_marker(&finding.id);
            if existing_comments
                .iter()
                .any(|comment| comment.body.contains(&marker))
            {
                return None;
            }
            Some(InlineCommentDraft {
                finding_id: finding.id.clone(),
                path: path.clone(),
                line,
                body: render_inline_comment_body(finding),
            })
        })
        .collect()
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
            scope: reviewgate_core::FindingScope::Line,
            severity: Severity::P1,
            confidence: 0.92,
            file: Some("src/lib.rs".to_string()),
            line: Some(42),
            title: "Missing error handling".to_string(),
            detail: Some("The error branch is dropped.".to_string()),
            agent_instruction: "Handle and test the error branch.".to_string(),
        };

        let drafts = plan_inline_comment_drafts(&[finding], &[], Severity::P2, 0.8);

        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].path, "src/lib.rs");
        assert_eq!(drafts[0].line, 42);
        assert!(drafts[0].body.contains(&inline_comment_marker("rg_001")));
        assert!(
            drafts[0]
                .body
                .contains("Agent instruction: Handle and test")
        );
    }

    #[test]
    fn fixture_plans_expected_inline_comment_payloads() {
        let artifact: ReviewArtifact =
            serde_json::from_str(include_str!("../../../fixtures/simple-review.json"))
                .expect("fixture parses");

        let drafts = plan_inline_comment_drafts(&artifact.findings, &[], Severity::P2, 0.8);

        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].finding_id, "rg_001");
        assert_eq!(drafts[0].path, "app/webhooks/retry.py");
        assert_eq!(drafts[0].line, 42);
        assert!(drafts[0].body.contains(&inline_comment_marker("rg_001")));
        assert!(
            drafts[0]
                .body
                .contains("**P2: Missing regression test for retry exhaustion**")
        );
        assert!(!drafts[0].body.contains("rg_002"));
    }

    #[test]
    fn changed_line_set_keeps_only_new_side_added_lines() {
        let diff = r#"diff --git a/crates/reviewgate-cli/src/main.rs b/crates/reviewgate-cli/src/main.rs
index bb299b1..5d4a70e 100644
--- a/crates/reviewgate-cli/src/main.rs
+++ b/crates/reviewgate-cli/src/main.rs
@@ -1630,6 +1630,8 @@ fn build_review_prompt(context: &ReviewContext, target_score: u8) -> String {
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
    fn filters_inline_drafts_to_changed_lines() {
        let changed_lines = ChangedLineSet::from_unified_diff(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -10,3 +10,4 @@\n context\n+changed\n unchanged\n",
        );
        let drafts = vec![
            InlineCommentDraft {
                finding_id: "changed".to_string(),
                path: "src/lib.rs".to_string(),
                line: 11,
                body: "changed".to_string(),
            },
            InlineCommentDraft {
                finding_id: "context".to_string(),
                path: "src/lib.rs".to_string(),
                line: 10,
                body: "context".to_string(),
            },
        ];

        let drafts = filter_inline_comment_drafts_to_changed_lines(drafts, &changed_lines);

        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].finding_id, "changed");
    }

    #[test]
    fn skips_ineligible_and_duplicate_inline_comments() {
        let duplicate = Finding {
            id: "rg_dup".to_string(),
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

        let drafts = plan_inline_comment_drafts(
            &[duplicate, low_confidence, no_line],
            &[existing],
            Severity::P2,
            0.8,
        );

        assert!(drafts.is_empty());
    }

    #[test]
    fn skips_file_and_pr_scope_findings_for_inline_comments() {
        let file_scope = Finding {
            id: "rg_file".to_string(),
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
            scope: reviewgate_core::FindingScope::Pr,
            title: "PR-level concern".to_string(),
            agent_instruction: "Handle at PR scope.".to_string(),
            ..file_scope.clone()
        };

        let drafts = plan_inline_comment_drafts(&[file_scope, pr_scope], &[], Severity::P2, 0.8);

        assert!(drafts.is_empty());
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

        let drafts = plan_inline_comment_drafts(&[finding], &[existing], Severity::P2, 0.8);

        assert!(drafts.is_empty());
    }
}
