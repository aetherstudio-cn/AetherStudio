//! 网络安全检查（离线优先，用户触发才联网）。
//!
//! 四项检查：
//! 1. 工作区密钥扫描：逐行模式匹配（私钥块 / AWS AK / 常见 API Key·口令赋值），
//!    报告 file:line + 掩码片段；跳过 .git/target/node_modules 与二进制/超大文件。
//! 2. 依赖审计：解析 Cargo.lock；优先调用本机 `cargo audit`，缺失时经 OSV API
//!    （api.osv.dev）批量查询已知漏洞（仅用户显式触发时联网）。
//! 3. AI 端点 TLS：base_url 明文 http 告警 + https 证书链校验探测（ureq/webpki）。
//! 4. 插件权限审计：当前构建未链接 aether-plugin 运行时，无 L4_System 授权记录。
//!
//! 设计约束：不新增 crate（模式匹配替代 regex、ureq 已在依赖中）。

use std::path::{Path, PathBuf};

/// 密钥扫描命中
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretFinding {
    /// 相对工作区路径
    pub file: String,
    /// 1-based 行号
    pub line: usize,
    /// 命中类型（人类可读）
    pub kind: &'static str,
    /// 掩码后的值片段（绝不输出完整密钥）
    pub masked: String,
}

/// 单个文件扫描结果上限（防止巨型日志拖垮扫描）
const MAX_FILE_BYTES: u64 = 1_000_000;
/// 全局命中上限
const MAX_FINDINGS: usize = 200;
/// 跳过的目录名
const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", ".qoder", ".trae"];
/// 跳过的二进制扩展名
const SKIP_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "webp", "tiff", "zip", "gz", "7z", "exe", "dll",
    "pdb", "so", "dylib", "bin", "pdf", "woff", "woff2", "ttf", "otf", "mp3", "mp4", "wav",
];

/// 掩码：保留首 3 与末 2 字符，中间以 * 代替；过短时只留首 1。
pub fn mask_secret(v: &str) -> String {
    let chars: Vec<char> = v.chars().collect();
    let n = chars.len();
    if n <= 4 {
        return "*".repeat(n.max(1));
    }
    if n <= 10 {
        return format!("{}***{}", chars[0], chars[n - 1]);
    }
    let head: String = chars[..3].iter().collect();
    let tail: String = chars[n - 2..].iter().collect();
    format!("{}***{}", head, tail)
}

/// 占位/示例值排除（避免把文档模板当泄漏）
fn is_placeholder(v: &str) -> bool {
    let low = v.to_ascii_lowercase();
    low.is_empty()
        || low.contains("xxx")
        || low.contains("***")
        || low.contains("your_")
        || low.contains("your-")
        || low.contains("example")
        || low.contains("placeholder")
        || low.contains("changeme")
        || low.contains("dummy")
        || low.contains("redacted")
        || low.starts_with('<')
        || low.starts_with("${")
        || low.starts_with("env(")
        || low.starts_with("$env:")
        || low.starts_with("process.env")
}

/// 键名是否像"敏感配置键"
fn is_secret_key_name(name: &str) -> bool {
    let low = name.to_ascii_lowercase();
    const KEYS: &[&str] = &[
        "api_key",
        "apikey",
        "api-key",
        "secret",
        "token",
        "password",
        "passwd",
        "pwd",
        "access_key",
        "private_key",
    ];
    KEYS.iter().any(|k| low.contains(k))
}

/// 扫描单行文本，返回 (类型, 掩码值)。纯函数，可单测。
pub fn scan_line_for_secret(line: &str) -> Option<(&'static str, String)> {
    // 1) 私钥块起始
    if line.contains("-----BEGIN ") && line.contains("PRIVATE KEY-----") {
        return Some((
            "私钥块（PRIVATE KEY）",
            "-----BEGIN PRIVATE KEY-----".to_string(),
        ));
    }
    // 2) AWS Access Key：AKIA + 16 位大写字母/数字
    if let Some(pos) = line.find("AKIA") {
        let rest = &line[pos + 4..];
        let take: String = rest
            .chars()
            .take(16)
            .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            .collect();
        if take.len() == 16 {
            return Some(("AWS Access Key", mask_secret(&format!("AKIA{}", take))));
        }
    }
    // 3) 常见平台前缀 token：sk- / ghp_ / github_pat_
    for prefix in ["sk-", "ghp_", "github_pat_"] {
        if let Some(pos) = line.find(prefix) {
            let rest = &line[pos + prefix.len()..];
            let take: String = rest
                .chars()
                .take(64)
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if take.len() >= 16 {
                return Some(("API Token", mask_secret(&format!("{}{}", prefix, take))));
            }
        }
    }
    // 4) key = "value" / key: value 形式的赋值
    let lower = line.to_ascii_lowercase();
    let sep = lower.find('=').or_else(|| lower.find(':'));
    if let Some(sep_pos) = sep {
        // 键名取分隔符前的最后一个标识符
        let key_part = &lower[..sep_pos];
        let name: String = key_part
            .trim()
            .rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
            .next()
            .unwrap_or("")
            .to_string();
        if is_secret_key_name(&name) {
            let raw_val = line[sep_pos + 1..]
                .trim()
                .trim_start_matches(['"', '\'', '`'])
                .trim_end_matches([',', ';']);
            let val: String = raw_val
                .chars()
                .take_while(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | '`' | ')' | '}'))
                .collect();
            if val.len() >= 8 && !is_placeholder(&val) {
                return Some(("疑似硬编码密钥/口令", mask_secret(&val)));
            }
        }
    }
    None
}

/// 工作区密钥扫描报告
#[derive(Debug, Default)]
pub struct SecretScanReport {
    pub scanned_files: usize,
    pub findings: Vec<SecretFinding>,
    pub exceeded_cap: bool,
}

/// 递归扫描工作区（跳过二进制/依赖目录/超大文件）。
pub fn scan_workspace_secrets(root: &Path) -> SecretScanReport {
    let mut report = SecretScanReport::default();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !SKIP_DIRS.iter().any(|d| *d == name) {
                    stack.push(path);
                }
                continue;
            }
            if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
                continue;
            }
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if SKIP_EXTS.iter().any(|x| *x == ext) {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            // 二进制嗅探：前 1KB 含 NUL 视为二进制
            if bytes[..bytes.len().min(1024)].contains(&0) {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            report.scanned_files += 1;
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            for (idx, line) in text.lines().enumerate() {
                if let Some((kind, masked)) = scan_line_for_secret(line) {
                    report.findings.push(SecretFinding {
                        file: rel.clone(),
                        line: idx + 1,
                        kind,
                        masked,
                    });
                    if report.findings.len() >= MAX_FINDINGS {
                        report.exceeded_cap = true;
                        return report;
                    }
                }
            }
        }
    }
    report
}

/// Cargo.lock 包条目
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockPackage {
    pub name: String,
    pub version: String,
}

/// 解析 Cargo.lock（纯函数）：按 `[[package]]` 块提取 name/version。
pub fn parse_cargo_lock(text: &str) -> Vec<LockPackage> {
    let mut out = Vec::new();
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut in_package = false;
    let flush =
        |name: &mut Option<String>, version: &mut Option<String>, out: &mut Vec<LockPackage>| {
            if let (Some(n), Some(v)) = (name.take(), version.take()) {
                out.push(LockPackage {
                    name: n,
                    version: v,
                });
            }
        };
    for line in text.lines() {
        let t = line.trim();
        if t == "[[package]]" {
            if in_package {
                flush(&mut name, &mut version, &mut out);
            }
            in_package = true;
            continue;
        }
        if !in_package {
            continue;
        }
        if t.starts_with('[') {
            // 进入其他块（如 [metadata]）
            flush(&mut name, &mut version, &mut out);
            in_package = false;
            continue;
        }
        if let Some(v) = t.strip_prefix("name = ") {
            name = Some(v.trim_matches('"').to_string());
        } else if let Some(v) = t.strip_prefix("version = ") {
            version = Some(v.trim_matches('"').to_string());
        }
    }
    if in_package {
        flush(&mut name, &mut version, &mut out);
    }
    out
}

/// 依赖审计（用户触发才联网）：优先本机 cargo audit，缺失时走 OSV 批量查询。
pub fn audit_dependencies(root: &Path) -> String {
    let lock_path = root.join("Cargo.lock");
    let Ok(lock_text) = std::fs::read_to_string(&lock_path) else {
        return "依赖审计：未找到 Cargo.lock（仅支持 Cargo 工作区）".to_string();
    };
    let packages = parse_cargo_lock(&lock_text);
    let mut out = format!("依赖审计：Cargo.lock 共 {} 个包\n", packages.len());

    // 1) 本机 cargo audit
    if let Ok(ver) = std::process::Command::new("cargo")
        .args(["audit", "--version"])
        .output()
    {
        if ver.status.success() {
            match std::process::Command::new("cargo")
                .arg("audit")
                .current_dir(root)
                .output()
            {
                Ok(o) => {
                    let text = format!(
                        "{}{}",
                        String::from_utf8_lossy(&o.stdout),
                        String::from_utf8_lossy(&o.stderr)
                    );
                    let summary: Vec<&str> = text
                        .lines()
                        .filter(|l| {
                            l.contains("vulnerabilit")
                                || l.contains("warning")
                                || l.contains("error")
                        })
                        .take(10)
                        .collect();
                    out.push_str("cargo audit 已运行：\n");
                    if summary.is_empty() {
                        out.push_str(&format!("  （退出码 {}，未见漏洞摘要）\n", o.status));
                    } else {
                        for l in summary {
                            out.push_str(&format!("  {}\n", l.trim()));
                        }
                    }
                    return out;
                }
                Err(e) => {
                    out.push_str(&format!("cargo audit 调用失败：{}（转 OSV 查询）\n", e));
                }
            }
        }
    }

    // 2) OSV 批量查询（离线/网络失败时如实提示）
    match query_osv_batch(&packages) {
        Ok(report) => out.push_str(&report),
        Err(e) => out.push_str(&format!(
            "本机无 cargo audit 且 OSV 查询失败（{}）；可在联网环境安装 cargo-audit 后重试\n",
            e
        )),
    }
    out
}

/// OSV 批量漏洞查询（api.osv.dev，一次请求覆盖全部依赖）
fn query_osv_batch(packages: &[LockPackage]) -> Result<String, String> {
    if packages.is_empty() {
        return Ok("OSV：无依赖需要查询".to_string());
    }
    let queries: Vec<serde_json::Value> = packages
        .iter()
        .map(|p| {
            serde_json::json!({
                "package": {"name": p.name, "ecosystem": "crates.io"},
                "version": p.version,
            })
        })
        .collect();
    let body = serde_json::json!({ "queries": queries });
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(8))
        .build();
    let resp = agent
        .post("https://api.osv.dev/v1/querybatch")
        .send_json(&body)
        .map_err(|e| format!("OSV 请求失败：{}", e))?;
    let json: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("OSV 响应解析失败：{}", e))?;
    let mut hits = 0usize;
    let mut out = String::from("OSV 查询结果：\n");
    if let Some(results) = json.get("results").and_then(|r| r.as_array()) {
        for (i, r) in results.iter().enumerate() {
            let vulns = r.get("vulns").and_then(|v| v.as_array());
            if let Some(vs) = vulns {
                for v in vs {
                    let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("?");
                    let summary = v.get("summary").and_then(|x| x.as_str()).unwrap_or("");
                    let pkg = packages.get(i).map(|p| p.name.as_str()).unwrap_or("?");
                    out.push_str(&format!("  ⚠ {} {}：{} {}\n", pkg, "", id, summary));
                    hits += 1;
                }
            }
        }
    }
    if hits == 0 {
        out.push_str("  未发现已知漏洞\n");
    } else {
        out.push_str(&format!("  共 {} 条漏洞命中\n", hits));
    }
    Ok(out)
}

/// AI 端点 TLS 检查：http 明文告警；https 做一次握手探测（证书链由 webpki 校验）。
pub fn check_ai_endpoint(base_url: &str) -> String {
    let url = base_url.trim();
    if url.is_empty() {
        return "AI 端点 TLS：未配置 base_url（使用提供商默认 https 端点）".to_string();
    }
    let lower = url.to_ascii_lowercase();
    if !lower.starts_with("https://") {
        return format!(
            "AI 端点 TLS：⚠ 当前端点 `{}` 非 https，API Key 与对话内容将以明文传输，建议改用 https",
            url
        );
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(4))
        .build();
    match agent.get(url).call() {
        Ok(_) => format!("AI 端点 TLS：✓ `{}` 证书链校验通过（可连通）", url),
        Err(ureq::Error::Status(code, _)) => format!(
            "AI 端点 TLS：✓ `{}` 证书链校验通过（HTTP {}，端点可达）",
            url, code
        ),
        Err(e) => {
            let msg = e.to_string();
            if msg.to_ascii_lowercase().contains("certificate")
                || msg.to_ascii_lowercase().contains("tls")
            {
                format!("AI 端点 TLS：✕ `{}` 证书校验失败：{}", url, msg)
            } else {
                format!(
                    "AI 端点 TLS：? `{}` 未能完成探测（{}）；证书链未验证，请检查网络/配置",
                    url, msg
                )
            }
        }
    }
}

/// 插件权限审计：当前构建未链接 aether-plugin 运行时。
pub fn audit_plugins() -> String {
    "插件权限审计：当前构建未集成插件运行时（aether-plugin 未链接），无 L4_System 授权记录"
        .to_string()
}

/// 统一入口：按 scope 执行（all | secrets | deps | tls | plugins）。
/// `root` 为空表示未打开工作区；`base_url` 为当前激活模型的 API 端点。
pub fn run_security_scan(root: Option<&Path>, base_url: Option<&str>, scope: &str) -> String {
    let scope = scope.trim().to_ascii_lowercase();
    let scope = if scope.is_empty() {
        "all".to_string()
    } else {
        scope
    };
    let want = |name: &str| scope == "all" || scope == name;
    let mut sections: Vec<String> = Vec::new();
    sections.push(format!("## 网络安全检查报告（范围：{}）", scope));

    if want("secrets") {
        sections.push("### 工作区密钥扫描".to_string());
        match root {
            Some(root) => {
                let r = scan_workspace_secrets(root);
                let mut s = format!(
                    "扫描 {} 个文本文件，发现 {} 处疑似凭据\n",
                    r.scanned_files,
                    r.findings.len()
                );
                for f in r.findings.iter().take(50) {
                    s.push_str(&format!(
                        "- `{}`:{} {}：`{}`\n",
                        f.file, f.line, f.kind, f.masked
                    ));
                }
                if r.findings.len() > 50 {
                    s.push_str(&format!(
                        "（其余 {} 处已省略，建议优先处理前 50 条）\n",
                        r.findings.len() - 50
                    ));
                }
                if r.exceeded_cap {
                    s.push_str(&format!("（命中达上限 {}，扫描提前结束）\n", MAX_FINDINGS));
                }
                sections.push(s.trim_end().to_string());
            }
            None => sections.push("未打开工作区，跳过".to_string()),
        }
    }
    if want("deps") {
        sections.push("### 依赖审计".to_string());
        match root {
            Some(root) => sections.push(audit_dependencies(root)),
            None => sections.push("未打开工作区，跳过".to_string()),
        }
    }
    if want("tls") {
        sections.push("### AI 端点 TLS".to_string());
        sections.push(check_ai_endpoint(base_url.unwrap_or("")));
    }
    if want("plugins") {
        sections.push("### 插件权限审计".to_string());
        sections.push(audit_plugins());
    }
    sections.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_secret_shapes() {
        assert_eq!(mask_secret("ab"), "**");
        assert_eq!(mask_secret("abcdefgh"), "a***h");
        assert_eq!(mask_secret("abcdefghijk"), "abc***jk");
        // 掩码后绝不包含完整原值
        assert!(!mask_secret("abcdefghijk").contains("defghi"));
    }

    #[test]
    fn test_scan_line_private_key_block() {
        let hit = scan_line_for_secret("-----BEGIN RSA PRIVATE KEY-----").unwrap();
        assert_eq!(hit.0, "私钥块（PRIVATE KEY）");
    }

    #[test]
    fn test_scan_line_aws_key() {
        let hit = scan_line_for_secret("aws_key = AKIAIOSFODNN7EXAMPLE").unwrap();
        assert_eq!(hit.0, "AWS Access Key");
        assert!(hit.1.starts_with("AKI"), "{}", hit.1);
        assert!(!hit.1.contains("IOSFODNN7EXAMPLE"), "{}", hit.1);
        // 长度不足不命中
        assert!(scan_line_for_secret("AKIA123").is_none());
    }

    #[test]
    fn test_scan_line_prefixed_tokens() {
        assert_eq!(
            scan_line_for_secret("let k = \"sk-abcdefghijklmnopqrstuvwx\";")
                .unwrap()
                .0,
            "API Token"
        );
        assert_eq!(
            scan_line_for_secret("token: ghp_abcdefghijklmnopqrstuvwxyz1234")
                .unwrap()
                .0,
            "API Token"
        );
        // 过短不命中
        assert!(scan_line_for_secret("sk-short").is_none());
    }

    #[test]
    fn test_scan_line_assignment_and_placeholders() {
        let hit = scan_line_for_secret("api_key = \"abcd1234efgh5678\"").unwrap();
        assert_eq!(hit.0, "疑似硬编码密钥/口令");
        // 占位符排除
        assert!(scan_line_for_secret("api_key = \"your_api_key_here\"").is_none());
        assert!(scan_line_for_secret("password: \"******\"").is_none());
        assert!(scan_line_for_secret("token = \"${TOKEN}\"").is_none());
        // 非敏感键名不命中
        assert!(scan_line_for_secret("let count = 12345678;").is_none());
        // 过短值不命中
        assert!(scan_line_for_secret("name: short").is_none());
    }

    #[test]
    fn test_parse_cargo_lock() {
        let text = r#"
version = 3

[[package]]
name = "serde"
version = "1.0.200"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "windows"
version = "0.58.0"

[metadata]
"key" = "value"
"#;
        let pkgs = parse_cargo_lock(text);
        assert_eq!(pkgs.len(), 2);
        assert_eq!(
            pkgs[0],
            LockPackage {
                name: "serde".into(),
                version: "1.0.200".into()
            }
        );
        assert_eq!(
            pkgs[1],
            LockPackage {
                name: "windows".into(),
                version: "0.58.0".into()
            }
        );
        assert!(parse_cargo_lock("").is_empty());
    }

    #[test]
    fn test_check_ai_endpoint_offline_shapes() {
        assert!(check_ai_endpoint("").contains("未配置"));
        assert!(check_ai_endpoint("http://api.example.com").contains("明文"));
        assert!(check_ai_endpoint("https://127.0.0.1:1").contains("https://127.0.0.1:1"));
    }

    #[test]
    fn test_run_security_scan_scopes() {
        let s = run_security_scan(None, None, "plugins");
        assert!(s.contains("插件权限审计"), "{}", s);
        assert!(!s.contains("密钥扫描"), "{}", s);
        let s2 = run_security_scan(None, Some("http://x"), "all");
        assert!(s2.contains("未打开工作区"), "{}", s2);
        assert!(s2.contains("明文"), "{}", s2);
    }
}
