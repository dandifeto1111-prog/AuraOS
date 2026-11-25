//! AuraOS v5.1 Multi-User Developer Hub Engine
//! - Production-ready per-user AI Quotas & Status (Antigravity, Gemini, Claude, Local Ollama, OpenRouter)
//!   using ONLY official/documented APIs, env vars, and user-managed local config (`%LocalAppData%\AuraShell\ai_quotas.json`)!
//! - Multi-User GitHub & Local Git Commits Monitor (auto-detects the active user's GitHub identity via
//!   `ai_quotas.json`, `gh` CLI, local repo `origin`, or `git config --global user.name` — ZERO hardcoded usernames!)
//! - Persistent Sidebar To-Do Checklist (`%LocalAppData%\AuraShell\todos.json`)!

use crate::models::app_data_dir;
use crate::win32_shell::to_wide;
use serde::{Deserialize, Serialize};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use windows::core::PCWSTR;
use windows::Win32::Networking::WinHttp::{
    WinHttpAddRequestHeaders, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
    WinHttpQueryDataAvailable, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetTimeouts, WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_ADDREQ_FLAG_ADD,
    WINHTTP_FLAG_SECURE, WINHTTP_OPEN_REQUEST_FLAGS,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// ==================== 1. PERSISTENT PER-USER AI QUOTAS & DEV CONFIG ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DevHubConfig {
    pub github_username: String,
    pub workspace_dirs: Vec<String>,
    pub antigravity_pct: i32,
    pub gemini_pct: i32,
    pub claude_pct: i32,
    pub openrouter_api_key: String,
}

impl Default for DevHubConfig {
    fn default() -> Self {
        Self {
            github_username: String::new(),
            workspace_dirs: Vec::new(),
            antigravity_pct: 84,
            gemini_pct: 76,
            claude_pct: 68,
            openrouter_api_key: String::new(),
        }
    }
}

pub fn load_dev_hub_config() -> DevHubConfig {
    let p = app_data_dir().join("ai_quotas.json");
    if let Ok(txt) = std::fs::read_to_string(p) {
        if let Ok(cfg) = serde_json::from_str::<DevHubConfig>(&txt) {
            return cfg;
        }
    }
    DevHubConfig::default()
}

pub fn save_dev_hub_config(cfg: &DevHubConfig) {
    let p = app_data_dir().join("ai_quotas.json");
    if let Ok(txt) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(p, txt);
    }
}

#[derive(Debug, Clone)]
pub struct AiProviderStatus {
    pub id: String,
    pub name: String,
    pub subtitle: String,
    pub pct_remaining: i32, // 0..100
    pub badge: String,
    pub live_connected: bool,
    pub dashboard_url: String,
    pub adjustable: bool,
}

#[derive(Debug, Clone)]
pub struct GitRepoSummary {
    pub name: String,
    pub path: String,
    pub branch: String,
    pub dirty_files: usize,
    pub last_hash: String,
    pub last_msg: String,
    pub last_when: String,
}

#[derive(Debug, Clone, Default)]
pub struct GitHubActivity {
    pub username: String,
    pub profile_url: String,
    pub daily_commits_7d: [u32; 7], // oldest -> today
    pub total_recent_commits: u32,
    pub latest_push_repo: String,
    pub latest_push_msg: String,
    pub fetched_ok: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DevHubSnapshot {
    pub providers: Vec<AiProviderStatus>,
    pub repos: Vec<GitRepoSummary>,
    pub github: GitHubActivity,
    pub ollama_models: Vec<String>,
    pub ollama_online: bool,
}

fn run_cmd_capture(program: &str, args: &[&str], cwd: Option<&Path>) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args).creation_flags(CREATE_NO_WINDOW);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn http_get_json(host: &str, port: u16, path: &str, secure: bool, auth_header: Option<&str>) -> Option<String> {
    unsafe {
        let ua = to_wide("AuraOS/5.1");
        let h_sess = WinHttpOpen(
            PCWSTR(ua.as_ptr()),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        );
        if h_sess.is_null() {
            return None;
        }
        let _ = WinHttpSetTimeouts(h_sess, 1200, 1500, 1500, 2200);
        let whost = to_wide(host);
        let h_conn = WinHttpConnect(h_sess, PCWSTR(whost.as_ptr()), port, 0);
        if h_conn.is_null() {
            let _ = WinHttpCloseHandle(h_sess);
            return None;
        }
        let verb = to_wide("GET");
        let wpath = to_wide(path);
        let flags = if secure { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) };
        let h_req = WinHttpOpenRequest(
            h_conn,
            PCWSTR(verb.as_ptr()),
            PCWSTR(wpath.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            flags,
        );
        if h_req.is_null() {
            let _ = WinHttpCloseHandle(h_conn);
            let _ = WinHttpCloseHandle(h_sess);
            return None;
        }
        let hdr_str = match auth_header {
            Some(a) => format!("Accept: application/json\r\nAuthorization: {}\r\n", a),
            None => "Accept: application/json\r\n".to_string(),
        };
        let whdr = to_wide(&hdr_str);
        let _ = WinHttpAddRequestHeaders(h_req, &whdr[..whdr.len().saturating_sub(1)], WINHTTP_ADDREQ_FLAG_ADD);

        let ok = WinHttpSendRequest(h_req, None, None, 0, 0, 0).is_ok()
            && WinHttpReceiveResponse(h_req, std::ptr::null_mut()).is_ok();
        let mut body = Vec::new();
        if ok {
            loop {
                let mut avail = 0u32;
                if WinHttpQueryDataAvailable(h_req, &mut avail).is_err() || avail == 0 {
                    break;
                }
                let mut chunk = vec![0u8; avail as usize];
                let mut read = 0u32;
                if WinHttpReadData(h_req, chunk.as_mut_ptr() as *mut _, avail, &mut read).is_err() || read == 0 {
                    break;
                }
                body.extend_from_slice(&chunk[..read as usize]);
                if body.len() > 256 * 1024 {
                    break;
                }
            }
        }
        let _ = WinHttpCloseHandle(h_req);
        let _ = WinHttpCloseHandle(h_conn);
        let _ = WinHttpCloseHandle(h_sess);
        if body.is_empty() {
            None
        } else {
            String::from_utf8(body).ok()
        }
    }
}

fn is_process_running(target_stem: &str) -> bool {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return false };
        let mut pe = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = false;
        if Process32FirstW(snap, &mut pe).is_ok() {
            loop {
                let end = pe.szExeFile.iter().position(|&c| c == 0).unwrap_or(pe.szExeFile.len());
                let name = String::from_utf16_lossy(&pe.szExeFile[..end]).to_lowercase();
                if name.starts_with(target_stem) {
                    found = true;
                    break;
                }
                if Process32NextW(snap, &mut pe).is_err() {
                    break;
                }
            }
        }
        let _ = windows::Win32::Foundation::CloseHandle(snap);
        found
    }
}

fn query_local_ollama() -> (bool, Vec<String>, usize) {
    let Some(txt) = http_get_json("127.0.0.1", 11434, "/api/tags", false, None) else {
        return (false, Vec::new(), 0);
    };
    let mut models = Vec::new();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
        if let Some(arr) = v.get("models").and_then(|m| m.as_array()) {
            for item in arr.iter().take(6) {
                if let Some(n) = item.get("name").and_then(|s| s.as_str()) {
                    models.push(n.to_string());
                }
            }
        }
    }
    let loaded = http_get_json("127.0.0.1", 11434, "/api/ps", false, None)
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("models").and_then(|m| m.as_array()).map(|a| a.len()))
        .unwrap_or(0);
    (true, models, loaded)
}

fn query_openrouter_credits(key: &str) -> Option<(f64, f64)> {
    if key.trim().is_empty() {
        return None;
    }
    let auth = format!("Bearer {}", key.trim());
    let txt = http_get_json("openrouter.ai", 443, "/api/v1/auth/key", true, Some(&auth))?;
    let v: serde_json::Value = serde_json::from_str(&txt).ok()?;
    let data = v.get("data")?;
    let usage = data.get("usage").and_then(|x| x.as_f64()).unwrap_or(0.0);
    let limit = data.get("limit").and_then(|x| x.as_f64()).unwrap_or(0.0);
    Some((usage, limit))
}

fn scan_local_git_repos(cfg: &DevHubConfig) -> Vec<GitRepoSummary> {
    let mut roots: Vec<PathBuf> = cfg.workspace_dirs.iter().map(PathBuf::from).collect();
    if let Ok(home) = std::env::var("USERPROFILE") {
        let hp = PathBuf::from(home);
        for rel in [
            r".gemini\antigravity\scratch",
            r"Documents\GitHub",
            r"Projects",
            r"source\repos",
            r"Desktop",
        ] {
            let p = hp.join(rel);
            if p.exists() && !roots.iter().any(|r| r == &p) {
                roots.push(p);
            }
        }
    }

    let mut repo_paths: Vec<(u64, PathBuf)> = Vec::new();
    for root in roots {
        if root.join(".git").exists() {
            let mtime = std::fs::metadata(root.join(".git"))
                .and_then(|m| m.modified())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "")))
                .map(|d| d.as_secs())
                .unwrap_or(0);
            repo_paths.push((mtime, root));
            continue;
        }
        if let Ok(rd) = std::fs::read_dir(&root) {
            for ent in rd.flatten() {
                let p = ent.path();
                if p.is_dir() && p.join(".git").exists() {
                    let mtime = std::fs::metadata(p.join(".git"))
                        .and_then(|m| m.modified())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "")))
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    repo_paths.push((mtime, p));
                }
            }
        }
    }

    repo_paths.sort_by(|a, b| b.0.cmp(&a.0));
    repo_paths.dedup_by(|a, b| a.1 == b.1);

    let mut out = Vec::new();
    for (_, path) in repo_paths.into_iter().take(4) {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Repo")
            .to_string();
        let branch = run_cmd_capture("git", &["rev-parse", "--abbrev-ref", "HEAD"], Some(&path))
            .unwrap_or_else(|| "main".to_string());
        let dirty_files = run_cmd_capture("git", &["status", "--porcelain"], Some(&path))
            .map(|s| s.lines().count())
            .unwrap_or(0);
        let log_line = run_cmd_capture("git", &["log", "-1", "--format=%h|%cr|%s"], Some(&path))
            .unwrap_or_default();
        let mut parts = log_line.splitn(3, '|');
        let last_hash = parts.next().unwrap_or("—").to_string();
        let last_when = parts.next().unwrap_or("reciente").to_string();
        let last_msg = parts.next().unwrap_or("Sin commits").to_string();

        out.push(GitRepoSummary {
            name,
            path: path.to_string_lossy().to_string(),
            branch,
            dirty_files,
            last_hash,
            last_msg,
            last_when,
        });
    }
    out
}

fn parse_github_owner_from_remote(remote: &str) -> Option<String> {
    let r = remote.trim();
    if let Some(pos) = r.find("github.com") {
        let rest = &r[pos + "github.com".len()..];
        let trimmed = rest.trim_start_matches(['/', ':']);
        let owner = trimmed.split('/').next()?.trim();
        if !owner.is_empty() && owner.len() < 48 {
            return Some(owner.to_string());
        }
    }
    None
}

fn resolve_github_username(cfg: &DevHubConfig, repos: &[GitRepoSummary]) -> String {
    if !cfg.github_username.trim().is_empty() {
        return cfg.github_username.trim().to_string();
    }
    // 1. Official GitHub CLI active authenticated user (if installed & signed in by the user!)
    if let Some(gh_user) = run_cmd_capture("gh", &["api", "user", "--jq", ".login"], None) {
        if !gh_user.contains(' ') && gh_user.len() < 48 {
            return gh_user;
        }
    }
    // 2. Match local repo `origin` owner ONLY if latest commit author matches user's own git email/name!
    let my_email = run_cmd_capture("git", &["config", "--global", "user.email"], None)
        .unwrap_or_default()
        .to_lowercase();
    let my_name = run_cmd_capture("git", &["config", "--global", "user.name"], None)
        .unwrap_or_default();
    let my_name_low = my_name.to_lowercase();

    for r in repos {
        let rp = Path::new(&r.path);
        let author_ok = if !my_email.is_empty() || !my_name_low.is_empty() {
            let last_auth = run_cmd_capture("git", &["log", "-1", "--format=%ae|%an"], Some(rp))
                .unwrap_or_default()
                .to_lowercase();
            (!my_email.is_empty() && last_auth.contains(&my_email))
                || (!my_name_low.is_empty() && last_auth.contains(&my_name_low))
        } else {
            true
        };
        if author_ok {
            if let Some(url) = run_cmd_capture("git", &["remote", "get-url", "origin"], Some(rp)) {
                if let Some(owner) = parse_github_owner_from_remote(&url) {
                    return owner;
                }
            }
        }
    }
    // 3. Fallback to `git config --global user.name` if single-token handle
    if !my_name.is_empty() && !my_name.contains(' ') && my_name.len() < 48 {
        return my_name;
    }
    // 4. Last fallback: any local repo origin owner
    for r in repos {
        if let Some(url) = run_cmd_capture("git", &["remote", "get-url", "origin"], Some(Path::new(&r.path))) {
            if let Some(owner) = parse_github_owner_from_remote(&url) {
                return owner;
            }
        }
    }
    String::new()
}

fn parse_iso_day_key(iso: &str) -> Option<i64> {
    if iso.len() < 10 || !iso.is_ascii() {
        return None;
    }
    let y: i64 = iso.get(0..4)?.parse().ok()?;
    let m: i64 = iso.get(5..7)?.parse().ok()?;
    let d: i64 = iso.get(8..10)?.parse().ok()?;
    // Julian-like day number for exact day deltas
    let a = (14 - m) / 12;
    let y2 = y + 4800 - a;
    let m2 = m + 12 * a - 3;
    Some(d + (153 * m2 + 2) / 5 + 365 * y2 + y2 / 4 - y2 / 100 + y2 / 400 - 32045)
}

fn today_day_key() -> i64 {
    let st = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let iso = format!("{:04}-{:02}-{:02}", st.wYear, st.wMonth, st.wDay);
    parse_iso_day_key(&iso).unwrap_or(0)
}

fn fetch_github_activity(username: &str) -> GitHubActivity {
    if username.trim().is_empty() {
        return GitHubActivity::default();
    }
    let profile_url = format!("https://github.com/{}", username);
    let mut act = GitHubActivity {
        username: username.to_string(),
        profile_url,
        daily_commits_7d: [0; 7],
        total_recent_commits: 0,
        latest_push_repo: String::new(),
        latest_push_msg: String::new(),
        fetched_ok: false,
    };

    let path = format!("/users/{}/events/public?per_page=30", username);
    let Some(body) = http_get_json("api.github.com", 443, &path, true, None) else {
        return act;
    };
    let Ok(serde_json::Value::Array(events)) = serde_json::from_str::<serde_json::Value>(&body) else {
        return act;
    };
    act.fetched_ok = true;
    let today = today_day_key();

    for ev in events {
        let ev_type = ev.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if ev_type != "PushEvent" {
            continue;
        }
        let created = ev.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
        let commits = ev
            .get("payload")
            .and_then(|p| p.get("commits"))
            .and_then(|c| c.as_array());
        let cnt = commits.map(|c| c.len() as u32).unwrap_or(1).max(1);
        act.total_recent_commits += cnt;

        if let Some(dk) = parse_iso_day_key(created) {
            let diff = today - dk;
            if (0..7).contains(&diff) {
                let idx = (6 - diff) as usize;
                act.daily_commits_7d[idx] += cnt;
            }
        }
        if act.latest_push_repo.is_empty() {
            act.latest_push_repo = ev
                .get("repo")
                .and_then(|r| r.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .split('/')
                .last()
                .unwrap_or("")
                .to_string();
            act.latest_push_msg = commits
                .and_then(|arr| arr.last())
                .and_then(|c| c.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Push reciente")
                .lines()
                .next()
                .unwrap_or("Push reciente")
                .to_string();
        }
    }
    act
}

/// Runs off the UI thread to collect live AI provider telemetry + local Git repos + GitHub public events.
pub fn fetch_dev_hub_snapshot(cfg: &DevHubConfig) -> DevHubSnapshot {
    let (ollama_online, ollama_models, ollama_loaded) = query_local_ollama();
    let ag_running = is_process_running("antigravity");
    let has_gemini_key = std::env::var("GEMINI_API_KEY").map(|v| !v.trim().is_empty()).unwrap_or(false);
    let has_claude_key = std::env::var("ANTHROPIC_API_KEY").map(|v| !v.trim().is_empty()).unwrap_or(false);

    let mut providers = vec![
        AiProviderStatus {
            id: "antigravity".into(),
            name: "Antigravity".into(),
            subtitle: if ag_running { "Sesión activa · Cuota usuario".into() } else { "Cuenta de usuario · Ajustable".into() },
            pct_remaining: cfg.antigravity_pct.clamp(0, 100),
            badge: if ag_running { "ACTIVA".into() } else { "CUENTA".into() },
            live_connected: ag_running,
            dashboard_url: "https://aistudio.google.com/".into(),
            adjustable: true,
        },
        AiProviderStatus {
            id: "gemini".into(),
            name: "Gemini".into(),
            subtitle: if has_gemini_key { "GEMINI_API_KEY detectada".into() } else { "Google AI Studio · Tu cuenta".into() },
            pct_remaining: cfg.gemini_pct.clamp(0, 100),
            badge: if has_gemini_key { "API OK".into() } else { "WEB".into() },
            live_connected: has_gemini_key,
            dashboard_url: "https://aistudio.google.com/apikey".into(),
            adjustable: true,
        },
        AiProviderStatus {
            id: "claude".into(),
            name: "Claude".into(),
            subtitle: if has_claude_key { "ANTHROPIC_API_KEY detectada".into() } else { "Anthropic Console · Tu cuenta".into() },
            pct_remaining: cfg.claude_pct.clamp(0, 100),
            badge: if has_claude_key { "API OK".into() } else { "WEB".into() },
            live_connected: has_claude_key,
            dashboard_url: "https://claude.ai/settings/usage".into(),
            adjustable: true,
        },
    ];

    // Live OpenRouter credits if user configured OPENROUTER_API_KEY env var or ai_quotas.json key
    let or_key = if !cfg.openrouter_api_key.trim().is_empty() {
        cfg.openrouter_api_key.clone()
    } else {
        std::env::var("OPENROUTER_API_KEY").unwrap_or_default()
    };
    if let Some((usage, limit)) = query_openrouter_credits(&or_key) {
        let pct = if limit > 0.001 {
            (((limit - usage).max(0.0) / limit) * 100.0).round() as i32
        } else {
            100
        };
        providers.push(AiProviderStatus {
            id: "openrouter".into(),
            name: "OpenRouter".into(),
            subtitle: format!("Usado ${:.2} / ${:.2}", usage, limit),
            pct_remaining: pct.clamp(0, 100),
            badge: "EN VIVO".into(),
            live_connected: true,
            dashboard_url: "https://openrouter.ai/credits".into(),
            adjustable: false,
        });
    } else {
        providers.push(AiProviderStatus {
            id: "ollama".into(),
            name: "Ollama Local".into(),
            subtitle: if ollama_online {
                format!("{} modelos · {} en VRAM", ollama_models.len(), ollama_loaded)
            } else {
                "Offline (127.0.0.1:11434)".into()
            },
            pct_remaining: if ollama_online { 100 } else { 0 },
            badge: if ollama_online { "LOCAL".into() } else { "OFF".into() },
            live_connected: ollama_online,
            dashboard_url: "http://127.0.0.1:11434".into(),
            adjustable: false,
        });
    }

    let repos = scan_local_git_repos(cfg);
    let gh_user = resolve_github_username(cfg, &repos);
    let github = fetch_github_activity(&gh_user);

    DevHubSnapshot {
        providers,
        repos,
        github,
        ollama_models,
        ollama_online,
    }
}

// ==================== 2. PERSISTENT QUICK TO-DO LIST FOR RIGHT SIDEBAR ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: u64,
    pub text: String,
    pub done: bool,
}

pub fn load_todos() -> Vec<TodoItem> {
    let p = app_data_dir().join("todos.json");
    if let Ok(txt) = std::fs::read_to_string(p) {
        if let Ok(items) = serde_json::from_str::<Vec<TodoItem>>(&txt) {
            return items;
        }
    }
    vec![
        TodoItem { id: 1, text: "Revisar commits de hoy en GitHub".into(), done: false },
        TodoItem { id: 2, text: "Probar temas Material You en Ajustes".into(), done: false },
        TodoItem { id: 3, text: "Sesión Pomodoro de 25 min".into(), done: true },
    ]
}

pub fn save_todos(items: &[TodoItem]) {
    let p = app_data_dir().join("todos.json");
    if let Ok(txt) = serde_json::to_string_pretty(items) {
        let _ = std::fs::write(p, txt);
    }
}
