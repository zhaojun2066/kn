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
    pub master: Box<dyn portable_pty::MasterPty + Send>,
}

pub struct PtyState {
    pub handles: HashMap<String, PtyHandle>,
}

fn drain_utf8_stream(buf: &mut Vec<u8>, on_event: &Channel<PtyEvent>) -> bool {
    loop {
        if buf.is_empty() {
            return true;
        }

        match std::str::from_utf8(buf) {
            Ok(s) => {
                if !s.is_empty() && on_event.send(PtyEvent::Data(s.to_string())).is_err() {
                    return false;
                }
                buf.clear();
                return true;
            }
            Err(err) => {
                let valid_up_to = err.valid_up_to();

                if valid_up_to > 0 {
                    let valid = &buf[..valid_up_to];
                    let valid_str = unsafe { std::str::from_utf8_unchecked(valid) };
                    if on_event
                        .send(PtyEvent::Data(valid_str.to_string()))
                        .is_err()
                    {
                        return false;
                    }
                }

                match err.error_len() {
                    Some(len) => {
                        let invalid_end = valid_up_to + len;
                        let invalid =
                            String::from_utf8_lossy(&buf[valid_up_to..invalid_end]).to_string();
                        if !invalid.is_empty() && on_event.send(PtyEvent::Data(invalid)).is_err() {
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
    let home = crate::home_dir().to_string_lossy().to_string();
    let shell_from_env = std::env::var("SHELL").ok();
    let shell = if cfg!(target_os = "windows") {
        let local_appdata = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let mut candidates: Vec<String> = vec![
            r"C:\Program Files\Git\bin\bash.exe".into(),
            r"C:\Program Files (x86)\Git\bin\bash.exe".into(),
            format!(r"{}\AppData\Local\Programs\Git\bin\bash.exe", home),
            r"C:\scoop\apps\git\current\bin\bash.exe".into(),
            r"C:\ProgramData\Git\bin\bash.exe".into(),
        ];
        if !local_appdata.is_empty() {
            candidates.push(format!(
                r"{}\Microsoft\WinGet\Links\bash.exe",
                local_appdata
            ));
        }
        if let Some(env_shell) = shell_from_env {
            if std::path::Path::new(&env_shell).exists() {
                candidates.insert(0, env_shell);
            }
        }
        candidates
            .iter()
            .find(|p| std::path::Path::new(p).exists())
            .cloned()
            .unwrap_or_else(|| "powershell.exe".into())
    } else {
        shell_from_env.unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "/bin/zsh".into()
            } else {
                "/bin/bash".into()
            }
        })
    };

    let shell_lower = shell.to_ascii_lowercase();
    let is_git_bash = cfg!(target_os = "windows") && shell_lower.ends_with("bash.exe");
    let is_powershell = cfg!(target_os = "windows")
        && (shell_lower.ends_with("powershell.exe") || shell_lower.ends_with("pwsh.exe"));

    let mut cmd = CommandBuilder::new(&shell);
    if is_git_bash {
        // Git Bash: login + interactive so .bashrc / shell-rc is sourced
        cmd.args(["-i", "-l"]);
    } else if is_powershell {
        // PowerShell: dot-source shell-rc.ps1 explicitly on startup.
        // Don't depend on $PROFILE — the profile may not be set up yet,
        // or the user may be in a fresh environment.
        let ps1_path = crate::config_dir()
            .join("shell-rc.ps1")
            .display()
            .to_string();
        let ps1_path_escaped = ps1_path.replace('\'', "''");
        let startup_cmd = format!(
            "$rc = '{}'; if (Test-Path $rc) {{ . $rc }} else {{ Write-Host '[kn] shell-rc.ps1 not found at:' $rc }}",
            ps1_path_escaped
        );
        cmd.args([
            "-NoExit",
            "-ExecutionPolicy",
            "Bypass",
            "-NoLogo",
            "-Command",
            &startup_cmd,
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
    // GUI apps (macOS .app, Windows, etc.) lack TERM and friends;
    // without these the shell may disable line editing or behave as "dumb" terminal.
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

    let _ = on_event.send(PtyEvent::Ready);

    // Spawn reader thread with its own clone of the shared child handle.
    let reader_child = shared_child;
    let reader_state = state.inner().clone();
    let reader_session_id = session_id.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 16384];
        let mut utf8_pending: Vec<u8> = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    if !utf8_pending.is_empty() {
                        let pending = String::from_utf8_lossy(&utf8_pending).to_string();
                        if !pending.is_empty() {
                            let _ = on_event.send(PtyEvent::Data(pending));
                        }
                    }
                    break;
                }
                Ok(n) => {
                    utf8_pending.extend_from_slice(&buf[..n]);
                    if !drain_utf8_stream(&mut utf8_pending, &on_event) {
                        break;
                    }
                }
                Err(e) => {
                    if !utf8_pending.is_empty() {
                        let pending = String::from_utf8_lossy(&utf8_pending).to_string();
                        if !pending.is_empty() {
                            let _ = on_event.send(PtyEvent::Data(pending));
                        }
                    }
                    let _ = on_event.send(PtyEvent::Error(e.to_string()));
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

        let _ = on_event.send(PtyEvent::Exit(exit_code));

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
        // Kill the child process first — this unblocks the reader thread
        // on Windows where ConPTY reads may otherwise hang after the master is dropped.
        if let Ok(mut guard) = child.lock() {
            if let Some(ref mut c) = *guard {
                let _ = c.kill();
            }
        }
    }

    let mut handles = state.lock().map_err(|e| format!("lock: {}", e))?;
    // Removing the handle drops the writer and master.
    // On Unix: closing master fd → kernel sends SIGHUP → child terminates.
    // On Windows: dropping master closes ConPTY → reader receives error.
    handles.handles.remove(&session_id);
    Ok(())
}
