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

/// Thread-safe writer handle so `write_pty` can lock it independently
/// without holding the global PtyState lock during I/O.
pub type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;

/// Shared child process handle so `kill_pty` can terminate the process
/// even if the reader thread is blocked on I/O (Windows ConPTY quirk).
pub type SharedChild = Arc<Mutex<Option<Box<dyn portable_pty::Child + Send>>>>;

pub struct PtyHandle {
    pub writer: SharedWriter,
    pub child: SharedChild,
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

    // Resolve shell binary.
    // On Windows, prefer Git Bash (provides Unix environment where shell-rc works).
    // Fall back to PowerShell with execution policy relaxed if Git Bash is absent.
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "windows") {
            let candidates = [
                r"C:\Program Files\Git\bin\bash.exe",
                r"C:\Program Files (x86)\Git\bin\bash.exe",
                // User-only installs (no admin)
                &format!(r"{}\AppData\Local\Programs\Git\bin\bash.exe", home),
                // Scoop / manual installs
                r"C:\scoop\apps\git\current\bin\bash.exe",
                // ProgramData (all-users, alternate)
                r"C:\ProgramData\Git\bin\bash.exe",
            ];
            candidates
                .iter()
                .find(|p| std::path::Path::new(p).exists())
                .map(|&s| s.to_string())
                .unwrap_or_else(|| "powershell.exe".into())
        } else if cfg!(target_os = "macos") {
            "/bin/zsh".into()
        } else {
            "/bin/bash".into()
        }
    });

    let is_git_bash = cfg!(target_os = "windows") && shell.ends_with("bash.exe");
    let is_powershell = cfg!(target_os = "windows") && (shell.ends_with("powershell.exe") || shell.ends_with("pwsh.exe"));

    let mut cmd = CommandBuilder::new(&shell);
    if is_git_bash {
        // Git Bash: login + interactive so .bashrc / shell-rc is sourced
        cmd.args(["-i", "-l"]);
    } else if is_powershell {
        // PowerShell: dot-source shell-rc.ps1 explicitly on startup.
        // Don't depend on $PROFILE — the profile may not be set up yet,
        // or the user may be in a fresh environment.
        let ps1_path = format!("{}/.claude-profiles/shell-rc.ps1", home);
        let ps1_path_escaped = ps1_path.replace('\'', "''");
        let startup_cmd = format!(
            "$rc = '{}'; if (Test-Path $rc) {{ . $rc }} else {{ Write-Host '[AI Profile Manager] shell-rc.ps1 not found at:' $rc }}",
            ps1_path_escaped
        );
        cmd.args([
            "-NoExit",
            "-ExecutionPolicy", "Bypass",
            "-NoLogo",
            "-Command", &startup_cmd,
        ]);
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
    cmd.env("TERM_PROGRAM", "ai-profile-manager");

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

    // Capture master fd BEFORE take_writer (safety: ensures fd is still valid)
    #[cfg(unix)]
    let master_fd = pair.master.as_raw_fd().unwrap_or(-1);

    let writer: SharedWriter = Arc::new(Mutex::new(Box::new(
        pair.master
            .take_writer()
            .map_err(|e| format!("take writer: {}", e))?,
    ) as Box<dyn Write + Send>));

    #[cfg(windows)]
    let master: Box<dyn portable_pty::MasterPty + Send> = pair.master;

    // Store handle
    {
        let mut handles = state.lock().map_err(|e| format!("lock: {}", e))?;
        handles.handles.insert(
            session_id.clone(),
            PtyHandle {
                writer,
                child: shared_child.clone(),
                #[cfg(unix)]
                master_fd,
                #[cfg(windows)]
                master,
            },
        );
    }

    let _ = on_event.send(PtyEvent::Ready);

    // Spawn reader thread with its own clone of the shared child handle.
    let reader_child = shared_child;
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
        // Wait for child to fully exit (takes the child out of the Arc).
        if let Ok(mut guard) = reader_child.lock() {
            if let Some(mut c) = guard.take() {
                let _ = c.wait();
            }
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
    // This prevents a blocked write (full ConPTY buffer on Windows)
    // from starving resize/kill operations on the same or other sessions.
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

    // On Unix, we need the master_fd. On Windows, we need the master.
    // Both are fast kernel calls — the lock is held only briefly.
    let handles = state.lock().map_err(|e| format!("lock: {}", e))?;
    let handle = handles
        .handles
        .get(&session_id)
        .ok_or_else(|| format!("session not found: {}", session_id))?;

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
    if let Some(handle) = handles.handles.get(&session_id) {
        // Kill the child process first — this unblocks the reader thread
        // on Windows where ConPTY reads may otherwise hang after the master is dropped.
        if let Ok(mut guard) = handle.child.lock() {
            if let Some(ref mut c) = *guard {
                let _ = c.kill();
            }
        }
    }
    // Removing the handle drops the writer and master.
    // On Unix: closing master fd → kernel sends SIGHUP → child terminates.
    // On Windows: dropping master closes ConPTY → reader receives error.
    handles.handles.remove(&session_id);
    Ok(())
}

