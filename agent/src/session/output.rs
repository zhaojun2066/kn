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
    /// 环形日志路径（~/.kn/agent/sessions/{nid}/output.log），最大 256KB
    log_path: PathBuf,
    /// 日志当前大小（避免每次 fstat）
    log_size: std::sync::atomic::AtomicU64,
    /// 远程控制开关（共享自 ManagedSession.remote_enabled），None 视为开启
    remote_enabled: Option<Arc<std::sync::atomic::AtomicBool>>,
}

/// 全局日志大小跟踪表（供 relay 模式的 `append_log_static` 使用）。
/// key = session nid, value = 该 session 日志的当前字节数。
static STATIC_LOG_SIZES: std::sync::LazyLock<std::sync::Mutex<HashMap<String, Arc<AtomicU64>>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// 全局日志文件写入锁表，防止并发 append + trim 导致的数据丢失。
/// key = 日志文件规范路径, value = Mutex<()>。
/// `append_log` 和 `trim_log_head` 通过此锁串行化，避免两个并发上下文
/// （spawn_blocking PTY reader + 100ms timer flush）的写-读-写竞争。
static LOG_FILE_LOCKS: std::sync::LazyLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

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

/// 环形日志最大字节数
const OUTPUT_LOG_MAX_SIZE: u64 = 256 * 1024;
/// 截断后保留的尾部字节数
const OUTPUT_LOG_KEEP_TAIL: u64 = 192 * 1024;

impl OutputFanout {
    /// 创建 OutputFanout 并启动 100ms 定时 flush 任务。
    /// `cancel` 用于停止定时器（session 结束时触发）。
    ///
    /// `session_nid` 是云端 DB 主键，对齐新协议 `to_session_id` 类型。
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
            std::fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0)
        );

        let inner = Arc::new(OutputFanoutInner {
            wss_tx: wss,
            ipc_tx: ipc,
            session_nid,
            buffer: std::sync::Mutex::new(Vec::new()),
            extra_subscribers: std::sync::Mutex::new(Vec::new()),
            log_path,
            log_size,
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
        self.inner.extra_subscribers.lock().unwrap_or_else(|e| e.into_inner()).push(tx);
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
        tracing::debug!(received = len, buffered = buf_len, "📟 [PTY-OUT] 收到 PTY 输出");
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
                    inner.remote_enabled.clone(),
                );
            });
        }
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
        remote_enabled: Option<Arc<std::sync::atomic::AtomicBool>>,
    ) {
        const CHUNK_SIZE: usize = 10 * 1024; // 10KB
        let total = data.len();
        let chunks = data.chunks(CHUNK_SIZE).count();
        let preview = String::from_utf8_lossy(if data.len() <= 200 { &data } else { &data[..200] });
        let wss_blocked = remote_enabled.as_ref()
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

        // 写入环形日志
        Self::append_log(&log_path, &data, log_size);

        for (i, chunk) in data.chunks(CHUNK_SIZE).enumerate() {
            let text = String::from_utf8_lossy(chunk);
            if !wss_blocked {
                if let Some(ref tx) = wss_tx {
                    let msg = WsMessageBuilder::output(&session_nid, &text);
                    match tx.send(msg) {
                        Ok(_) => tracing::info!(chunk = i, len = chunk.len(), "📤 [FLUSH] chunk 已发送到 wss_tx"),
                        Err(e) => tracing::error!(chunk = i, error = %e, "📤 [FLUSH] chunk 发送到 wss_tx 失败"),
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

    /// 追加写入环形日志，超过 OUTPUT_LOG_MAX_SIZE 时截掉头部。
    ///
    /// 使用 per-file Mutex 防止两个并发上下文（spawn_blocking PTY reader +
    /// 100ms timer flush）的 write-all → trim 序列互相穿插导致数据丢失。
    fn append_log(path: &PathBuf, data: &[u8], log_size: &std::sync::atomic::AtomicU64) {
        let lock = get_log_lock(path);
        let _guard = lock.lock().unwrap();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::OpenOptions::new().create(true).append(true).open(path) {
            Ok(mut f) => {
                use std::io::Write;
                if let Err(e) = f.write_all(data) {
                    tracing::warn!(path = %path.display(), error = %e, "环形日志写入失败");
                    return;
                }
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "环形日志打开失败");
                return;
            }
        }
        let new_size = log_size.fetch_add(data.len() as u64, std::sync::atomic::Ordering::Relaxed) + data.len() as u64;
        if new_size > OUTPUT_LOG_MAX_SIZE {
            Self::trim_log_head(path, log_size);
        }
    }

    /// 供 relay 模式使用：不依赖 OutputFanout 实例，直接从 nid 写入 ring log。
    /// 通过全局 `STATIC_LOG_SIZES` 表复用 log_size 跟踪，确保 256KB 截断
    /// 在多次调用间正确累积。
    pub fn append_log_static(nid: &str, data: &[u8]) {
        let log_path = kn_common::path::agent_dir()
            .join("sessions")
            .join(nid)
            .join("output.log");
        let log_size = get_static_log_size(nid);
        Self::append_log(&log_path, data, &*log_size);
    }

    /// 读取环形日志全部内容，用于恢复时回放。
    pub fn replay_log(nid: &str) -> Option<Vec<u8>> {
        let path = kn_common::path::agent_dir()
            .join("sessions")
            .join(nid)
            .join("output.log");
        std::fs::read(&path).ok().filter(|d| !d.is_empty())
    }

    /// 截掉日志文件头部，保留尾部 KEEP_TAIL 字节。
    fn trim_log_head(path: &PathBuf, log_size: &std::sync::atomic::AtomicU64) {
        let keep = OUTPUT_LOG_KEEP_TAIL as usize;
        match std::fs::read(path) {
            Ok(data) if data.len() > keep => {
                let tail = &data[data.len() - keep..];
                if let Err(e) = std::fs::write(path, tail) {
                    tracing::warn!(path = %path.display(), error = %e, "环形日志截断失败");
                } else {
                    log_size.store(tail.len() as u64, std::sync::atomic::Ordering::Relaxed);
                    tracing::debug!(path = %path.display(), old_len = data.len(), new_len = tail.len(), "环形日志已截断");
                }
            }
            Err(e) => tracing::warn!(path = %path.display(), error = %e, "环形日志读取失败（截断时）"),
            _ => {}
        }
    }
}

// ── SessionManager ──────────────────────────────────────────
