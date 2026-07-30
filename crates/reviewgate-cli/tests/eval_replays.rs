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

    std::fs::remove_dir_all(output_dir).ok();
}
