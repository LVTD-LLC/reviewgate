use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

pub const BENCHMARK_MANIFEST_SCHEMA_VERSION: &str = "reviewgate-benchmark-manifest/v1";
pub const BENCHMARK_REPORT_SCHEMA_VERSION: &str = "reviewgate-benchmark-report/v1";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkManifest {
    pub schema_version: String,
    pub name: String,
    pub blinded: bool,
    pub minimum_case_count: usize,
    pub repetitions: usize,
    pub thresholds: BenchmarkThresholds,
    pub sources: Vec<BenchmarkSource>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkThresholds {
    pub minimum_blocking_precision: f64,
    pub minimum_serious_defect_recall: f64,
    pub maximum_false_blockers_per_case: f64,
    pub maximum_contradiction_rate: f64,
    pub minimum_rereview_stability: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkSource {
    pub id: String,
    pub kind: BenchmarkSourceKind,
    pub path: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkSourceKind {
    EvidenceGrounding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaseObservation {
    pub expected_blocking: bool,
    pub observed_blocking: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConfigurationMetrics {
    pub case_count: usize,
    pub expected_serious_defects: usize,
    pub observed_blockers: usize,
    pub true_positives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
    pub true_negatives: usize,
    pub blocking_precision: Option<f64>,
    pub serious_defect_recall: Option<f64>,
    pub false_blockers_per_case: f64,
    pub contradiction_rate: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkExpectedFinding {
    pub semantic_key: String,
    pub adjudicated_claim: String,
    pub serious: bool,
    pub expected_blocking: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkKnownNonFinding {
    pub semantic_key: String,
    pub adjudicated_claim: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkCaseExpectation {
    pub case_id: String,
    pub expected_findings: Vec<BenchmarkExpectedFinding>,
    pub known_non_findings: Vec<BenchmarkKnownNonFinding>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkObservedFinding {
    pub semantic_key: String,
    pub blocking: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkObservedRun {
    pub case_id: String,
    pub repetition: usize,
    pub review_complete: bool,
    pub findings: Vec<BenchmarkObservedFinding>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct BenchmarkRunScore {
    pub case_id: String,
    pub repetition: usize,
    pub review_complete: bool,
    pub expected_serious_defects: usize,
    pub detected_serious_defects: usize,
    pub missed_serious_defects: Vec<String>,
    pub observed_blockers: usize,
    pub true_blockers: usize,
    pub false_blockers: usize,
    pub contradicted_non_findings: Vec<String>,
    pub unexpected_blockers: Vec<String>,
    pub duplicate_findings: Vec<String>,
}

pub fn validate_manifest(manifest: &BenchmarkManifest) -> Result<(), String> {
    if manifest.schema_version != BENCHMARK_MANIFEST_SCHEMA_VERSION {
        return Err(format!(
            "unsupported benchmark manifest schema `{}`; expected `{BENCHMARK_MANIFEST_SCHEMA_VERSION}`",
            manifest.schema_version
        ));
    }
    if manifest.name.trim().is_empty() {
        return Err("benchmark manifest name must not be empty".to_string());
    }
    if !manifest.blinded {
        return Err("benchmark manifest must declare blinded: true".to_string());
    }
    if manifest.minimum_case_count < 30 {
        return Err("benchmark manifest minimum_case_count must be at least 30".to_string());
    }
    if manifest.repetitions < 2 {
        return Err("benchmark manifest repetitions must be at least 2".to_string());
    }
    validate_rate(
        "minimum_blocking_precision",
        manifest.thresholds.minimum_blocking_precision,
    )?;
    validate_rate(
        "minimum_serious_defect_recall",
        manifest.thresholds.minimum_serious_defect_recall,
    )?;
    validate_rate(
        "maximum_false_blockers_per_case",
        manifest.thresholds.maximum_false_blockers_per_case,
    )?;
    validate_rate(
        "maximum_contradiction_rate",
        manifest.thresholds.maximum_contradiction_rate,
    )?;
    validate_rate(
        "minimum_rereview_stability",
        manifest.thresholds.minimum_rereview_stability,
    )?;
    if manifest.sources.is_empty() {
        return Err("benchmark manifest must contain at least one source".to_string());
    }
    let mut source_ids = std::collections::BTreeSet::new();
    for source in &manifest.sources {
        if source.id.trim().is_empty() {
            return Err("benchmark source id must not be empty".to_string());
        }
        if !source_ids.insert(source.id.as_str()) {
            return Err(format!("duplicate benchmark source id `{}`", source.id));
        }
        if !safe_repo_relative_path(&source.path) {
            return Err(format!(
                "benchmark source path `{}` must be a repo-relative path without `..`",
                source.path
            ));
        }
    }
    Ok(())
}

fn validate_rate(name: &str, value: f64) -> Result<(), String> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(format!("{name} must be finite and between 0 and 1"));
    }
    Ok(())
}

fn safe_repo_relative_path(value: &str) -> bool {
    let path = Path::new(value);
    !value.trim().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

pub fn configuration_metrics(observations: &[CaseObservation]) -> ConfigurationMetrics {
    let mut true_positives = 0;
    let mut false_positives = 0;
    let mut false_negatives = 0;
    let mut true_negatives = 0;
    for observation in observations {
        match (observation.expected_blocking, observation.observed_blocking) {
            (true, true) => true_positives += 1,
            (false, true) => false_positives += 1,
            (true, false) => false_negatives += 1,
            (false, false) => true_negatives += 1,
        }
    }
    let observed_blockers = true_positives + false_positives;
    let expected_serious_defects = true_positives + false_negatives;
    let expected_non_findings = false_positives + true_negatives;
    let case_count = observations.len();
    ConfigurationMetrics {
        case_count,
        expected_serious_defects,
        observed_blockers,
        true_positives,
        false_positives,
        false_negatives,
        true_negatives,
        blocking_precision: ratio(true_positives, observed_blockers),
        serious_defect_recall: ratio(true_positives, expected_serious_defects),
        false_blockers_per_case: if case_count == 0 {
            0.0
        } else {
            false_positives as f64 / case_count as f64
        },
        contradiction_rate: ratio(false_positives, expected_non_findings),
    }
}

pub fn score_benchmark_run(
    expectation: &BenchmarkCaseExpectation,
    run: &BenchmarkObservedRun,
) -> Result<BenchmarkRunScore, String> {
    if expectation.case_id != run.case_id {
        return Err(format!(
            "benchmark expectation case id `{}` does not match run case id `{}`",
            expectation.case_id, run.case_id
        ));
    }
    if expectation.case_id.trim().is_empty() {
        return Err("benchmark case id must not be empty".to_string());
    }
    if run.repetition == 0 {
        return Err("benchmark run repetition must be at least 1".to_string());
    }

    let mut expected_by_key = std::collections::BTreeMap::new();
    for expected in &expectation.expected_findings {
        validate_semantic_expectation(
            "expected finding",
            &expected.semantic_key,
            &expected.adjudicated_claim,
        )?;
        if expected_by_key
            .insert(expected.semantic_key.as_str(), expected)
            .is_some()
        {
            return Err(format!(
                "duplicate expected finding semantic_key `{}`",
                expected.semantic_key
            ));
        }
    }
    let mut known_non_findings = std::collections::BTreeSet::new();
    for known in &expectation.known_non_findings {
        validate_semantic_expectation(
            "known non-finding",
            &known.semantic_key,
            &known.adjudicated_claim,
        )?;
        if expected_by_key.contains_key(known.semantic_key.as_str()) {
            return Err(format!(
                "semantic_key `{}` cannot be both expected and a known non-finding",
                known.semantic_key
            ));
        }
        if !known_non_findings.insert(known.semantic_key.as_str()) {
            return Err(format!(
                "duplicate known non-finding semantic_key `{}`",
                known.semantic_key
            ));
        }
    }

    let mut seen_keys = std::collections::BTreeSet::new();
    let mut detected_serious_keys = std::collections::BTreeSet::new();
    let mut contradicted_non_findings = std::collections::BTreeSet::new();
    let mut unexpected_blockers = std::collections::BTreeSet::new();
    let mut duplicate_findings = std::collections::BTreeSet::new();
    let mut observed_blockers = 0;
    let mut true_blockers = 0;
    let mut false_blockers = 0;

    for finding in &run.findings {
        if finding.semantic_key.trim().is_empty() {
            return Err("observed finding semantic_key must not be empty".to_string());
        }
        let duplicate = !seen_keys.insert(finding.semantic_key.as_str());
        if duplicate {
            duplicate_findings.insert(finding.semantic_key.clone());
        }
        if !finding.blocking {
            continue;
        }
        observed_blockers += 1;
        let expected = expected_by_key.get(finding.semantic_key.as_str());
        if !duplicate && expected.is_some_and(|expected| expected.expected_blocking) {
            true_blockers += 1;
            if expected.is_some_and(|expected| expected.serious) {
                detected_serious_keys.insert(finding.semantic_key.as_str());
            }
        } else {
            false_blockers += 1;
            if known_non_findings.contains(finding.semantic_key.as_str()) {
                contradicted_non_findings.insert(finding.semantic_key.clone());
            } else if !duplicate {
                unexpected_blockers.insert(finding.semantic_key.clone());
            }
        }
    }

    let expected_serious_keys = expectation
        .expected_findings
        .iter()
        .filter(|expected| expected.serious)
        .map(|expected| expected.semantic_key.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let missed_serious_defects = expected_serious_keys
        .difference(&detected_serious_keys)
        .map(|key| (*key).to_string())
        .collect::<Vec<_>>();

    Ok(BenchmarkRunScore {
        case_id: run.case_id.clone(),
        repetition: run.repetition,
        review_complete: run.review_complete,
        expected_serious_defects: expected_serious_keys.len(),
        detected_serious_defects: detected_serious_keys.len(),
        missed_serious_defects,
        observed_blockers,
        true_blockers,
        false_blockers,
        contradicted_non_findings: contradicted_non_findings.into_iter().collect(),
        unexpected_blockers: unexpected_blockers.into_iter().collect(),
        duplicate_findings: duplicate_findings.into_iter().collect(),
    })
}

fn validate_semantic_expectation(
    label: &str,
    semantic_key: &str,
    adjudicated_claim: &str,
) -> Result<(), String> {
    if semantic_key.trim().is_empty() {
        return Err(format!("{label} semantic_key must not be empty"));
    }
    if adjudicated_claim.trim().is_empty() {
        return Err(format!("{label} adjudicated_claim must not be empty"));
    }
    Ok(())
}

fn ratio(numerator: usize, denominator: usize) -> Option<f64> {
    (denominator != 0).then(|| numerator as f64 / denominator as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> BenchmarkManifest {
        BenchmarkManifest {
            schema_version: BENCHMARK_MANIFEST_SCHEMA_VERSION.to_string(),
            name: "replacement gate".to_string(),
            blinded: true,
            minimum_case_count: 30,
            repetitions: 2,
            thresholds: BenchmarkThresholds {
                minimum_blocking_precision: 0.9,
                minimum_serious_defect_recall: 0.8,
                maximum_false_blockers_per_case: 0.1,
                maximum_contradiction_rate: 0.1,
                minimum_rereview_stability: 0.95,
            },
            sources: vec![BenchmarkSource {
                id: "grounding".to_string(),
                kind: BenchmarkSourceKind::EvidenceGrounding,
                path: "fixtures/evidence-grounding/regressions.json".to_string(),
            }],
        }
    }

    #[test]
    fn validates_a_blinded_versioned_manifest_with_bounded_thresholds() {
        assert_eq!(validate_manifest(&valid_manifest()), Ok(()));
    }

    #[test]
    fn rejects_unblinded_or_under_sized_replacement_corpora() {
        let mut unblinded = valid_manifest();
        unblinded.blinded = false;
        assert_eq!(
            validate_manifest(&unblinded),
            Err("benchmark manifest must declare blinded: true".to_string())
        );

        let mut too_small = valid_manifest();
        too_small.minimum_case_count = 29;
        assert_eq!(
            validate_manifest(&too_small),
            Err("benchmark manifest minimum_case_count must be at least 30".to_string())
        );
    }

    #[test]
    fn rejects_invalid_versions_repetitions_thresholds_and_source_paths() {
        let mut wrong_version = valid_manifest();
        wrong_version.schema_version = "reviewgate-benchmark-manifest/v2".to_string();
        assert!(
            validate_manifest(&wrong_version)
                .expect_err("unknown version")
                .contains("unsupported benchmark manifest schema")
        );

        let mut no_repetitions = valid_manifest();
        no_repetitions.repetitions = 0;
        assert!(
            validate_manifest(&no_repetitions)
                .expect_err("zero repetitions")
                .contains("repetitions")
        );

        let mut bad_threshold = valid_manifest();
        bad_threshold.thresholds.minimum_blocking_precision = 1.1;
        assert!(
            validate_manifest(&bad_threshold)
                .expect_err("out of range threshold")
                .contains("minimum_blocking_precision")
        );

        let mut escaping_path = valid_manifest();
        escaping_path.sources[0].path = "../private.json".to_string();
        assert!(
            validate_manifest(&escaping_path)
                .expect_err("escaping path")
                .contains("repo-relative")
        );
    }

    #[test]
    fn computes_precision_recall_false_blockers_and_contradictions() {
        let observations = [
            CaseObservation {
                expected_blocking: true,
                observed_blocking: true,
            },
            CaseObservation {
                expected_blocking: true,
                observed_blocking: false,
            },
            CaseObservation {
                expected_blocking: false,
                observed_blocking: true,
            },
            CaseObservation {
                expected_blocking: false,
                observed_blocking: false,
            },
        ];

        assert_eq!(
            configuration_metrics(&observations),
            ConfigurationMetrics {
                case_count: 4,
                expected_serious_defects: 2,
                observed_blockers: 2,
                true_positives: 1,
                false_positives: 1,
                false_negatives: 1,
                true_negatives: 1,
                blocking_precision: Some(0.5),
                serious_defect_recall: Some(0.5),
                false_blockers_per_case: 0.25,
                contradiction_rate: Some(0.5),
            }
        );
    }

    #[test]
    fn reports_undefined_ratios_as_null_instead_of_inventing_success() {
        let metrics = configuration_metrics(&[CaseObservation {
            expected_blocking: false,
            observed_blocking: false,
        }]);

        assert_eq!(metrics.blocking_precision, None);
        assert_eq!(metrics.serious_defect_recall, None);
        assert_eq!(metrics.contradiction_rate, Some(0.0));
    }

    fn expectation() -> BenchmarkCaseExpectation {
        BenchmarkCaseExpectation {
            case_id: "case-1".to_string(),
            expected_findings: vec![BenchmarkExpectedFinding {
                semantic_key: "release.repository-context".to_string(),
                adjudicated_claim: "Release commands lack repository context.".to_string(),
                serious: true,
                expected_blocking: true,
            }],
            known_non_findings: vec![BenchmarkKnownNonFinding {
                semantic_key: "upload-artifact.actions-write".to_string(),
                adjudicated_claim: "upload-artifact requires actions:write.".to_string(),
            }],
        }
    }

    #[test]
    fn semantic_key_matching_is_wording_independent_and_duplicates_do_not_inflate_recall() {
        let run = BenchmarkObservedRun {
            case_id: "case-1".to_string(),
            repetition: 1,
            review_complete: true,
            findings: vec![
                BenchmarkObservedFinding {
                    semantic_key: "release.repository-context".to_string(),
                    blocking: true,
                },
                BenchmarkObservedFinding {
                    semantic_key: "release.repository-context".to_string(),
                    blocking: true,
                },
            ],
        };

        let score = score_benchmark_run(&expectation(), &run).expect("run scores");

        assert_eq!(score.detected_serious_defects, 1);
        assert_eq!(score.true_blockers, 1);
        assert_eq!(score.false_blockers, 1);
        assert_eq!(score.duplicate_findings, vec!["release.repository-context"]);
    }

    #[test]
    fn semantic_key_drift_is_both_a_miss_and_an_unexpected_blocker() {
        let run = BenchmarkObservedRun {
            case_id: "case-1".to_string(),
            repetition: 1,
            review_complete: true,
            findings: vec![BenchmarkObservedFinding {
                semantic_key: "release.similar-words".to_string(),
                blocking: true,
            }],
        };

        let score = score_benchmark_run(&expectation(), &run).expect("run scores");

        assert_eq!(
            score.missed_serious_defects,
            vec!["release.repository-context"]
        );
        assert_eq!(score.unexpected_blockers, vec!["release.similar-words"]);
        assert_eq!(score.false_blockers, 1);
    }

    #[test]
    fn known_non_finding_blockers_are_reported_as_contradictions() {
        let run = BenchmarkObservedRun {
            case_id: "case-1".to_string(),
            repetition: 1,
            review_complete: true,
            findings: vec![BenchmarkObservedFinding {
                semantic_key: "upload-artifact.actions-write".to_string(),
                blocking: true,
            }],
        };

        let score = score_benchmark_run(&expectation(), &run).expect("run scores");

        assert_eq!(
            score.contradicted_non_findings,
            vec!["upload-artifact.actions-write"]
        );
        assert_eq!(score.false_blockers, 1);
    }

    #[test]
    fn partial_reviews_remain_in_the_recall_denominator() {
        let run = BenchmarkObservedRun {
            case_id: "case-1".to_string(),
            repetition: 1,
            review_complete: false,
            findings: vec![],
        };

        let score = score_benchmark_run(&expectation(), &run).expect("run scores");

        assert!(!score.review_complete);
        assert_eq!(score.expected_serious_defects, 1);
        assert_eq!(score.detected_serious_defects, 0);
        assert_eq!(
            score.missed_serious_defects,
            vec!["release.repository-context"]
        );
    }

    #[test]
    fn rejects_mismatched_case_ids_and_invalid_expectations() {
        let mut invalid = expectation();
        invalid.expected_findings[0].semantic_key.clear();
        let run = BenchmarkObservedRun {
            case_id: "different-case".to_string(),
            repetition: 1,
            review_complete: true,
            findings: vec![],
        };

        assert!(
            score_benchmark_run(&expectation(), &run)
                .expect_err("mismatched case")
                .contains("case id")
        );
        assert!(
            score_benchmark_run(
                &invalid,
                &BenchmarkObservedRun {
                    case_id: "case-1".to_string(),
                    ..run
                }
            )
            .expect_err("blank semantic key")
            .contains("semantic_key")
        );
    }
}
