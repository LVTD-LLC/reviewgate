use reviewgate_core::{Finding, SUMMARY_MARKER, SecretString, Severity};

pub const GITHUB_TOKEN_ENV: &str = "GITHUB_TOKEN";
pub const INLINE_COMMENT_MARKER_PREFIX: &str = "<!-- review-gate-finding:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingSummaryComment {
    pub id: u64,
    pub body: String,
}

pub fn find_summary_comment(
    comments: &[ExistingSummaryComment],
) -> Option<&ExistingSummaryComment> {
    comments
        .iter()
        .find(|comment| comment.body.contains(SUMMARY_MARKER))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryCommentAction {
    Create { body: String },
    Update { id: u64, body: String },
    Noop { id: u64 },
}

pub fn plan_summary_comment_upsert(
    comments: &[ExistingSummaryComment],
    rendered_summary: impl Into<String>,
) -> SummaryCommentAction {
    let body = rendered_summary.into();
    if let Some(existing) = find_summary_comment(comments) {
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
    }
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

pub fn inline_comment_marker(finding_id: &str) -> String {
    format!("{INLINE_COMMENT_MARKER_PREFIX}{finding_id} -->")
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
    if let Some(detail) = &finding.detail {
        if !detail.trim().is_empty() {
            body.push_str(detail.trim());
            body.push_str("\n\n");
        }
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

    #[test]
    fn finds_canonical_summary_comment_by_marker() {
        let comments = vec![ExistingSummaryComment {
            id: 1,
            body: format!("{}\n# Review Gate: 4/5", SUMMARY_MARKER),
        }];

        assert_eq!(
            find_summary_comment(&comments).map(|comment| comment.id),
            Some(1)
        );
    }

    #[test]
    fn plans_create_when_summary_comment_is_missing() {
        let action = plan_summary_comment_upsert(&[], format!("{SUMMARY_MARKER}\n# Review Gate"));

        assert_eq!(
            action,
            SummaryCommentAction::Create {
                body: format!("{SUMMARY_MARKER}\n# Review Gate")
            }
        );
    }

    #[test]
    fn plans_update_when_summary_comment_exists_with_old_body() {
        let comments = vec![ExistingSummaryComment {
            id: 42,
            body: format!("{SUMMARY_MARKER}\n# Review Gate: 3/5"),
        }];

        let action =
            plan_summary_comment_upsert(&comments, format!("{SUMMARY_MARKER}\n# Review Gate: 5/5"));

        assert_eq!(
            action,
            SummaryCommentAction::Update {
                id: 42,
                body: format!("{SUMMARY_MARKER}\n# Review Gate: 5/5")
            }
        );
    }

    #[test]
    fn plans_noop_when_summary_comment_body_matches() {
        let body = format!("{SUMMARY_MARKER}\n# Review Gate: 5/5");
        let comments = vec![ExistingSummaryComment {
            id: 42,
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
            body: format!("{SUMMARY_MARKER}\n# Review Gate: 4/5"),
        }];

        let id = upsert_summary_comment(
            &mut client,
            &comments,
            format!("{SUMMARY_MARKER}\n# Review Gate: 5/5"),
        )
        .expect("mock update succeeds");

        assert_eq!(id, 42);
        assert_eq!(
            client.updated,
            Some((42, format!("{SUMMARY_MARKER}\n# Review Gate: 5/5")))
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
        assert!(
            drafts[0]
                .body
                .contains("<!-- review-gate-finding:rg_001 -->")
        );
        assert!(drafts[0].body.contains("Agent instruction: Handle and test"));
    }

    #[test]
    fn skips_ineligible_and_duplicate_inline_comments() {
        let duplicate = Finding {
            id: "rg_dup".to_string(),
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
}
