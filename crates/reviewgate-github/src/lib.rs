use reviewgate_core::SUMMARY_MARKER;

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
}
