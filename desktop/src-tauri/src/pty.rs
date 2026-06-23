use kn_common::pty_trait::{PtyOutputSink, SharedChild, SharedWriter};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum PtyEvent {
    Ready,
    Data(String),
    Exit(i32),
    Error(String),
}

pub struct PtyHandle {
    pub writer: SharedWriter,
    pub child: SharedChild,
    pub master: Box<dyn portable_pty::MasterPty + Send>,
}

pub struct PtyState {
    pub handles: HashMap<String, PtyHandle>,
}

fn drain_utf8_stream(buf: &mut Vec<u8>, sink: &impl PtyOutputSink) -> bool {
    loop {
        if buf.is_empty() {
            return true;
        }

        match std::str::from_utf8(buf) {
            Ok(s) => {
                if !s.is_empty() && sink.send(s.as_bytes()).is_err() {
                    return false;
                }
                buf.clear();
                return true;
            }
            Err(err) => {
                let valid_up_to = err.valid_up_to();

                if valid_up_to > 0 {
                    let valid = &buf[..valid_up_to];
                    if sink.send(valid).is_err() {
                        return false;
                    }
                }

                match err.error_len() {
                    Some(len) => {
                        let invalid_end = valid_up_to + len;
                        if sink.send(&buf[valid_up_to..invalid_end]).is_err() {
                            return false;
                        }
                        buf.drain(..invalid_end);
                    }
                    None => {
                        buf.drain(..valid_up_to);
                        return true;
                    }
                }
            }
        }
    }
}

/// Tauri Channel 适配器 — 实现 PtyOutputSink，将 PTY 输出桥接到前端。
struct ChannelSink {
    channel: Channel<PtyEvent>,
}

impl PtyOutputSink for ChannelSink {
    fn send(&self, data: &[u8]) -> Result<(), String> {
        if let Ok(text) = std::str::from_utf8(data) {
            self.channel
                .send(PtyEvent::Data(text.to_string()))
                .map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    }

    fn on_ready(&self) -> Result<(), String> {
        self.channel
            .send(PtyEvent::Ready)
            .map_err(|e| e.to_string())
    }

    fn on_exit(&self, code: i32) -> Result<(), String> {
        self.channel
            .send(PtyEvent::Exit(code))
            .map_err(|e| e.to_string())
    }

    fn on_error(&self, msg: &str) -> Result<(), String> {
        self.channel
            .send(PtyEvent::Error(msg.to_string()))
            .map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn start_pty(
    state: tauri::State<'_, Arc<Mutex<PtyState>>>,
    session_id: String,
    work_dir: Option<String>,
    cols: u16,
    rows: u16,
    on_event: Channel<PtyEvent>,
) -> Result<(), String> {
    let pty_system = NativePtySystem::default();

    let size = PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    };

    let pair = pty_system
        .openpty(size)
        .map_err(|e| format!("openpty: {}", e))?;

    // Resolve shell binary — use $SHELL or default to /bin/zsh
    let shell = std::env::var("SHELL").ok().unwrap_or_else(|| "/bin/zsh".into());

    let mut cmd = CommandBuilder::new(&shell);
    cmd.args(["-i", "-l"]);

    if let Some(ref dir) = work_dir {
        if !dir.is_empty() {
            cmd.cwd(dir);
        }
    }

    for (k, v) in std::env::vars() {
        cmd.env(&k, &v);
    }

    // macOS GUI apps launched from Finder/Dock get a minimal PATH
    // (/usr/bin:/bin:/usr/sbin:/sbin) that misses Homebrew and other
    // user-installed tools. Append common paths so `claude`, `node`,
    // `python3`, etc. are found inside the PTY.
    if cfg!(target_os = "macos") {
        let current_path = std::env::var("PATH").unwrap_or_default();
        let extra = [
            "/opt/homebrew/bin",
            "/opt/homebrew/sbin",
            "/usr/local/bin",
            "/usr/local/sbin",
        ];
        let missing: Vec<&str> = extra
            .iter()
            .filter(|p| !current_path.split(':').any(|seg| seg == **p))
            .copied()
            .collect();
        if !missing.is_empty() {
            let augmented = format!("{}:{}", current_path, missing.join(":"));
            cmd.env("PATH", augmented);
        }
    }

    if std::env::var_os("LANG").is_none() {
        cmd.env("LANG", "en_US.UTF-8");
    }
    if std::env::var_os("LC_CTYPE").is_none() {
        cmd.env("LC_CTYPE", "en_US.UTF-8");
    }

    // Ensure terminal-essential env vars are set.
    // macOS .app bundles lack TERM and friends; without these
    // the shell may disable line editing or behave as "dumb" terminal.
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "kn");

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("spawn: {}", e))?;

    drop(pair.slave);

    // Share child handle between PtyHandle (for kill_pty) and reader thread (for wait).
    let shared_child: SharedChild = Arc::new(Mutex::new(Some(child)));

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("clone reader: {}", e))?;

    let writer: SharedWriter = Arc::new(Mutex::new(Box::new(
        pair.master
            .take_writer()
            .map_err(|e| format!("take writer: {}", e))?,
    ) as Box<dyn Write + Send>));

    let master: Box<dyn portable_pty::MasterPty + Send> = pair.master;
    // Store handle
    {
        let mut handles = state.lock().map_err(|e| format!("lock: {}", e))?;
        handles.handles.insert(
            session_id.clone(),
            PtyHandle {
                writer,
                child: shared_child.clone(),
                master,
            },
        );
    }

    let sink = ChannelSink {
        channel: on_event.clone(),
    };
    let _ = sink.on_ready();

    // Spawn reader thread with its own clone of the shared child handle.
    let reader_child = shared_child;
    let reader_state = state.inner().clone();
    let reader_session_id = session_id.clone();
    let reader_sink = ChannelSink {
        channel: on_event.clone(),
    };
    std::thread::spawn(move || {
        let mut buf = [0u8; 16384];
        let mut utf8_pending: Vec<u8> = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    if !utf8_pending.is_empty() {
                        let _ = reader_sink.send(&utf8_pending);
                    }
                    break;
                }
                Ok(n) => {
                    utf8_pending.extend_from_slice(&buf[..n]);
                    if !drain_utf8_stream(&mut utf8_pending, &reader_sink) {
                        break;
                    }
                }
                Err(e) => {
                    if !utf8_pending.is_empty() {
                        let _ = reader_sink.send(&utf8_pending);
                    }
                    let _ = reader_sink.on_error(&e.to_string());
                    break;
                }
            }
        }
        // Wait for child to fully exit (takes the child out of the Arc).
        let exit_code = if let Ok(mut guard) = reader_child.lock() {
            if let Some(mut c) = guard.take() {
                c.wait()
                    .map(|status| status.exit_code() as i32)
                    .unwrap_or(1)
            } else {
                0
            }
        } else {
            1
        };

        let _ = reader_sink.on_exit(exit_code);

        if let Ok(mut handles) = reader_state.lock() {
            handles.handles.remove(&reader_session_id);
        }
    });

    Ok(())
}

#[tauri::command]
pub fn write_pty(
    state: tauri::State<'_, Arc<Mutex<PtyState>>>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    // Clone the Arc<Mutex<Writer>> while holding the global lock,
    // then drop the lock before performing the actual I/O.
    // This prevents a blocked write from starving resize/kill
    // operations on the same or other sessions.
    let writer: SharedWriter = {
        let handles = state.lock().map_err(|e| format!("lock: {}", e))?;
        handles
            .handles
            .get(&session_id)
            .ok_or_else(|| format!("session not found: {}", session_id))?
            .writer
            .clone()
    };

    let mut w = writer.lock().map_err(|e| format!("writer lock: {}", e))?;
    w.write_all(data.as_bytes())
        .map_err(|e| format!("write: {}", e))
}

#[tauri::command]
pub fn resize_pty(
    state: tauri::State<'_, Arc<Mutex<PtyState>>>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let size = PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    };

    // Keep the MasterPty alive for the lifetime of the session and resize
    // through portable-pty. On Unix this calls TIOCSWINSZ on a valid master
    // handle and delivers SIGWINCH to full-screen TUIs such as Claude.
    let handles = state.lock().map_err(|e| format!("lock: {}", e))?;
    let handle = handles
        .handles
        .get(&session_id)
        .ok_or_else(|| format!("session not found: {}", session_id))?;

    handle
        .master
        .resize(size)
        .map_err(|e| format!("resize: {}", e))
}

#[tauri::command]
pub fn kill_pty(
    state: tauri::State<'_, Arc<Mutex<PtyState>>>,
    session_id: String,
) -> Result<(), String> {
    let child = {
        let handles = state.lock().map_err(|e| format!("lock: {}", e))?;
        handles
            .handles
            .get(&session_id)
            .map(|handle| handle.child.clone())
    };

    if let Some(child) = child {
        // Kill the child process first to unblock the reader thread.
        if let Ok(mut guard) = child.lock() {
            if let Some(ref mut c) = *guard {
                let _ = c.kill();
            }
        }
    }

    let mut handles = state.lock().map_err(|e| format!("lock: {}", e))?;
    // Removing the handle drops the writer and master.
    // Closing master fd → kernel sends SIGHUP → child terminates.
    handles.handles.remove(&session_id);
    Ok(())
}
