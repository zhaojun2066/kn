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
    pub writer: Box<dyn Write + Send>,
    #[cfg(unix)]
    pub master_fd: std::os::unix::io::RawFd,
    #[cfg(windows)]
    pub master: Box<dyn portable_pty::MasterPty + Send>,
}

pub struct PtyState {
    pub handles: HashMap<String, PtyHandle>,
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

    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "windows") { "powershell.exe".into() }
        else if cfg!(target_os = "macos") { "/bin/zsh".into() }
        else { "/bin/bash".into() }
    });

    let mut cmd = CommandBuilder::new(&shell);
    // Use login + interactive flags for Unix shells.
    // PowerShell on Windows uses -NoExit instead (no -i/-l semantics).
    if cfg!(target_os = "windows") {
        cmd.args(["-NoExit"]);
    } else {
        cmd.args(["-i", "-l"]);
    }

    if let Some(ref dir) = work_dir {
        if !dir.is_empty() {
            cmd.cwd(dir);
        }
    }

    for (k, v) in std::env::vars() {
        cmd.env(&k, &v);
    }

    // Ensure terminal-essential env vars are set.
    // GUI apps (macOS .app, Windows, etc.) lack TERM and friends;
    // without these the shell may disable line editing or behave as "dumb" terminal.
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("spawn: {}", e))?;

    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("clone reader: {}", e))?;

    // Capture master fd BEFORE take_writer consumes it
    #[cfg(unix)]
    let master_fd = pair.master.as_raw_fd().unwrap_or(-1);

    let writer: Box<dyn Write + Send> = Box::new(
        pair.master
            .take_writer()
            .map_err(|e| format!("take writer: {}", e))?,
    );

    #[cfg(windows)]
    let master: Box<dyn portable_pty::MasterPty + Send> = Box::new(pair.master);

    // Store handle
    {
        let mut handles = state.lock().map_err(|e| format!("lock: {}", e))?;
        handles.handles.insert(
            session_id.clone(),
            PtyHandle {
                writer,
                #[cfg(unix)]
                master_fd,
                #[cfg(windows)]
                master,
            },
        );
    }

    let _ = on_event.send(PtyEvent::Ready);

    std::thread::spawn(move || {
        let mut buf = [0u8; 16384];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    let _ = on_event.send(PtyEvent::Exit(0));
                    break;
                }
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    if on_event.send(PtyEvent::Data(data)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = on_event.send(PtyEvent::Error(e.to_string()));
                    break;
                }
            }
        }
        let _ = child.wait();
    });

    Ok(())
}

#[tauri::command]
pub fn write_pty(
    state: tauri::State<'_, Arc<Mutex<PtyState>>>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    let mut handles = state.lock().map_err(|e| format!("lock: {}", e))?;
    let handle = handles
        .handles
        .get_mut(&session_id)
        .ok_or_else(|| format!("session not found: {}", session_id))?;

    handle
        .writer
        .write_all(data.as_bytes())
        .map_err(|e| format!("write: {}", e))
}

#[tauri::command]
pub fn resize_pty(
    state: tauri::State<'_, Arc<Mutex<PtyState>>>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let handles = state.lock().map_err(|e| format!("lock: {}", e))?;
    let handle = handles
        .handles
        .get(&session_id)
        .ok_or_else(|| format!("session not found: {}", session_id))?;

    let size = PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    };

    #[cfg(unix)]
    {
        if handle.master_fd < 0 {
            return Ok(()); // fd not available, can't resize
        }
        let ws = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        if unsafe { libc::ioctl(handle.master_fd, libc::TIOCSWINSZ, &ws) } != 0 {
            return Err(format!("ioctl TIOCSWINSZ failed"));
        }
    }

    #[cfg(windows)]
    {
        handle
            .master
            .resize(size)
            .map_err(|e| format!("resize: {}", e))?;
    }

    // ioctl TIOCSWINSZ on the master fd already triggers SIGWINCH
    // for the PTY slave's foreground process group via the kernel.
    // No manual kill needed.

    let _ = size; // suppress unused warning on non-windows
    Ok(())
}

#[tauri::command]
pub fn kill_pty(
    state: tauri::State<'_, Arc<Mutex<PtyState>>>,
    session_id: String,
) -> Result<(), String> {
    let mut handles = state.lock().map_err(|e| format!("lock: {}", e))?;
    // Removing the handle drops the writer (PTY master fd).
    // On Unix, this closes the master fd → kernel sends SIGHUP to child → child terminates.
    // The reader thread's child.wait() then returns, cleaning up the process.
    handles.handles.remove(&session_id);
    Ok(())
}

