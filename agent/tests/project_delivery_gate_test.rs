use kn_agent::project_delivery::ProjectOperationGate;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn same_project_operations_serialize_without_blocking_other_projects() {
    let gate = Arc::new(ProjectOperationGate::default());
    let first = gate.lock("42:/repo-a").await;

    let (same_project_entered_tx, same_project_entered_rx) = oneshot::channel();
    let same_project_gate = gate.clone();
    let same_project = tokio::spawn(async move {
        let _operation = same_project_gate.lock("42:/repo-a").await;
        let _ = same_project_entered_tx.send(());
    });

    assert!(
        timeout(Duration::from_millis(50), same_project_entered_rx)
            .await
            .is_err(),
        "a second operation on the same project must wait for the first"
    );

    let (other_project_entered_tx, other_project_entered_rx) = oneshot::channel();
    let other_project_gate = gate.clone();
    let other_project = tokio::spawn(async move {
        let _operation = other_project_gate.lock("42:/repo-b").await;
        let _ = other_project_entered_tx.send(());
    });

    timeout(Duration::from_millis(50), other_project_entered_rx)
        .await
        .expect("another project must not wait")
        .expect("other project task should signal");

    drop(first);
    timeout(Duration::from_secs(1), same_project)
        .await
        .expect("same project task should finish after release")
        .expect("same project task should not panic");
    other_project
        .await
        .expect("other project task should not panic");
}

#[tokio::test]
async fn operations_across_projects_respect_the_global_concurrency_limit() {
    let gate = Arc::new(ProjectOperationGate::with_limit(2));
    let (entered_tx, mut entered_rx) = mpsc::unbounded_channel();
    let (release_one_tx, release_one_rx) = oneshot::channel();
    let (release_two_tx, release_two_rx) = oneshot::channel();
    let (release_three_tx, release_three_rx) = oneshot::channel();

    let first = spawn_held_operation(
        gate.clone(),
        "42:/repo-a",
        entered_tx.clone(),
        release_one_rx,
    );
    let second = spawn_held_operation(
        gate.clone(),
        "42:/repo-b",
        entered_tx.clone(),
        release_two_rx,
    );
    let third = spawn_held_operation(gate, "42:/repo-c", entered_tx, release_three_rx);

    timeout(Duration::from_secs(1), entered_rx.recv())
        .await
        .expect("first operation should enter")
        .expect("entry channel should stay open");
    timeout(Duration::from_secs(1), entered_rx.recv())
        .await
        .expect("second operation should enter")
        .expect("entry channel should stay open");
    assert!(
        timeout(Duration::from_millis(50), entered_rx.recv())
            .await
            .is_err(),
        "a third project must wait for the global delivery limit"
    );

    release_one_tx.send(()).expect("release first operation");
    timeout(Duration::from_secs(1), entered_rx.recv())
        .await
        .expect("third operation should enter after a permit is released")
        .expect("entry channel should stay open");

    release_two_tx.send(()).expect("release second operation");
    release_three_tx.send(()).expect("release third operation");
    first.await.expect("first task should not panic");
    second.await.expect("second task should not panic");
    third.await.expect("third task should not panic");
}

fn spawn_held_operation(
    gate: Arc<ProjectOperationGate>,
    project_key: &'static str,
    entered: mpsc::UnboundedSender<()>,
    release: oneshot::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let _operation = gate.lock(project_key).await;
        let _ = entered.send(());
        let _ = release.await;
    })
}
