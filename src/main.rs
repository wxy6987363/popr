// popr — Pylind plugin publisher
//
// Usage:
//   popr init [-y] [--dir PATH] [--force]
//   popr publish [--dir PATH] [--dry-run]
//   popr config <KEY>
//   popr config -g <KEY>
//   popr config
//   popr --help

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};
use serde_json::Value;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

// ============================================================
// Constants
// ============================================================
const DEFAULT_API: &str = "https://pylind.pages.dev/api/plugins";
const MANIFEST_NAME: &str = "plugin.json5";

fn api_base() -> String {
    std::env::var("PYLIND_API").unwrap_or_else(|_| DEFAULT_API.to_string())
}

fn user_config_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".pylindrc")
}

fn system_config_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into());
        PathBuf::from(base).join("pylind").join("popr.json")
    }
    #[cfg(not(target_os = "windows"))]
    {
        PathBuf::from("/etc/pylind/popr.json")
    }
}

// ============================================================
// CLI
// ============================================================
#[derive(Parser)]
#[command(name = "popr", version, about = "Pylind plugin publisher")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create plugin.json5
    Init(InitArgs),
    /// Pack and upload plugin
    Publish(PublishArgs),
    /// Manage API key
    Config(ConfigArgs),
}

#[derive(Args)]
struct InitArgs {
    /// Use defaults, no prompts
    #[arg(short = 'y', long = "yes")]
    yes: bool,

    /// Target directory (default: cwd)
    #[arg(short = 'd', long = "dir")]
    dir: Option<PathBuf>,

    /// Overwrite if exists
    #[arg(long = "force")]
    force: bool,
}

#[derive(Args)]
struct PublishArgs {
    /// Plugin directory (default: cwd)
    #[arg(short = 'd', long = "dir")]
    dir: Option<PathBuf>,

    /// Pack only, do not upload
    #[arg(long = "dry-run")]
    dry_run: bool,
}

#[derive(Args)]
struct ConfigArgs {
    /// Save to system config (may require admin)
    #[arg(short = 'g', long = "global")]
    global: bool,

    /// API key. If omitted, show current configuration.
    key: Option<String>,
}

// ============================================================
// Errors
// ============================================================
fn fail(msg: impl AsRef<str>) -> ! {
    eprintln!("error: {}", msg.as_ref());
    std::process::exit(1);
}

// ============================================================
// Config file I/O
// ============================================================
fn read_config_file(p: &Path) -> Option<Value> {
    let s = fs::read_to_string(p).ok()?;
    serde_json::from_str(&s).ok()
}

fn write_config_file(p: &Path, v: &Value) -> Result<(), String> {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            format!("cannot create directory {}: {}", parent.display(), e)
        })?;
    }
    let s = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    fs::write(p, s).map_err(|e| format!("cannot write {}: {}", p.display(), e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(p, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Resolve API key: env > user config > system config.
fn load_api_key() -> Option<(String, String)> {
    if let Ok(k) = std::env::var("PYLIND_API_KEY") {
        if !k.is_empty() {
            return Some((k, "PYLIND_API_KEY".into()));
        }
    }

    let user = user_config_path();
    if let Some(cfg) = read_config_file(&user) {
        if let Some(k) = cfg.get("apiKey").and_then(|v| v.as_str()) {
            if !k.is_empty() {
                return Some((k.to_string(), user.display().to_string()));
            }
        }
    }

    let sys = system_config_path();
    if let Some(cfg) = read_config_file(&sys) {
        if let Some(k) = cfg.get("apiKey").and_then(|v| v.as_str()) {
            if !k.is_empty() {
                return Some((k.to_string(), sys.display().to_string()));
            }
        }
    }

    None
}

fn mask_key(k: &str) -> String {
    if k.len() <= 12 {
        return k.to_string();
    }
    format!("{}…{}", &k[..8], &k[k.len() - 4..])
}

// ============================================================
// init
// ============================================================
fn prompt(label: &str, default: &str) -> String {
    let hint = if default.is_empty() {
        String::new()
    } else {
        format!(" ({})", default)
    };
    print!("{}{}: ", label, hint);
    io::stdout().flush().ok();
    let mut s = String::new();
    io::stdin().read_line(&mut s).ok();
    let v = s.trim().to_string();
    if v.is_empty() { default.to_string() } else { v }
}

fn cmd_init(a: InitArgs) {
    let dir = a.dir.unwrap_or_else(|| std::env::current_dir().unwrap());
    if !dir.is_dir() {
        fail(format!("not a directory: {}", dir.display()));
    }

    let manifest = dir.join(MANIFEST_NAME);
    if manifest.exists() && !a.force {
        fail(format!(
            "{} already exists: {} (use --force)",
            MANIFEST_NAME,
            manifest.display()
        ));
    }

    let folder = dir.file_name().and_then(|s| s.to_str()).unwrap_or("plugin");
    let slug: String = folder
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').to_string();

    let default_id = if slug.is_empty() {
        "com.example.my-plugin".into()
    } else {
        format!("com.example.{}", slug)
    };

    let default_name = if slug.is_empty() {
        "My Plugin".into()
    } else {
        slug.replace(['-', '_'], " ")
            .split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };

    let (id, name, version, description, entry, files): (
        String,
        String,
        String,
        String,
        String,
        Vec<String>,
    ) = if a.yes {
        (
            default_id,
            default_name,
            "1.0.0".into(),
            "What this plugin does".into(),
            "index.js".into(),
            vec!["index.js".into()],
        )
    } else {
        let id = prompt("id", &default_id);
        let name = prompt("name", &default_name);
        let version = prompt("version", "1.0.0");
        let description = prompt("description", "What this plugin does");
        let entry = prompt("entry", "index.js");
        let files_raw = prompt("files (comma separated)", "index.js");
        let mut files: Vec<String> = files_raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if files.is_empty() {
            files.push(entry.clone());
        }
        if !files.contains(&entry) {
            files.insert(0, entry.clone());
        }
        (id, name, version, description, entry, files)
    };

    let esc = |s: &str| s.replace('"', "\\\"");
    let files_block = files
        .iter()
        .map(|f| format!("        \"{}\"", esc(f)))
        .collect::<Vec<_>>()
        .join(",\n");

    let content = format!(
        r#"{{
    // publish metadata (used by CLI upload)
    id: "{id}",
    name: "{name}",
    version: "{version}",
    description: "{description}",

    // client load metadata
    entry: "{entry}",
    files: [
{files_block}
    ]
}}
"#,
        id = esc(&id),
        name = esc(&name),
        version = esc(&version),
        description = esc(&description),
        entry = esc(&entry),
        files_block = files_block,
    );

    fs::write(&manifest, content)
        .unwrap_or_else(|e| fail(format!("cannot write {}: {}", manifest.display(), e)));

    if !a.yes {
        println!("created {}", manifest.display());
    }
}

// ============================================================
// ZIP
// ============================================================
fn build_zip(dir: &Path, files: &[PathBuf]) -> Result<Vec<u8>, String> {
    let mut buf = io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut buf);
        let opts: SimpleFileOptions = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644);

        for rel in files {
            let abs = dir.join(rel);
            let name = rel.to_string_lossy().replace('\\', "/");
            zw.start_file(name, opts).map_err(|e| e.to_string())?;
            let data = fs::read(&abs)
                .map_err(|e| format!("cannot read {}: {}", abs.display(), e))?;
            zw.write_all(&data).map_err(|e| e.to_string())?;
        }

        zw.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

fn collect_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let rel = match entry.path().strip_prefix(dir) {
            Ok(r) => r.to_path_buf(),
            Err(_) => continue,
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let s = rel.to_string_lossy();
        let skip = s
            .split('/')
            .any(|c| matches!(c, "node_modules" | ".git" | "__MACOSX" | ".DS_Store"))
            || s == ".gitignore";
        if skip {
            continue;
        }
        if entry.file_type().is_file() {
            out.push(rel);
        }
    }
    out.sort();
    out
}

fn format_size(n: u64) -> String {
    if n < 1024 {
        format!("{} B", n)
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.2} MB", n as f64 / 1024.0 / 1024.0)
    }
}

// ============================================================
// publish
// ============================================================
fn cmd_publish(a: PublishArgs) {
    let dir = a.dir.unwrap_or_else(|| std::env::current_dir().unwrap());
    let manifest = dir.join(MANIFEST_NAME);

    if !manifest.is_file() {
        fail(format!(
            "{} not found in {} (run \"popr init\" first)",
            MANIFEST_NAME,
            dir.display()
        ));
    }

    let text = fs::read_to_string(&manifest)
        .unwrap_or_else(|e| fail(format!("cannot read manifest: {}", e)));
    let cfg: Value = json5::from_str(&text)
        .unwrap_or_else(|e| fail(format!("failed to parse {}: {}", MANIFEST_NAME, e)));

    let get = |k: &str| -> String {
        cfg.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };

    let id = get("id");
    let name = get("name");
    let version = get("version");
    let description = {
        let d = get("description");
        if d.is_empty() { get("desc") } else { d }
    };

    if id.is_empty() {
        fail("plugin.json5: missing \"id\"");
    }
    if name.is_empty() {
        fail("plugin.json5: missing \"name\"");
    }
    if version.is_empty() {
        fail("plugin.json5: missing \"version\"");
    }
    if !is_valid_id(&id) {
        fail("plugin.json5: invalid \"id\" (must start alnum, [a-zA-Z0-9._-], 2-64 chars)");
    }
    if !is_valid_version(&version) {
        fail("plugin.json5: invalid \"version\" (e.g. 1.0.0)");
    }

    let files = collect_files(&dir);
    if files.is_empty() {
        fail("no files to pack");
    }
    if !files.iter().any(|f| f.to_string_lossy() == MANIFEST_NAME) {
        fail(format!("zip must contain {}", MANIFEST_NAME));
    }

    let zip_bytes =
        build_zip(&dir, &files).unwrap_or_else(|e| fail(format!("zip failed: {}", e)));

    if a.dry_run {
        let out = dir.join(format!("{}.zip", id));
        fs::write(&out, &zip_bytes)
            .unwrap_or_else(|e| fail(format!("cannot write {}: {}", out.display(), e)));
        println!(
            "dry-run: wrote {} ({} , {} files)",
            out.display(),
            format_size(zip_bytes.len() as u64),
            files.len()
        );
        return;
    }

    let (api_key, source) = load_api_key().unwrap_or_else(|| {
        fail(
            "no API key. Run \"popr config <key>\" (user) or \"sudo popr config -g <key>\" (system)",
        )
    });

    println!("publishing {}@{} via {}", id, version, source);

    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("popr/", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap_or_else(|e| fail(format!("http client: {}", e)));

    let part = reqwest::blocking::multipart::Part::bytes(zip_bytes)
        .file_name(format!("{}.zip", id))
        .mime_str("application/zip")
        .unwrap();

    let form = reqwest::blocking::multipart::Form::new()
        .text("id", id.clone())
        .text("name", name.clone())
        .text("desc", description.clone())
        .text("version", version.clone())
        .part("zip", part);

    let url = format!("{}/upload", api_base());
    let resp = client
        .post(&url)
        .header("X-API-Key", api_key)
        .multipart(form)
        .send()
        .unwrap_or_else(|e| fail(format!("network error: {}", e)));

    let status = resp.status();
    let body = resp.text().unwrap_or_default();

    let data: Value = serde_json::from_str(&body).unwrap_or_else(|_| {
        fail(format!(
            "server returned non-JSON (HTTP {}): {}",
            status,
            body.chars().take(300).collect::<String>()
        ))
    });

    if data.get("success").and_then(|v| v.as_bool()) != Some(true) {
        let err = data
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        fail(format!("upload failed: {}", err));
    }

    let action = data
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("done");
    let rid = data.get("id").and_then(|v| v.as_str()).unwrap_or(&id);
    let rver = data
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or(&version);
    println!("ok: {} {}@{}", action, rid, rver);
}

fn is_valid_id(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 2 || bytes.len() > 64 {
        return false;
    }
    let first = bytes[0] as char;
    if !first.is_ascii_alphanumeric() {
        return false;
    }
    bytes.iter().all(|&b| {
        let c = b as char;
        c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-'
    })
}

fn is_valid_version(s: &str) -> bool {
    let re_like = |part: &str| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit());
    let (core, suffix) = match s.find(['-', '+']) {
        Some(i) => (&s[..i], Some(&s[i..])),
        None => (s, None),
    };
    let dots: Vec<&str> = core.split('.').collect();
    if dots.is_empty() || dots.len() > 4 {
        return false;
    }
    if !dots.iter().all(|d| re_like(d)) {
        return false;
    }
    if let Some(suf) = suffix {
        if suf.len() < 2 {
            return false;
        }
        if !suf[1..]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            return false;
        }
    }
    true
}

// ============================================================
// config
// ============================================================
fn cmd_config(a: ConfigArgs) {
    if let Some(key) = a.key {
        let target = if a.global {
            system_config_path()
        } else {
            user_config_path()
        };

        let mut cfg = read_config_file(&target)
            .unwrap_or_else(|| Value::Object(Default::default()));
        if !cfg.is_object() {
            cfg = Value::Object(Default::default());
        }
        cfg.as_object_mut()
            .unwrap()
            .insert("apiKey".into(), Value::String(key));
        write_config_file(&target, &cfg).unwrap_or_else(|e| fail(e));
        println!("saved to {}", target.display());
        return;
    }

    let user = user_config_path();
    let sys = system_config_path();

    let user_key: Option<String> = read_config_file(&user)
        .and_then(|c| c.get("apiKey").and_then(|v| v.as_str()).map(|s| s.to_string()));

    let sys_key: Option<String> = read_config_file(&sys)
        .and_then(|c| c.get("apiKey").and_then(|v| v.as_str()).map(|s| s.to_string()));

    let user_active = user_key.is_some();
    let sys_active = !user_active && sys_key.is_some();

    match &user_key {
        Some(k) => {
            println!("user   {}", user.display());
            println!("       {} (active)", mask_key(k));
        }
        None => println!("user   {} (unset)", user.display()),
    }

    match &sys_key {
        Some(k) => {
            println!("system {}", sys.display());
            println!(
                "       {}{}",
                mask_key(k),
                if sys_active { " (active)" } else { "" }
            );
        }
        None => println!("system {} (unset)", sys.display()),
    }
}

// ============================================================
// main
// ============================================================
fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Init(a) => cmd_init(a),
        Cmd::Publish(a) => cmd_publish(a),
        Cmd::Config(a) => cmd_config(a),
    }
}
