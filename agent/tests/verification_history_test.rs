use kn_agent::session::verification_history::{LastVerification, VerificationHistory};
use std::time::{Duration, SystemTime};

fn record(state: &str, finished_at: Option<SystemTime>) -> LastVerification {
    LastVerification {
        run_id: "v_test".to_string(),
        state: state.to_string(),
        started_at_ms: 1_000,
        finished_at_ms: finished_at.and_then(|time| {
            time.duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_millis() as u64)
        }),
        duration_ms: 100,
        target: "all".to_string(),
        environment: "default".to_string(),
        command_source: "auto".to_string(),
        build_state: Some("passed".to_string()),
        test_state: Some("failed".to_string()),
        log_available: true,
        is_running: finished_at.is_none(),
    }
}

#[test]
fn persists_and_loads_the_latest_verification_summary() {
    let temp = tempfile::tempdir().expect("temporary history root");
    let history = VerificationHistory::at(temp.path());
    let now = SystemTime::now();
    let expected = record("testFailed", Some(now));

    history
        .save("17:/workspace/kn", &expected)
        .expect("save summary");

    assert_eq!(history.load("17:/workspace/kn", now), Some(expected));
}

#[test]
fn recovers_an_unfinished_record_as_interrupted() {
    let temp = tempfile::tempdir().expect("temporary history root");
    let history = VerificationHistory::at(temp.path());
    let now = SystemTime::now();
    let mut running = record("runningBuild", None);
    running.started_at_ms = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("current epoch")
        .as_millis() as u64;
    history
        .save("17:/workspace/kn", &running)
        .expect("save running summary");

    let loaded = history
        .load("17:/workspace/kn", now)
        .expect("recover summary");

    assert_eq!(loaded.state, "interrupted");
    assert!(!loaded.is_running);
    assert!(loaded.finished_at_ms.is_some());
}

#[test]
fn ignores_summaries_older_than_seven_days() {
    let temp = tempfile::tempdir().expect("temporary history root");
    let history = VerificationHistory::at(temp.path());
    let now = SystemTime::now();
    let old = now - Duration::from_secs(8 * 24 * 60 * 60);
    history
        .save("17:/workspace/kn", &record("passed", Some(old)))
        .expect("save old summary");

    assert_eq!(history.load("17:/workspace/kn", now), None);
}
