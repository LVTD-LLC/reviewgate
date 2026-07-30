use std::path::PathBuf;
use std::process::Command;

#[test]
fn deterministic_replay_is_networkless_and_byte_stable() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo = crate_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root");
    let output_dir =
        std::env::temp_dir().join(format!("reviewgate-eval-replays-{}", std::process::id()));
    std::fs::create_dir_all(&output_dir).expect("create output directory");
    let first = output_dir.join("first.json");
    let second = output_dir.join("second.json");

    for output in [&first, &second] {
        let result = Command::new(env!("CARGO_BIN_EXE_reviewgate"))
            .current_dir(repo)
            .env_remove("OPENROUTER_API_KEY")
            .args([
                "eval-replays",
                "--manifest",
                "fixtures/evaluation/manifest-v1.json",
                "--openrouter-base-url",
                "http://127.0.0.1:1",
                "--json-out",
            ])
            .arg(output)
            .output()
            .expect("run deterministic replay");

        assert!(
            result.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            result.stdout.is_empty(),
            "explicit output path keeps stdout clean"
        );
    }

    assert_eq!(
        std::fs::read(&first).expect("read first report"),
        std::fs::read(&second).expect("read second report"),
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&first).expect("read report")).expect("valid JSON");
    assert_eq!(report["schema_version"], "reviewgate-benchmark-report/v1");
    assert_eq!(report["mode"], "deterministic");
    assert_eq!(report["passed"], true);
    assert!(
        report["configurations"]
            .as_array()
            .is_some_and(|configurations| {
                configurations.iter().all(|configuration| {
                    configuration["cases"].as_array().is_some_and(|cases| {
                        cases.iter().all(|case| {
                            case["provenance"].is_null() || case["provenance"].is_object()
                        })
                    })
                })
            })
    );

    let manifest_schema: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo.join("schemas/reviewgate-benchmark-manifest-v1.schema.json"))
            .expect("read manifest schema"),
    )
    .expect("manifest schema JSON");
    assert_eq!(
        manifest_schema["properties"]["configurations"]["maxItems"],
        2
    );
    assert_eq!(
        manifest_schema["properties"]["configurations"]["allOf"][0]["contains"]["properties"]["role"]
            ["const"],
        "baseline"
    );
    assert_eq!(
        manifest_schema["properties"]["configurations"]["allOf"][1]["contains"]["properties"]["role"]
            ["const"],
        "candidate"
    );
    assert!(
        manifest_schema["$defs"]["thresholds"]["required"]
            .as_array()
            .is_some_and(|required| required
                .iter()
                .any(|field| field == "minimum_completion_rate"))
    );

    std::fs::remove_dir_all(output_dir).ok();
}

#[test]
fn live_replay_requires_a_key_before_network_or_output() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo = crate_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root");
    let output = std::env::temp_dir().join(format!(
        "reviewgate-live-replay-{}.json",
        std::process::id()
    ));
    let result = Command::new(env!("CARGO_BIN_EXE_reviewgate"))
        .current_dir(repo)
        .env_remove("OPENROUTER_API_KEY")
        .args([
            "eval-replays",
            "--live",
            "--max-cases",
            "1",
            "--manifest",
            "fixtures/evaluation/manifest-v1.json",
            "--openrouter-base-url",
            "http://127.0.0.1:1",
            "--json-out",
        ])
        .arg(&output)
        .output()
        .expect("run live replay without key");

    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("OPENROUTER_API_KEY"),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!output.exists());
}
