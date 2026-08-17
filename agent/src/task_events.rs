use crate::proto::{TaskCompleteEvent, WsMessageBuilder};
use crate::session::{SessionManager, SessionStatus};
use std::io::{BufRead, Seek, SeekFrom, Write};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::mpsc;

pub async fn flush_task_complete_queue(
    sessions: Arc<SessionManager>,
    outgoing: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
) {
    let queue = kn_common::path::config_dir()
        .join("events")
        .join("task-complete.jsonl");
    let offset_path = queue.with_extension("jsonl.offset");
    let mut offset = std::fs::read_to_string(&offset_path)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0);

    let mut file = match std::fs::File::open(&queue) {
        Ok(file) => file,
        Err(_) => return,
    };
    if file
        .metadata()
        .map(|meta| offset > meta.len())
        .unwrap_or(false)
    {
        offset = 0;
    }
    if file.seek(SeekFrom::Start(offset)).is_err() {
        offset = 0;
        let _ = file.seek(SeekFrom::Start(0));
    }

    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    loop {
        line.clear();
        let read = match reader.read_line(&mut line) {
            Ok(read) => read,
            Err(_) => break,
        };
        if read == 0 {
            break;
        }
        let next_offset = offset.saturating_add(read as u64);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            offset = next_offset;
            continue;
        }
        let event = match serde_json::from_str::<TaskCompleteEvent>(trimmed) {
            Ok(event) => event,
            Err(_) => {
                offset = next_offset;
                continue;
            }
        };

        match send_if_remote(&sessions, &outgoing, event).await {
            QueueSend::Skip => offset = next_offset,
            QueueSend::Sent | QueueSend::RetryLater => break,
        }
    }

    let _ = std::fs::write(&offset_path, offset.to_string());
    compact_queue_if_consumed(&queue, &offset_path, offset);
}

pub fn acknowledge_task_complete_event(event_id: &str) {
    let event_id = event_id.trim();
    if event_id.is_empty() {
        return;
    }
    let queue = kn_common::path::config_dir()
        .join("events")
        .join("task-complete.jsonl");
    let offset_path = queue.with_extension("jsonl.offset");
    let mut offset = std::fs::read_to_string(&offset_path)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0);

    let mut file = match std::fs::File::open(&queue) {
        Ok(file) => file,
        Err(_) => return,
    };
    if file
        .metadata()
        .map(|meta| offset > meta.len())
        .unwrap_or(false)
    {
        offset = 0;
    }
    if file.seek(SeekFrom::Start(offset)).is_err() {
        offset = 0;
        let _ = file.seek(SeekFrom::Start(0));
    }

    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    loop {
        line.clear();
        let read = match reader.read_line(&mut line) {
            Ok(read) => read,
            Err(_) => return,
        };
        if read == 0 {
            return;
        }
        let next_offset = offset.saturating_add(read as u64);
        let trimmed = line.trim();
        if trimmed.is_empty() || serde_json::from_str::<TaskCompleteEvent>(trimmed).is_err() {
            offset = next_offset;
            let _ = std::fs::write(&offset_path, offset.to_string());
            continue;
        }
        let matched = serde_json::from_str::<TaskCompleteEvent>(trimmed)
            .map(|event| event.event_id == event_id)
            .unwrap_or(false);
        if !matched {
            return;
        }
        offset = next_offset;
        let _ = std::fs::write(&offset_path, offset.to_string());
        compact_queue_if_consumed(&queue, &offset_path, offset);
        return;
    }
}

enum QueueSend {
    Sent,
    Skip,
    RetryLater,
}

async fn send_if_remote(
    sessions: &Arc<SessionManager>,
    outgoing: &Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    mut event: TaskCompleteEvent,
) -> QueueSend {
    let session = if !event.session_id.trim().is_empty() {
        sessions.get(event.session_id.trim()).await.ok().flatten()
    } else {
        find_matching_remote_session(sessions, &event).await
    };
    let Some(session) = session else {
        return QueueSend::Skip;
    };
    if session.status == SessionStatus::Ended || !session.remote_enabled.load(Ordering::Relaxed) {
        return QueueSend::Skip;
    }

    event.session_id = session.nid.clone();
    if event.tool.trim().is_empty() {
        event.tool = session.tool;
    }
    if event
        .profile
        .as_ref()
        .is_none_or(|value| value.trim().is_empty())
    {
        event.profile = session.profile;
    }
    if event.project_path.trim().is_empty() {
        event.project_path = session.cwd;
    }

    let Some(tx) = outgoing.lock().await.as_ref().cloned() else {
        return QueueSend::RetryLater;
    };
    match tx.send(WsMessageBuilder::task_completed(&event)) {
        Ok(()) => QueueSend::Sent,
        Err(_) => QueueSend::RetryLater,
    }
}

fn compact_queue_if_consumed(queue: &std::path::Path, offset_path: &std::path::Path, offset: u64) {
    let Ok(meta) = std::fs::metadata(queue) else {
        return;
    };
    if offset == 0 || offset < meta.len() || meta.len() < 1024 * 1024 {
        return;
    }
    let tmp = queue.with_extension("jsonl.tmp");
    if std::fs::File::create(&tmp)
        .and_then(|mut file| file.flush())
        .is_err()
    {
        return;
    }
    if std::fs::rename(&tmp, queue).is_ok() {
        let _ = std::fs::write(offset_path, "0");
    }
}

async fn find_matching_remote_session(
    sessions: &Arc<SessionManager>,
    event: &TaskCompleteEvent,
) -> Option<crate::session::ManagedSession> {
    let summaries = sessions.list().await.ok()?;
    let profile = event.profile.as_deref().unwrap_or("").trim();
    let tool = event.tool.trim();
    let project_path = event.project_path.trim();
    let summary = summaries.into_iter().find(|session| {
        session.status != SessionStatus::Ended
            && session.remote_enabled
            && (tool.is_empty() || session.tool == tool)
            && (profile.is_empty() || session.profile.as_deref() == Some(profile))
            && (project_path.is_empty() || session.cwd == project_path)
    })?;
    sessions.get(&summary.nid).await.ok().flatten()
}
