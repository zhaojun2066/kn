use crate::proto::WsMessageBuilder;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing;

/// PTY 输出扇出到 WSS + IPC，带 100ms/64KB 批处理和 10KB 分块。
///
/// `broadcast()` 由 PTY reader 在 `spawn_blocking` 上下文中调用，
/// buffer 使用 `std::sync::Mutex`（锁持有时间极短）。
#[derive(Clone)]
pub struct OutputFanout {
    pub(crate) inner: Arc<OutputFanoutInner>,
    cancel: tokio_util::sync::CancellationToken,
}

pub(crate) struct OutputFanoutInner {
    pub(crate) wss_tx: Option<mpsc::UnboundedSender<String>>,
    ipc_tx: Option<mpsc::UnboundedSender<String>>,
    session_nid: String,
    buffer: std::sync::Mutex<Vec<u8>>,
    /// 额外的 output subscriber（供 attach_pty 注册）
    extra_subscribers: std::sync::Mutex<Vec<mpsc::UnboundedSender<Vec<u8>>>>,
    /// 活跃会话的完整原始输出日志（~/.kn/agent/sessions/{nid}/output.log）。
    /// 会话结束后由 SessionManager 删除；运行中不截断，确保新终端能从
    /// 会话起点重放到当前屏幕状态。
    log_path: PathBuf,
    /// 日志当前大小（避免每次 fstat）
    log_size: std::sync::atomic::AtomicU64,
    stream: Arc<OutputStream>,
    /// 远程控制开关（共享自 ManagedSession.remote_enabled），None 视为开启
    remote_enabled: Option<Arc<std::sync::atomic::AtomicBool>>,
}

/// The single owner of one session's remote output stream.  Its mutex spans
/// durable live-frame sequencing and replay snapshot enqueueing, so an Agent
/// cannot interleave a live frame into one replay transaction.
struct OutputStream {
    cursor: Mutex<u64>,
    sequence_path: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReplayLogResult {
    pub status: &'static str,
    pub data: Vec<u8>,
    pub bytes: usize,
    pub message: Option<String>,
}

/// 全局日志大小跟踪表（供 relay 模式的 `append_log_static` 使用）。
/// key = session nid, value = 该 session 日志的当前字节数。
static STATIC_LOG_SIZES: std::sync::LazyLock<std::sync::Mutex<HashMap<String, Arc<AtomicU64>>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// 全局日志文件写入锁表，防止并发 append 与结束清理导致的数据丢失。
/// key = 日志文件规范路径, value = Mutex<()>。
/// `append_log` 和 `remove_replay_log` 通过此锁串行化，避免两个并发上下文
/// （PTY reader / relay output 与结束清理）竞争同一文件。
static LOG_FILE_LOCKS: std::sync::LazyLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

static OUTPUT_STREAMS: std::sync::LazyLock<Mutex<HashMap<PathBuf, Arc<OutputStream>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

fn sequence_path(log_path: &PathBuf) -> PathBuf {
    log_path.with_file_name("output.seq")
}

fn restore_sequence(sequence_path: &PathBuf) -> u64 {
    std::fs::read_to_string(sequence_path)
        .ok()
        .and_then(|content| {
            content
                .lines()
                .rev()
                .find_map(|line| line.parse::<u64>().ok())
        })
        .unwrap_or(0)
}

fn get_output_stream(log_path: &PathBuf) -> Arc<OutputStream> {
    let canonical = std::fs::canonicalize(log_path).unwrap_or_else(|_| log_path.clone());
    let mut streams = OUTPUT_STREAMS.lock().unwrap();
    streams
        .entry(canonical)
        .or_insert_with(|| {
            let sequence_path = sequence_path(log_path);
            Arc::new(OutputStream {
                cursor: Mutex::new(restore_sequence(&sequence_path)),
                sequence_path,
            })
        })
        .clone()
}

fn remove_output_stream(log_path: &PathBuf) {
    let canonical = std::fs::canonicalize(log_path).unwrap_or_else(|_| log_path.clone());
    OUTPUT_STREAMS.lock().unwrap().remove(&canonical);
}

fn persist_next_sequence(stream: &OutputStream, cursor: &mut u64) -> std::io::Result<u64> {
    let next = cursor
        .checked_add(1)
        .ok_or_else(|| std::io::Error::other("output sequence exhausted"))?;
    if let Some(parent) = stream.sequence_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    use std::io::Write;
    let mut journal = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&stream.sequence_path)?;
    writeln!(journal, "{next}")?;
    journal.sync_data()?;
    *cursor = next;
    Ok(next)
}

/// 获取或创建指定路径的日志写入锁。
fn get_log_lock(path: &PathBuf) -> Arc<Mutex<()>> {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
    let mut map = LOG_FILE_LOCKS.lock().unwrap();
    map.entry(canonical)
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

/// Session 结束时释放对应日志文件的锁条目，防止长期运行内存泄漏。
pub(crate) fn remove_log_lock(nid: &str) {
    let log_path = kn_common::path::agent_dir()
        .join("sessions")
        .join(nid)
        .join("output.log");
    let canonical = std::fs::canonicalize(&log_path).unwrap_or(log_path);
    let mut map = LOG_FILE_LOCKS.lock().unwrap();
    map.remove(&canonical);
}

/// Deletes a complete replay log only after its PTY has stopped producing
/// output. Active sessions retain their entire stream for iOS recovery.
pub(crate) fn remove_replay_log(nid: &str) {
    let log_path = kn_common::path::agent_dir()
        .join("sessions")
        .join(nid)
        .join("output.log");
    let stream = get_output_stream(&log_path);
    let _stream_guard = stream.cursor.lock().unwrap();
    let lock = get_log_lock(&log_path);
    {
        let _guard = lock.lock().unwrap();
        if let Err(error) = std::fs::remove_file(&log_path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(nid = %nid, path = %log_path.display(), error = %error, "删除已结束会话回放日志失败");
            }
        }
    }
    if let Err(error) = std::fs::remove_file(&stream.sequence_path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(nid = %nid, path = %stream.sequence_path.display(), error = %error, "删除输出序号日志失败");
        }
    }
    remove_log_lock(nid);
    remove_output_stream(&log_path);
    STATIC_LOG_SIZES.lock().unwrap().remove(nid);
}

/// 获取或初始化指定 nid 的日志大小 AtomicU64。供 `append_log_static` 复用。
pub(crate) fn get_static_log_size(nid: &str) -> Arc<AtomicU64> {
    let mut map = STATIC_LOG_SIZES.lock().unwrap();
    map.entry(nid.to_string())
        .or_insert_with(|| {
            let log_path = kn_common::path::agent_dir()
                .join("sessions")
                .join(nid)
                .join("output.log");
            let initial = std::fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0);
            Arc::new(AtomicU64::new(initial))
        })
        .clone()
}

/// Codex wraps atomic screen updates with this marker. It remains useful when
/// reading legacy, previously-truncated logs during an upgrade.
const SYNCHRONIZED_OUTPUT_START: &[u8] = b"\x1b[?2026h";

impl OutputFanout {
    /// 创建 OutputFanout 并启动 100ms 定时 flush 任务。
    /// `cancel` 用于停止定时器（session 结束时触发）。
    ///
    /// `session_nid` 是会话唯一标识，对齐新协议 `sessionId` 类型。
    pub fn new(
        session_nid: String,
        wss: Option<mpsc::UnboundedSender<String>>,
        ipc: Option<mpsc::UnboundedSender<String>>,
        cancel: tokio_util::sync::CancellationToken,
        remote_enabled: Option<Arc<std::sync::atomic::AtomicBool>>,
    ) -> Self {
        let log_path = kn_common::path::agent_dir()
            .join("sessions")
            .join(&session_nid)
            .join("output.log");
        let log_size = std::sync::atomic::AtomicU64::new(
            std::fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0),
        );

        let stream = get_output_stream(&log_path);
        let inner = Arc::new(OutputFanoutInner {
            wss_tx: wss,
            ipc_tx: ipc,
            session_nid,
            buffer: std::sync::Mutex::new(Vec::new()),
            extra_subscribers: std::sync::Mutex::new(Vec::new()),
            log_path,
            log_size,
            stream,
            remote_enabled,
        });

        let inner_clone = inner.clone();
        let timer_cancel = cancel.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        let (data, subscribers) = {
                            let mut buf = inner_clone.buffer.lock().unwrap_or_else(|e| e.into_inner());
                            if buf.is_empty() {
                                (Vec::new(), Vec::new())
                            } else {
                                let data = std::mem::take(&mut *buf);
                                // Clone subscriber list for flushing outside the lock
                                let subs = inner_clone.extra_subscribers.lock().unwrap_or_else(|e| e.into_inner()).clone();
                                (data, subs)
                            }
                        };
                        if !data.is_empty() {
                            let len = data.len();
                            tracing::info!(len = len, nid = %inner_clone.session_nid, "⏱️  [FLUSH] 100ms 定时器触发 flush");
                            // Send to extra subscribers first (raw bytes, before data is moved)
                            for tx in &subscribers {
                                let _ = tx.send(data.clone());
                            }
                            Self::flush_chunked(
                                inner_clone.session_nid.clone(),
                                data,
                                inner_clone.wss_tx.clone(),
                                inner_clone.ipc_tx.clone(),
                                inner_clone.log_path.clone(),
                                &inner_clone.log_size,
                                inner_clone.stream.clone(),
                                inner_clone.remote_enabled.clone(),
                            );
                        }
                    }
                    _ = timer_cancel.cancelled() => break,
                }
            }
        });

        OutputFanout { inner, cancel }
    }

    /// 注册额外的 output subscriber（供 attach_pty 使用）。
    /// 返回 receiver，调用方应持续读取并转发到客户端。
    pub fn register_subscriber(&self) -> mpsc::UnboundedReceiver<Vec<u8>> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.inner
            .extra_subscribers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(tx);
        rx
    }

    /// 返回 session 的取消令牌（用于停止 stdin writer 等）。
    pub fn cancel_token(&self) -> tokio_util::sync::CancellationToken {
        self.cancel.clone()
    }

    /// PTY reader 调用此方法追加输出数据。
    ///
    /// 来自 `spawn_blocking` 上下文（同步），使用 `std::sync::Mutex`。
    /// 缓冲区达到 64KB 时立即 flush，否则等待 100ms 定时器。
    pub fn broadcast(&self, data: &[u8]) {
        let len = data.len();
        let mut buf = self.inner.buffer.lock().unwrap_or_else(|e| e.into_inner());
        buf.extend_from_slice(data);
        let buf_len = buf.len();
        tracing::debug!(
            received = len,
            buffered = buf_len,
            "📟 [PTY-OUT] 收到 PTY 输出"
        );
        if buf_len >= 64 * 1024 {
            let data = std::mem::take(&mut *buf);
            drop(buf); // 释放锁后再 flush
            let inner = self.inner.clone();
            tracing::info!(len = data.len(), nid = %inner.session_nid, "📟 [PTY-OUT] 达到 64KB 阈值, 异步 flush");
            // spawn 到 tokio 异步线程，避免在 PTY reader（spawn_blocking）中同步写环
            // 形日志 + 分块发送 WSS/IPC，阻塞 PTY 读取。
            tokio::spawn(async move {
                Self::flush_chunked(
                    inner.session_nid.clone(),
                    data,
                    inner.wss_tx.clone(),
                    inner.ipc_tx.clone(),
                    inner.log_path.clone(),
                    &inner.log_size,
                    inner.stream.clone(),
                    inner.remote_enabled.clone(),
                );
            });
        }
    }

    /// Flushes output which has not yet reached the periodic flush timer. This
    /// is required before a terminal is cancelled so a CLI's final error is not
    /// lost when it exits immediately.
    pub fn flush_pending(&self) {
        let (data, subscribers) = {
            let mut buffer = self.inner.buffer.lock().unwrap_or_else(|e| e.into_inner());
            if buffer.is_empty() {
                return;
            }
            let data = std::mem::take(&mut *buffer);
            let subscribers = self
                .inner
                .extra_subscribers
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            (data, subscribers)
        };
        for subscriber in subscribers {
            let _ = subscriber.send(data.clone());
        }
        Self::flush_chunked(
            self.inner.session_nid.clone(),
            data,
            self.inner.wss_tx.clone(),
            self.inner.ipc_tx.clone(),
            self.inner.log_path.clone(),
            &self.inner.log_size,
            self.inner.stream.clone(),
            self.inner.remote_enabled.clone(),
        );
    }

    /// 将数据按 10KB 分块，分别发送到 WSS 和 IPC 通道，同时写入环形日志。
    /// `remote_enabled` 为 Some(false) 时跳过 WSS 发送（但仍写 ring log）。
    fn flush_chunked(
        session_nid: String,
        data: Vec<u8>,
        wss_tx: Option<mpsc::UnboundedSender<String>>,
        ipc_tx: Option<mpsc::UnboundedSender<String>>,
        log_path: PathBuf,
        log_size: &std::sync::atomic::AtomicU64,
        stream: Arc<OutputStream>,
        remote_enabled: Option<Arc<std::sync::atomic::AtomicBool>>,
    ) {
        const CHUNK_SIZE: usize = 10 * 1024; // 10KB
        let total = data.len();
        let chunks = data.chunks(CHUNK_SIZE).count();
        let preview = String::from_utf8_lossy(if data.len() <= 200 {
            &data
        } else {
            &data[..200]
        });
        let wss_blocked = remote_enabled
            .as_ref()
            .map(|f| !f.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(false);
        tracing::info!(
            nid = %session_nid,
            total_len = total,
            chunks = chunks,
            wss_blocked = wss_blocked,
            preview = %preview.trim_end(),
            "📤 [FLUSH] 开始分块发送输出"
        );

        let mut cursor = stream
            .cursor
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        // Persist the raw snapshot before its sequence journal. A crash in this
        // interval may leave bytes only in the next replay snapshot, never a
        // reused sequence for an already-emitted live frame. If this fails, do
        // not expose a live frame: its sequence would otherwise be outside the
        // next snapshot and violate the replay fence. Local IPC does not take
        // part in remote replay, so keep the desktop-local fan-out available.
        if let Err(error) = Self::append_log(&log_path, &data, log_size) {
            tracing::error!(nid = %session_nid, error = %error, "输出日志持久化失败，拒绝发送未被快照覆盖的 live output");
            if let Some(ref tx) = ipc_tx {
                for chunk in data.chunks(CHUNK_SIZE) {
                    let _ = tx.send(String::from_utf8_lossy(chunk).to_string());
                }
            }
            return;
        }

        for (i, chunk) in data.chunks(CHUNK_SIZE).enumerate() {
            let text = String::from_utf8_lossy(chunk);
            if !wss_blocked {
                if let Some(ref tx) = wss_tx {
                    let sequence = match persist_next_sequence(&stream, &mut cursor) {
                        Ok(sequence) => sequence,
                        Err(error) => {
                            tracing::error!(nid = %session_nid, error = %error, "输出序号持久化失败，拒绝发送无序 live output");
                            continue;
                        }
                    };
                    let msg = WsMessageBuilder::output_live(&session_nid, &text, sequence);
                    match tx.send(msg) {
                        Ok(_) => tracing::info!(
                            chunk = i,
                            len = chunk.len(),
                            "📤 [FLUSH] chunk 已发送到 wss_tx"
                        ),
                        Err(e) => {
                            tracing::error!(chunk = i, error = %e, "📤 [FLUSH] chunk 发送到 wss_tx 失败")
                        }
                    }
                } else {
                    tracing::warn!(chunk = i, "📤 [FLUSH] wss_tx 为 None, 跳过");
                }
            }
            if let Some(ref tx) = ipc_tx {
                let _ = tx.send(text.to_string());
            }
        }
    }

    /// 追加写入活跃会话的完整日志。日志在会话实际结束时删除；不能在运行中
    /// 截断，否则全屏 CLI 的后续增量帧无法重建一个新终端的屏幕基线。
    ///
    /// 使用 per-file Mutex 防止两个并发上下文（spawn_blocking PTY reader +
    /// 100ms timer flush）的 write-all → trim 序列互相穿插导致数据丢失。
    fn append_log(
        path: &PathBuf,
        data: &[u8],
        log_size: &std::sync::atomic::AtomicU64,
    ) -> std::io::Result<()> {
        let lock = get_log_lock(path);
        let _guard = lock.lock().unwrap();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        use std::io::Write;
        file.write_all(data)?;
        // A remote live frame may only be emitted once a subsequent replay can
        // reconstruct it after a crash, not merely after it reached a process
        // buffer.
        file.sync_data()?;
        log_size.fetch_add(data.len() as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Relay-mode live output uses the same durable sequence owner as a native PTY.
    pub fn emit_static_live(nid: &str, data: &str, wss_tx: &mpsc::UnboundedSender<String>) {
        let log_path = kn_common::path::agent_dir()
            .join("sessions")
            .join(nid)
            .join("output.log");
        let log_size = get_static_log_size(nid);
        let stream = get_output_stream(&log_path);
        let mut cursor = stream
            .cursor
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Err(error) = Self::append_log(&log_path, data.as_bytes(), &*log_size) {
            tracing::error!(nid = %nid, error = %error, "Relay 输出日志持久化失败，拒绝发送未被快照覆盖的 live output");
            return;
        }
        match persist_next_sequence(&stream, &mut cursor) {
            Ok(sequence) => {
                if let Err(error) = wss_tx.send(WsMessageBuilder::output_live(nid, data, sequence))
                {
                    tracing::warn!(nid = %nid, error = %error, "Relay output 发送到 WSS 失败");
                }
            }
            Err(error) => {
                tracing::error!(nid = %nid, error = %error, "Relay output 序号持久化失败，拒绝发送无序 live output")
            }
        }
    }

    /// Captures and enqueues one complete replay while holding the session's
    /// stream fence. The caller must pass a cloned outgoing sender, so this
    /// synchronous critical section never awaits a Tokio mutex.
    pub fn replay_to_wss(nid: &str, replay_id: &str, wss_tx: &mpsc::UnboundedSender<String>) {
        let log_path = kn_common::path::agent_dir()
            .join("sessions")
            .join(nid)
            .join("output.log");
        let stream = get_output_stream(&log_path);
        Self::replay_to_wss_at_path(nid, replay_id, &log_path, stream, wss_tx);
    }

    fn replay_to_wss_at_path(
        nid: &str,
        replay_id: &str,
        log_path: &PathBuf,
        stream: Arc<OutputStream>,
        wss_tx: &mpsc::UnboundedSender<String>,
    ) {
        let cursor = stream
            .cursor
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let replay = Self::replay_log_result_at_path(log_path);
        match replay.status {
            "ok" => {
                const CHUNK_SIZE: usize = 32 * 1024;
                let ansi_text = String::from_utf8_lossy(&replay.data).into_owned();
                let mut offset = 0;
                let mut chunks = 0usize;
                while offset < ansi_text.len() {
                    let end = std::cmp::min(offset + CHUNK_SIZE, ansi_text.len());
                    let mut chunk_end = end;
                    while chunk_end > offset && !ansi_text.is_char_boundary(chunk_end) {
                        chunk_end -= 1;
                    }
                    if wss_tx
                        .send(WsMessageBuilder::output_replay(
                            nid,
                            &ansi_text[offset..chunk_end],
                            replay_id,
                        ))
                        .is_err()
                    {
                        return;
                    }
                    chunks += 1;
                    offset = chunk_end;
                }
                let _ = wss_tx.send(WsMessageBuilder::replay_output_done(
                    nid,
                    replay_id,
                    "ok",
                    replay.bytes,
                    chunks,
                    None,
                    Some(*cursor),
                ));
            }
            "empty" => {
                let _ = wss_tx.send(WsMessageBuilder::replay_output_done(
                    nid,
                    replay_id,
                    "empty",
                    0,
                    0,
                    None,
                    Some(*cursor),
                ));
            }
            _ => {
                let _ = wss_tx.send(WsMessageBuilder::replay_output_done(
                    nid,
                    replay_id,
                    "error",
                    0,
                    0,
                    replay.message.as_deref(),
                    None,
                ));
            }
        }
    }

    /// 读取活动会话的完整日志，用于恢复时回放。
    pub fn replay_log(nid: &str) -> Option<Vec<u8>> {
        let path = kn_common::path::agent_dir()
            .join("sessions")
            .join(nid)
            .join("output.log");
        std::fs::read(&path).ok().filter(|d| !d.is_empty())
    }

    pub fn replay_log_result(nid: &str) -> ReplayLogResult {
        let path = kn_common::path::agent_dir()
            .join("sessions")
            .join(nid)
            .join("output.log");
        Self::replay_log_result_at_path(&path)
    }

    fn replay_log_result_at_path(path: &PathBuf) -> ReplayLogResult {
        if path.is_dir() {
            return ReplayLogResult {
                status: "error",
                data: Vec::new(),
                bytes: 0,
                message: Some("output log path is a directory".to_string()),
            };
        }
        match std::fs::read(&path) {
            Ok(data) if data.is_empty() => ReplayLogResult {
                status: "empty",
                data,
                bytes: 0,
                message: None,
            },
            Ok(data) => {
                let bytes = data.len();
                ReplayLogResult {
                    status: "ok",
                    data,
                    bytes,
                    message: None,
                }
            }
            Err(_) if !path.exists() => ReplayLogResult {
                status: "empty",
                data: Vec::new(),
                bytes: 0,
                message: None,
            },
            Err(error) => ReplayLogResult {
                status: "error",
                data: Vec::new(),
                bytes: 0,
                message: Some(error.to_string()),
            },
        }
    }

    /// Chooses a safe replay boundary in a legacy raw PTY ring buffer. Prefer
    /// the first complete synchronized-output frame after the nominal tail
    /// start; otherwise skip only a CSI command proven to cross the boundary.
    fn replay_safe_tail(data: &[u8], keep: usize) -> &[u8] {
        let start = data.len().saturating_sub(keep);
        let tail = &data[start..];
        let safe_offset = tail
            .windows(SYNCHRONIZED_OUTPUT_START.len())
            .position(|window| window == SYNCHRONIZED_OUTPUT_START)
            .or_else(|| Self::csi_sequence_crossing(data, start).map(|end| end - start))
            .unwrap_or(0);
        let safe_start = Self::skip_utf8_continuation_bytes(data, start + safe_offset);
        &data[safe_start..]
    }

    /// A byte-based ring boundary can cut through a multibyte UTF-8 scalar.
    /// Drop only its orphaned continuation bytes; the rest of the terminal
    /// stream remains byte-for-byte intact and can be replayed without `�`.
    fn skip_utf8_continuation_bytes(data: &[u8], start: usize) -> usize {
        let mut index = start;
        while index < data.len() && (0x80..=0xbf).contains(&data[index]) {
            index += 1;
        }
        index
    }

    /// Detects a CSI sequence which began shortly before the nominal tail and
    /// ends at or after it. This also handles a tail that starts at the final
    /// byte (for example `m` from `ESC[31m`).
    fn csi_sequence_crossing(data: &[u8], start: usize) -> Option<usize> {
        const LOOKBACK: usize = 256;
        let lower_bound = start.saturating_sub(LOOKBACK);
        for candidate in (lower_bound..start).rev() {
            if data.get(candidate..candidate + 2) != Some(b"\x1b[") {
                continue;
            }
            let mut index = candidate + 2;
            while index < data.len() && (0x20..=0x3f).contains(&data[index]) {
                index += 1;
            }
            if index < data.len() && (0x40..=0x7e).contains(&data[index]) && index >= start {
                return Some(index + 1);
            }
        }
        None
    }
}

// ── SessionManager ──────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{get_output_stream, OutputFanout, OutputFanoutInner};
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc;

    fn with_temp_log<R>(name: &str, f: impl FnOnce(std::path::PathBuf) -> R) -> R {
        static COUNTER: AtomicUsize = AtomicUsize::new(1);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "kn-agent-output-test-{}-{}-{}",
            std::process::id(),
            name,
            n
        ));
        let _ = std::fs::remove_dir_all(&root);
        let path = root
            .join("agent")
            .join("sessions")
            .join(name)
            .join("output.log");
        let result = f(path);
        let _ = std::fs::remove_dir_all(&root);
        result
    }

    #[test]
    fn replay_log_result_reports_ok_for_existing_log() {
        with_temp_log("s_ok", |path| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"hello").unwrap();

            let result = OutputFanout::replay_log_result_at_path(&path);

            assert_eq!(result.status, "ok");
            assert_eq!(result.bytes, 5);
            assert_eq!(result.data, b"hello");
            assert_eq!(result.message, None);
        });
    }

    #[test]
    fn replay_log_result_reports_empty_for_missing_log() {
        with_temp_log("s_missing", |path| {
            let result = OutputFanout::replay_log_result_at_path(&path);

            assert_eq!(result.status, "empty");
            assert_eq!(result.bytes, 0);
            assert!(result.data.is_empty());
            assert_eq!(result.message, None);
        });
    }

    #[test]
    fn replay_log_result_reports_empty_for_empty_log() {
        with_temp_log("s_empty", |path| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"").unwrap();

            let result = OutputFanout::replay_log_result_at_path(&path);

            assert_eq!(result.status, "empty");
            assert_eq!(result.bytes, 0);
            assert!(result.data.is_empty());
            assert_eq!(result.message, None);
        });
    }

    #[test]
    fn replay_empty_output_reports_zero_snapshot_high_watermark() {
        with_temp_log("s_replay_empty", |path| {
            let stream = get_output_stream(&path);
            let (tx, mut rx) = mpsc::unbounded_channel();

            OutputFanout::replay_to_wss_at_path(
                "s_replay_empty",
                "replay_request_0001",
                &path,
                stream,
                &tx,
            );

            let done: serde_json::Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
            assert_eq!(done["type"], "replay_output_done");
            assert_eq!(done["data"]["replay_id"], "replay_request_0001");
            assert_eq!(done["data"]["status"], "empty");
            assert_eq!(done["data"]["snapshot_high_watermark"], "0");
        });
    }

    #[test]
    fn replay_snapshot_echoes_its_id_and_current_live_sequence_fence() {
        with_temp_log("s_replay_fence", |path| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"snapshot").unwrap();
            let stream = get_output_stream(&path);
            *stream.cursor.lock().unwrap() = 7;
            let (tx, mut rx) = mpsc::unbounded_channel();

            OutputFanout::replay_to_wss_at_path(
                "s_replay_fence",
                "replay_request_0002",
                &path,
                stream,
                &tx,
            );

            let snapshot: serde_json::Value =
                serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
            assert_eq!(snapshot["type"], "output");
            assert_eq!(snapshot["data"]["replay_id"], "replay_request_0002");
            assert!(snapshot["data"].get("output_seq").is_none());
            let done: serde_json::Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
            assert_eq!(done["data"]["replay_id"], "replay_request_0002");
            assert_eq!(done["data"]["snapshot_high_watermark"], "7");
        });
    }

    #[test]
    fn active_session_log_keeps_output_beyond_the_legacy_ring_limit() {
        with_temp_log("s_complete", |path| {
            let log_size = AtomicU64::new(0);
            let first = vec![b'a'; 256 * 1024];
            let second = vec![b'b'; 256 * 1024];

            OutputFanout::append_log(&path, &first, &log_size).unwrap();
            OutputFanout::append_log(&path, &second, &log_size).unwrap();

            let recovered = std::fs::read(&path).unwrap();
            assert_eq!(recovered.len(), first.len() + second.len());
            assert_eq!(&recovered[..first.len()], first.as_slice());
            assert_eq!(&recovered[first.len()..], second.as_slice());
        });
    }

    #[test]
    fn replay_log_result_reports_error_for_unreadable_path() {
        with_temp_log("s_error", |path| {
            std::fs::create_dir_all(&path).unwrap();

            let result = OutputFanout::replay_log_result_at_path(&path);

            assert_eq!(result.status, "error");
            assert_eq!(result.bytes, 0);
            assert!(result.data.is_empty());
            assert!(result.message.is_some());
        });
    }

    #[test]
    fn replay_safe_tail_skips_a_truncated_csi_prefix_to_a_full_frame() {
        let mut output = b"old-output-old-output;2H".to_vec();
        output.extend_from_slice(b"\x1b[0m\x1b[?2026hcomplete frame\x1b[?2026l");

        let tail = OutputFanout::replay_safe_tail(&output, 40);

        assert_eq!(tail, b"\x1b[?2026hcomplete frame\x1b[?2026l");
    }

    #[test]
    fn replay_safe_tail_keeps_plain_output_before_a_later_escape_sequence() {
        let suffix = b"normal output \x1b[31mred";
        let mut output = b"old-output".to_vec();
        output.extend_from_slice(suffix);

        let tail = OutputFanout::replay_safe_tail(&output, suffix.len());

        assert_eq!(tail, suffix);
    }

    #[test]
    fn replay_safe_tail_preserves_plain_output_without_a_verifiable_escape_boundary() {
        let output = b"old-output;2Hplain output";

        let tail = OutputFanout::replay_safe_tail(output, 15);

        assert_eq!(tail, b";2Hplain output");
    }

    #[test]
    fn replay_safe_tail_never_treats_plain_text_as_a_csi_continuation() {
        let output = b"old-output1 file changed";

        let tail = OutputFanout::replay_safe_tail(output, 14);

        assert_eq!(tail, b"1 file changed");
    }

    #[test]
    fn replay_safe_tail_drops_the_final_byte_of_a_csi_crossing_the_boundary() {
        let output = b"old-output\x1b[31mplain output";

        let tail = OutputFanout::replay_safe_tail(output, 13);

        assert_eq!(tail, b"plain output");
    }

    #[test]
    fn replay_safe_tail_skips_a_utf8_continuation_at_the_boundary() {
        let mut output = b"old-output".to_vec();
        output.extend_from_slice("你restored output".as_bytes());
        let start = b"old-output".len() + 1;

        let tail = OutputFanout::replay_safe_tail(&output, output.len() - start);

        assert_eq!(tail, b"restored output");
    }

    #[test]
    fn flush_pending_delivers_final_output_before_cancellation() {
        with_temp_log("s_final", |path| {
            let (wss_tx, mut wss_rx) = mpsc::unbounded_channel();
            let fanout = OutputFanout {
                inner: Arc::new(OutputFanoutInner {
                    wss_tx: Some(wss_tx),
                    ipc_tx: None,
                    session_nid: "s_final".to_string(),
                    buffer: std::sync::Mutex::new(Vec::new()),
                    extra_subscribers: std::sync::Mutex::new(Vec::new()),
                    log_path: path.clone(),
                    log_size: AtomicU64::new(0),
                    stream: get_output_stream(&path),
                    remote_enabled: None,
                }),
                cancel: tokio_util::sync::CancellationToken::new(),
            };

            fanout.broadcast(b"Error: no session found");
            fanout.flush_pending();

            let message = wss_rx.try_recv().expect("final output should be forwarded");
            assert!(message.contains("no session found"));
            assert_eq!(std::fs::read(path).unwrap(), b"Error: no session found");
        });
    }

    #[test]
    fn log_write_failure_does_not_emit_a_live_frame_or_advance_the_cursor() {
        with_temp_log("s_log_failure", |path| {
            // Opening a directory as the output file deterministically fails
            // on every supported platform, unlike permission-based tests.
            std::fs::create_dir_all(&path).unwrap();
            let stream = get_output_stream(&path);
            let (wss_tx, mut wss_rx) = mpsc::unbounded_channel();
            let (ipc_tx, mut ipc_rx) = mpsc::unbounded_channel();

            OutputFanout::flush_chunked(
                "s_log_failure".to_string(),
                b"not replayable".to_vec(),
                Some(wss_tx),
                Some(ipc_tx),
                path,
                &AtomicU64::new(0),
                stream.clone(),
                None,
            );

            assert!(wss_rx.try_recv().is_err());
            assert_eq!(ipc_rx.try_recv().unwrap(), "not replayable");
            assert_eq!(*stream.cursor.lock().unwrap(), 0);
        });
    }
}
