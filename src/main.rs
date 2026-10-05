// popr — Pylind plugin publisher
//
// Usage:
//   popr init [-y] [--dir PATH] [--force]
//   popr version <X.Y.Z | major | minor | patch> [--dir PATH]
//   popr publish [--dir PATH] [--dry-run] [--force]
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
const LOCK_NAME: &str = "plugin.lock";

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
    /// Bump version in plugin.json5
    Version(VersionArgs),
    /// Pack and upload plugin
    Publish(PublishArgs),
    /// Manage API key
    Config(ConfigArgs),
}

#[derive(Args)]
struct InitArgs {
    #[arg(short = 'y', long = "yes")]
    yes: bool,
    #[arg(short = 'd', long = "dir")]
    dir: Option<PathBuf>,
    #[arg(long = "force")]
    force: bool,
}

#[derive(Args)]
struct VersionArgs {
    /// X.Y.Z | major | minor | patch
    target: String,
    #[arg(short = 'd', long = "dir")]
    dir: Option<PathBuf>,
}

#[derive(Args)]
struct PublishArgs {
    #[arg(short = 'd', long = "dir")]
    dir: Option<PathBuf>,
    #[arg(long = "dry-run")]
    dry_run: bool,
    /// Skip version check against server
    #[arg(long = "force")]
    force: bool,
}

#[derive(Args)]
struct ConfigArgs {
    #[arg(short = 'g', long = "global")]
    global: bool,
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
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create directory {}: {}", parent.display(), e))?;
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
// Version helpers
// ============================================================
fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let core = match s.find(['-', '+']) {
        Some(i) => &s[..i],
        None => s,
    };
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let major = parts[0].parse().ok()?;
    let minor = parts[1].parse().ok()?;
    let patch = parts[2].parse().ok()?;
    Some((major, minor, patch))
}

fn is_valid_version(s: &str) -> bool {
    parse_version(s).is_some()
}

fn bump_version(current: &str, target: &str) -> Option<String> {
    if target == "major" || target == "minor" || target == "patch" {
        let (mut major, mut minor, mut patch) = parse_version(current)?;
        match target {
            "major" => {
                major += 1;
                minor = 0;
                patch = 0;
            }
            "minor" => {
                minor += 1;
                patch = 0;
            }
            "patch" => {
                patch += 1;
            }
            _ => unreachable!(),
        }
        return Some(format!("{}.{}.{}", major, minor, patch));
    }

    if parse_version(target).is_none() {
        return None;
    }
    Some(target.to_string())
}

fn compare_versions(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    let (amaj, amin, apat) = parse_version(a)?;
    let (bmaj, bmin, bpat) = parse_version(b)?;
    Some((amaj, amin, apat).cmp(&(bmaj, bmin, bpat)))
}

// ============================================================
// JSON5 helpers
// ============================================================
fn read_manifest(path: &Path) -> Value {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|e| fail(format!("cannot read {}: {}", path.display(), e)));
    json5::from_str(&text)
        .unwrap_or_else(|e| fail(format!("failed to parse {}: {}", MANIFEST_NAME, e)))
}

fn get_str(cfg: &Value, k: &str) -> String {
    cfg.get(k)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// 在 JSON5 文本里替换 `version` 的值。
///
/// 保留：
/// - 前导缩进
/// - 引号风格（单/双）
/// - 行尾逗号
/// - 行尾注释
///
/// 支持写法：`version: "x.y.z"` / `version: 'x.y.z'` / `version = "x.y.z"`
///
/// 返回新内容；找不到 version 行返回 None。
fn replace_version_line(text: &str, new_version: &str) -> Option<String> {
    let mut out = String::new();
    let mut replaced = false;

    for line in text.lines() {
        if !replaced {
            let trimmed_start = line.trim_start();

            if trimmed_start.starts_with("version") {
                // version 后面必须紧跟 : 或 =
                let after_kw = trimmed_start["version".len()..].trim_start();
                let is_sep = after_kw.starts_with(':') || after_kw.starts_with('=');

                if is_sep {
                    // 在整行里找 `:` 或 `=` 的位置
                    if let Some(sep_pos) = line.find(|c| c == ':' || c == '=') {
                        // 分隔符后：找第一个引号
                        let after_sep = &line[sep_pos + 1..];
                        if let Some(open_rel) = after_sep.find(['"', '\'']) {
                            let quote_char = after_sep.as_bytes()[open_rel] as char;
                            let open_abs = sep_pos + 1 + open_rel;

                            // 找同一个引号的闭合位置
                            if let Some(close_rel) = line[open_abs + 1..].find(quote_char) {
                                let close_abs = open_abs + 1 + close_rel;

                                // 前缀：缩进 + version 原文（到开引号前，含分隔符）
                                let prefix = &line[..open_abs]; // 不含开引号

                                // 后缀：从闭引号后开始（含逗号、注释）
                                let suffix = &line[close_abs + quote_char.len_utf8()..];

                                // 缩进
                                let indent: String =
                                    line.chars().take_while(|c| c.is_whitespace()).collect();

                                // 构造新行：保留 prefix 里的 "version:" 或 "version =" 原文
                                // 简单点：直接重新拼成 `version: "x.y.z"`
                                let _ = prefix; // prefix 只用来定位，实际输出重拼

                                out.push_str(&indent);
                                out.push_str("version: ");
                                out.push(quote_char);
                                out.push_str(new_version);
                                out.push(quote_char);
                                out.push_str(suffix); // 保留 , 和注释

                                out.push('\n');
                                replaced = true;
                                continue;
                            }
                        }
                    }
                }
            }
        }

        out.push_str(line);
        out.push('\n');
    }

    if !replaced {
        return None;
    }

    // 原文件结尾无换行，则去掉我们多补的那个
    if !text.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }

    Some(out)
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
    if v.is_empty() {
        default.to_string()
    } else {
        v
    }
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
            Vec::new(),
        )
    } else {
        let id = prompt("id", &default_id);
        let name = prompt("name", &default_name);
        let version = prompt("version", "1.0.0");
        let description = prompt("description", "What this plugin does");
        let entry = prompt("entry", "index.js");
        let files_raw = prompt("files (comma separated, empty = auto)", "");
        let mut files: Vec<String> = files_raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if files.is_empty() {
            files = Vec::new();
        } else if !files.contains(&entry) {
            files.insert(0, entry.clone());
        }
        (id, name, version, description, entry, files)
    };

    let esc = |s: &str| s.replace('"', "\\\"");
    let files_block = if files.is_empty() {
        "        // auto-populated by `popr publish`\n".to_string()
    } else {
        files
            .iter()
            .map(|f| format!("        \"{}\"", esc(f)))
            .collect::<Vec<_>>()
            .join(",\n")
    };

    let content = format!(
        r#"{{
    // publish metadata (used by CLI upload)
    id: "{id}",
    name: "{name}",
    version: "{version}",
    description: "{description}",

    // files/dirs to exclude from packing
    exclude: [
        // ".git",
        // "node_modules",
        // "*.md",
    ],

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
// version
// ============================================================
fn cmd_version(a: VersionArgs) {
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

    // 解析一遍，拿当前 version
    let cfg: Value = json5::from_str(&text)
        .unwrap_or_else(|e| fail(format!("failed to parse {}: {}", MANIFEST_NAME, e)));

    let current = get_str(&cfg, "version");
    if current.is_empty() {
        fail("plugin.json5: missing \"version\"");
    }

    let new_version = bump_version(&current, &a.target).unwrap_or_else(|| {
        fail(format!(
            "invalid version target '{}'. Use X.Y.Z, major, minor, or patch",
            a.target
        ))
    });

    if new_version == current {
        println!("version unchanged: {}", current);
        return;
    }

    // 手写替换，只改 version 那一行，保留原文件其它内容
    let new_text = match replace_version_line(&text, &new_version) {
        Some(t) => t,
        None => fail("could not find version line in plugin.json5"),
    };

    // 校验替换后仍是合法 JSON5（防止改坏文件）
    if let Err(e) = json5::from_str::<Value>(&new_text) {
        fail(format!(
            "replacement produced invalid JSON5 (this is a bug, please report): {}",
            e
        ));
    }

    fs::write(&manifest, new_text)
        .unwrap_or_else(|e| fail(format!("cannot write {}: {}", manifest.display(), e)));

    println!("version: {} -> {}", current, new_version);
}

// ============================================================
// ZIP
// ============================================================
fn build_zip_from_entries(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut buf = io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut buf);
        let opts: SimpleFileOptions = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644);

        for (name, data) in entries {
            zw.start_file(name, opts).map_err(|e| e.to_string())?;
            zw.write_all(data).map_err(|e| e.to_string())?;
        }

        zw.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

fn is_excluded(rel: &str, excludes: &[String]) -> bool {
    if excludes.is_empty() {
        return false;
    }

    let basename = rel.rsplit('/').next().unwrap_or(rel);

    for pat in excludes {
        let pat = pat.trim();
        if pat.is_empty() || pat.starts_with('#') {
            continue;
        }

        let pat = pat.strip_prefix("./").unwrap_or(pat);

        if pat.ends_with('/') {
            let prefix = pat.trim_end_matches('/');
            if rel == prefix || rel.starts_with(&format!("{}/", prefix)) {
                return true;
            }
            continue;
        }

        if rel == pat {
            return true;
        }

        if !pat.contains('/') && !pat.contains('*') && basename == pat {
            return true;
        }

        if pat.contains('*') && !pat.contains('/') && glob_match(pat, basename) {
            return true;
        }

        if pat.contains('/') && pat.contains('*') && glob_match(pat, rel) {
            return true;
        }
    }

    false
}

fn glob_match(pattern: &str, s: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = s.chars().collect();

    fn rec(p: &[char], t: &[char]) -> bool {
        if p.is_empty() {
            return t.is_empty();
        }
        match p[0] {
            '*' => rec(&p[1..], t) || (!t.is_empty() && rec(p, &t[1..])),
            '?' => !t.is_empty() && rec(&p[1..], &t[1..]),
            c => !t.is_empty() && t[0] == c && rec(&p[1..], &t[1..]),
        }
    }

    rec(&p, &t)
}

fn collect_files(dir: &Path, excludes: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let rel = match entry.path().strip_prefix(dir) {
            Ok(r) => r.to_path_buf(),
            Err(_) => continue,
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let s = rel.to_string_lossy().replace('\\', "/");

        let hard_skip = s
            .split('/')
            .any(|c| matches!(c, ".git" | "__MACOSX" | ".DS_Store"))
            || s == ".gitignore"
            || s == LOCK_NAME
            || s == MANIFEST_NAME;

        if hard_skip {
            continue;
        }

        if is_excluded(&s, excludes) {
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

    let mut cfg = read_manifest(&manifest);

    let id = get_str(&cfg, "id");
    let name = get_str(&cfg, "name");
    let version = get_str(&cfg, "version");
    let entry = {
        let e = get_str(&cfg, "entry");
        if e.is_empty() {
            "index.js".to_string()
        } else {
            e
        }
    };
    let description = {
        let d = get_str(&cfg, "description");
        if d.is_empty() {
            get_str(&cfg, "desc")
        } else {
            d
        }
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

    let excludes: Vec<String> = cfg
        .get("exclude")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    let (api_key, source) = if a.dry_run {
        (String::new(), String::new())
    } else {
        load_api_key().unwrap_or_else(|| {
            fail("no API key. Run \"popr config <key>\" (user) or \"sudo popr config -g <key>\" (system)")
        })
    };

    // ---------- 服务器版本检查 ----------
    if !a.dry_run && !a.force {
        println!("checking server version for {}...", id);
        let check_url = format!(
            "{}/check?id={}&t={}",
            api_base(),
            urlencode(&id),
            now_ms()
        );

        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("popr/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_else(|e| fail(format!("http client: {}", e)));

        match client.get(&check_url).header("X-API-Key", &api_key).send() {
            Ok(resp) => {
                let body = resp.text().unwrap_or_default();
                if let Ok(data) = serde_json::from_str::<Value>(&body) {
                    if data.get("success").and_then(|v| v.as_bool()) == Some(true)
                        && data.get("exists").and_then(|v| v.as_bool()) == Some(true)
                    {
                        let remote_version = data
                            .get("version")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();

                        if !remote_version.is_empty() {
                            println!("  server version: {}", remote_version);
                            println!("  local version:  {}", version);

                            if let Some(ord) = compare_versions(&version, &remote_version) {
                                use std::cmp::Ordering::*;
                                match ord {
                                    Less | Equal => {
                                        fail(format!(
                                            "local version {} is not greater than server version {}. \
                                             Bump version first (e.g. `popr version patch`), \
                                             or use --force to override.",
                                            version, remote_version
                                        ));
                                    }
                                    Greater => {
                                        println!("  ok: local is newer");
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("warning: cannot check server version: {}", e);
                eprintln!("         continuing anyway...");
            }
        }
    }

    // ---------- 扫描 ----------
    let all_files: Vec<String> = collect_files(&dir, &excludes)
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();

    let user_files: Vec<String> = cfg
        .get("files")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.replace('\\', "/"))
                .filter(|s| s != LOCK_NAME && s != MANIFEST_NAME)
                .collect()
        })
        .unwrap_or_default();

    let mut final_files: Vec<String> = if user_files.is_empty() {
        all_files.clone()
    } else {
        let mut v: Vec<String> = user_files.clone();
        if !v.contains(&entry) {
            v.insert(0, entry.clone());
        }
        v
    };

    final_files.sort();
    final_files.dedup();

    cfg.as_object_mut().unwrap().insert(
        "files".into(),
        Value::Array(final_files.iter().map(|s| Value::String(s.clone())).collect()),
    );

    if cfg.get("entry").is_none() {
        cfg.as_object_mut()
            .unwrap()
            .insert("entry".into(), Value::String(entry.clone()));
    }

    // ---------- 校验 ----------
    let mut missing: Vec<String> = Vec::new();
    for rel in &final_files {
        if !dir.join(rel).is_file() {
            missing.push(rel.clone());
        }
    }
    if !missing.is_empty() {
        fail(format!(
            "plugin.json5: files list references missing files:\n  {}",
            missing.join("\n  ")
        ));
    }

    // ---------- 写 plugin.lock ----------
    let lock_path = dir.join(LOCK_NAME);
    let lock_json = serde_json::to_string_pretty(&cfg)
        .unwrap_or_else(|e| fail(format!("serialize lock failed: {}", e)));

    let lock_content = format!(
        "// AUTO-GENERATED by popr, do not edit\n// source: {}\n{}\n",
        MANIFEST_NAME, lock_json
    );

    fs::write(&lock_path, &lock_content)
        .unwrap_or_else(|e| fail(format!("cannot write {}: {}", lock_path.display(), e)));

    println!("generated {} ({} files)", LOCK_NAME, final_files.len());

    // ---------- 打包 ----------
    let mut zip_entries: Vec<(String, Vec<u8>)> = Vec::new();
    for rel in &final_files {
        let abs = dir.join(rel);
        let data = fs::read(&abs)
            .unwrap_or_else(|e| fail(format!("cannot read {}: {}", abs.display(), e)));
        zip_entries.push((rel.clone(), data));
    }
    zip_entries.push((
        MANIFEST_NAME.to_string(),
        lock_content.as_bytes().to_vec(),
    ));

    let zip_bytes = build_zip_from_entries(&zip_entries)
        .unwrap_or_else(|e| fail(format!("zip failed: {}", e)));

    if a.dry_run {
        let out = dir.join(format!("{}.zip", id));
        fs::write(&out, &zip_bytes)
            .unwrap_or_else(|e| fail(format!("cannot write {}: {}", out.display(), e)));
        println!(
            "dry-run: wrote {} ({}, {} files)",
            out.display(),
            format_size(zip_bytes.len() as u64),
            zip_entries.len()
        );
        return;
    }

    // ---------- 上传 ----------
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

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
            out.push(c);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
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

        let mut cfg =
            read_config_file(&target).unwrap_or_else(|| Value::Object(Default::default()));
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
        Cmd::Version(a) => cmd_version(a),
        Cmd::Publish(a) => cmd_publish(a),
        Cmd::Config(a) => cmd_config(a),
    }
}
