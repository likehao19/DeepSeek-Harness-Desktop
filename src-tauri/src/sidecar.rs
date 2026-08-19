//! DSH sidecar lifecycle: spawn, port discovery, window, and teardown.
//!
//! The `dsh --profile web` process prints a line like `dsh web:
//! http://127.0.0.1:<port>` once its HTTP server binds. We read that line,
//! derive the actual port (`--port 0` lets the OS pick a free one) and open the
//! main window against it.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// `CREATE_NO_WINDOW` — don't let the node sidecar (a console subsystem app)
/// flash its own console window when spawned from the GUI app.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Current live DSH web URL, shared with the tray's "open in browser".
pub struct WebState(pub Mutex<Option<String>>);

/// Process handle kept in app state so the exit handler can reap it.
pub struct SidecarState(pub Arc<Mutex<Option<Child>>>);

/// Spawn the DSH web backend and open the window once its URL is known.
pub fn start(app: &AppHandle) {
    let state = Arc::new(Mutex::new(None));
    app.manage(SidecarState(Arc::clone(&state)));
    app.manage(WebState(Mutex::new(None)));

    let dsh_home = match resolve_dsh_home(app) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("dsh-desktop: cannot resolve dsh-home: {e}");
            return;
        }
    };
    let workspace = match resolve_workspace(app) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("dsh-desktop: cannot resolve workspace: {e}");
            return;
        }
    };

    let (node, bin) = resolve_runtime(app);
    let mut cmd = Command::new(&node);
    cmd.arg(&bin)
        .args(["--profile", "web", "--port", "0", "--host", "127.0.0.1"])
        .env("DSH_HOME", &dsh_home)
        .env("NO_COLOR", "1")
        .current_dir(&workspace)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "dsh-desktop: failed to spawn dsh (node={} bin={}): {e}",
                node.display(),
                bin.display()
            );
            return;
        }
    };
    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            eprintln!("dsh-desktop: no stdout pipe on sidecar");
            return;
        }
    };
    *state.lock().unwrap() = Some(child);

    let handle = app.clone();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let line = line.unwrap_or_default();
            if let Some(port) = parse_port(&line) {
                let url = format!("http://127.0.0.1:{port}");
                if let Some(state) = handle.try_state::<WebState>() {
                    *state.0.lock().unwrap() = Some(url.clone());
                }
                if let Err(e) = open_window(&handle, &url) {
                    eprintln!("dsh-desktop: failed to open window at {url}: {e}");
                }
                return;
            }
            eprintln!("dsh-desktop[sidecar]: {line}");
        }
        eprintln!(
            "dsh-desktop: sidecar stdout closed before a URL was printed (did dsh boot?)"
        );
    });
}

/// Kill and reap the sidecar process.
pub fn kill(app: &AppHandle) {
    if let Some(state) = app.try_state::<SidecarState>() {
        if let Some(mut child) = state.0.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Resolve the Node binary and the dsh bin.js entry.
///
/// Resolution order:
/// 1. `DSH_DESKTOP_NODE` / `DSH_DESKTOP_RUNTIME` env overrides (packaging/dev).
/// 2. Candidate runtime dirs derived from `resource_dir()` / `exe_dir()`,
///    probing for a `node.exe`. The NSIS bundle may place resources either
///    directly beside the exe or under the updater `_up_` subdir, so we check
///    both. This lets the packaged app run on machines with no Node installed.
/// 3. Dev fallback: `node` on PATH + `<repo>/dsh-runtime`.
fn resolve_runtime(app: &AppHandle) -> (PathBuf, PathBuf) {
    if let Ok(rt) = std::env::var("DSH_DESKTOP_RUNTIME") {
        let rt = PathBuf::from(rt);
        let node = std::env::var("DSH_DESKTOP_NODE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| rt.join("node.exe"));
        return (node, rt.join("node_modules/@deepseek-ai/dsh/lib/bin.js"));
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(res) = app.path().resource_dir() {
        candidates.push(res.join("dsh-runtime"));
        candidates.push(res.join("_up_/dsh-runtime"));
    }
    if let Ok(exe) = app.path().executable_dir() {
        candidates.push(exe.join("dsh-runtime"));
        candidates.push(exe.join("_up_/dsh-runtime"));
    }
    if cfg!(debug_assertions) {
        candidates.push(dev_runtime());
    }
    for rt in candidates {
        let node = rt.join("node.exe");
        if node.exists() {
            return (normalize(node), normalize(rt.join("node_modules/@deepseek-ai/dsh/lib/bin.js")));
        }
    }
    // Fallback: dev path (may not exist; a missing node errors loudly at spawn).
    (dev_runtime().join("node.exe"), dev_runtime().join("node_modules/@deepseek-ai/dsh/lib/bin.js"))
}

/// Strip the Windows `\\?\` extended-length prefix that tauri's path resolver
/// may add; Node chokes on it when resolving the main module.
fn normalize(p: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = p.to_string_lossy();
        let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
        PathBuf::from(s.to_string())
    }
    #[cfg(not(windows))]
    {
        p
    }
}

/// Dev default: `dsh-runtime` next to the project's `src-tauri`.
fn dev_runtime() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dsh-runtime")
}

fn ensure_dir(app: &AppHandle, name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = data_root(app)?.join(name);
    std::fs::create_dir_all(&dir)?;
    Ok(normalize(dir))
}

/// Resolve the DSH home (`DSH_HOME`) used by the sidecar.
///
/// Sync behaviour with a separately installed Harness:
/// 1. `DSH_DESKTOP_DSH_HOME` env override wins.
/// 2. Otherwise, if the user already has a Harness home (`~/.dsh`), reuse it so
///    sessions, credentials, model config and presets are shared.
/// 3. Otherwise fall back to the app-owned `<data_dir>/dsh-home`.
fn resolve_dsh_home(app: &AppHandle) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(p) = std::env::var("DSH_DESKTOP_DSH_HOME") {
        let p = PathBuf::from(p);
        std::fs::create_dir_all(&p)?;
        return Ok(normalize(p));
    }
    let user_home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    if let Some(home) = user_home {
        let candidate = PathBuf::from(&home).join(".dsh");
        if candidate.exists() {
            return Ok(normalize(candidate));
        }
    }
    ensure_dir(app, "dsh-home")
}

/// Resolve the model's working directory (cwd for the sidecar).
fn resolve_workspace(app: &AppHandle) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(p) = std::env::var("DSH_DESKTOP_WORKSPACE") {
        let p = PathBuf::from(p);
        std::fs::create_dir_all(&p)?;
        return Ok(normalize(p));
    }
    ensure_dir(app, "workspace")
}

fn parse_port(line: &str) -> Option<u16> {
    const NEEDLE: &str = "http://127.0.0.1:";
    let idx = line.find(NEEDLE)?;
    let digits: String = line[idx + NEEDLE.len()..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn open_window(app: &AppHandle, url: &str) -> tauri::Result<()> {
    // Keep WebView2's browser data inside the same data root so sandboxed /
    // portable deployments can write it. Must be set before the webview inits.
    if let Ok(root) = data_root(app) {
        let wv = root.join("webview2");
        let _ = std::fs::create_dir_all(&wv);
        std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &wv);
    }
    let webview_url = WebviewUrl::External(url.parse::<url::Url>().expect("valid loopback URL"));
    WebviewWindowBuilder::new(app, "main", webview_url)
        .title("DeepSeek Harness")
        .inner_size(1280.0, 820.0)
        .min_inner_size(900.0, 600.0)
        .build()?;
    Ok(())
}

/// Resolve the base data directory (env override or app data dir).
fn data_root(app: &AppHandle) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(p) = std::env::var("DSH_DESKTOP_DATA_DIR") {
        return Ok(PathBuf::from(p));
    }
    Ok(app.path().app_data_dir()?)
}
