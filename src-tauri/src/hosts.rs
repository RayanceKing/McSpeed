use crate::config::{LEGACY_MARKERS, LOOPBACK_V4, LOOPBACK_V6, MARKER};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// hosts 默认路径。
pub fn default_hosts_path() -> PathBuf {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    Path::new(&windir)
        .join("System32")
        .join("drivers")
        .join("etc")
        .join("hosts")
}

/// hosts 中的一行记录。
#[derive(Debug, Clone, Serialize)]
pub struct HostsEntry {
    pub ip: String,
    pub domain: String,
    pub managed: bool,
    pub comment: Option<String>,
}

/// 读写结果。
#[derive(Debug, Clone, Serialize)]
pub struct HostsApplyResult {
    pub added: Vec<HostsEntry>,
    pub removed: Vec<HostsEntry>,
    pub backup_path: Option<String>,
}

/// 判断一行是否为 MC Speed 标记行。
pub fn is_managed_line(line: &str) -> bool {
    line.contains(MARKER) || LEGACY_MARKERS.iter().any(|m| line.contains(m))
}

/// 判断注释文本是否由本工具写入（兼容改名前的旧标记）。
fn is_managed_comment(comment: &str) -> bool {
    comment.starts_with("MC Speed") || comment.starts_with("McSpeed")
}

/// 解析单行。非记录行返回 None。
pub fn parse_line(line: &str) -> Option<HostsEntry> {
    if line.trim().is_empty() {
        return None;
    }

    let comment_index = line.find('#');
    let (content, comment) = match comment_index {
        Some(i) => (&line[..i], Some(line[i + 1..].trim().to_string())),
        None => (line, None),
    };

    let parts: Vec<&str> = content
        .split([' ', '\t'])
        .filter(|s| !s.is_empty())
        .collect();

    if parts.len() < 2 {
        return None;
    }

    // 第一列必须是合法 IP
    if parts[0].parse::<std::net::IpAddr>().is_err() {
        return None;
    }

    let managed = comment
        .as_deref()
        .map(is_managed_comment)
        .unwrap_or(false);

    Some(HostsEntry {
        ip: parts[0].to_string(),
        domain: parts[1].to_string(),
        managed,
        comment,
    })
}

/// 读取全部 managed 条目。
pub fn read_managed(path: &Path) -> std::io::Result<Vec<HostsEntry>> {
    let content = fs::read_to_string(path)?;
    Ok(content
        .lines()
        .filter_map(parse_line)
        .filter(|e| e.managed)
        .collect())
}

/// 用给定条目替换所有 managed 行，保留其余内容。
pub fn apply(path: &Path, entries: &[HostsEntry]) -> std::io::Result<HostsApplyResult> {
    let backup = backup(path)?;
    let content = fs::read_to_string(path)?;

    let mut kept: Vec<String> = Vec::new();
    let mut removed: Vec<HostsEntry> = Vec::new();

    for line in content.lines() {
        if let Some(entry) = parse_line(line) {
            if entry.managed {
                removed.push(entry);
                continue;
            }
        }
        // 单独的块头行
        if is_managed_line(line) {
            continue;
        }
        kept.push(line.to_string());
    }

    let mut final_lines = kept;
    if !entries.is_empty() {
        while final_lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
            final_lines.pop();
        }
        final_lines.push(MARKER.to_string());
        for entry in entries {
            final_lines.push(entry.to_line());
        }
    }

    write_lines(path, &final_lines)?;

    Ok(HostsApplyResult {
        added: entries.to_vec(),
        removed,
        backup_path: Some(backup.to_string_lossy().to_string()),
    })
}

/// 删除所有 managed 行。
pub fn remove_managed(path: &Path) -> std::io::Result<HostsApplyResult> {
    let backup = backup(path)?;
    let content = fs::read_to_string(path)?;

    let mut kept: Vec<String> = Vec::new();
    let mut removed: Vec<HostsEntry> = Vec::new();

    for line in content.lines() {
        if let Some(entry) = parse_line(line) {
            if entry.managed {
                removed.push(entry);
                continue;
            }
        }
        if is_managed_line(line) {
            continue;
        }
        kept.push(line.to_string());
    }

    write_lines(path, &kept)?;

    Ok(HostsApplyResult {
        added: Vec::new(),
        removed,
        backup_path: Some(backup.to_string_lossy().to_string()),
    })
}

/// 备份 hosts，返回备份路径。
pub fn backup(path: &Path) -> std::io::Result<PathBuf> {
    let backup_path = PathBuf::from(format!("{}.mcspeed.bak", path.display()));
    fs::copy(path, &backup_path)?;
    Ok(backup_path)
}

/// 生成本代理要写入的条目：每条规则的域名 × IPv4/IPv6。
///
/// 通配域名不写 hosts：hosts 只认完整主机名，`*.example.com` 写进去没有意义，
/// 这类域名的匹配完全交给 Caddy 的站点地址完成。
pub fn proxy_entries(rules: &[crate::config::ProxyRule]) -> Vec<HostsEntry> {
    let comment = Some("本地加速代理（Caddy → 上游）".to_string());
    let mut entries: Vec<HostsEntry> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    for rule in rules.iter() {
        for domain in &rule.domains {
            // 通配域名进不了 hosts，跳过
            if domain.trim().to_ascii_lowercase().starts_with("*.") {
                continue;
            }
            let domain = domain.trim().to_ascii_lowercase();
            if domain.is_empty() || seen.contains(&domain) {
                continue;
            }
            seen.push(domain.clone());

            entries.push(HostsEntry {
                ip: LOOPBACK_V4.into(),
                domain: domain.clone(),
                managed: true,
                comment: comment.clone(),
            });
            entries.push(HostsEntry {
                ip: LOOPBACK_V6.into(),
                domain,
                managed: true,
                comment: comment.clone(),
            });
        }
    }
    entries
}

/// 判断 hosts 是否已写入本代理的条目（至少一条规则的 IPv4 记录）。
pub fn is_applied(path: &Path, rules: &[crate::config::ProxyRule]) -> bool {
    let wanted: Vec<String> = rules
        .iter()
        .flat_map(|r| r.domains.iter())
        .map(|d| d.trim().to_ascii_lowercase())
        .filter(|d| !d.is_empty() && !d.starts_with("*."))
        .collect();

    if wanted.is_empty() {
        return false;
    }

    read_managed(path)
        .map(|entries| {
            entries.iter().any(|e| {
                e.ip == LOOPBACK_V4
                    && wanted
                        .iter()
                        .any(|d| d.eq_ignore_ascii_case(&e.domain))
            })
        })
        .unwrap_or(false)
}

impl HostsEntry {
    /// 生成 hosts 行，保留标记与注释。
    pub fn to_line(&self) -> String {
        match &self.comment {
            Some(c) if !c.is_empty() => format!("{} {} {} {}", self.ip, self.domain, MARKER, c),
            _ => format!("{} {} {}", self.ip, self.domain, MARKER),
        }
    }
}

fn write_lines(path: &Path, lines: &[String]) -> std::io::Result<()> {
    // 用 \n 拼接，末尾补一个换行，与 Windows hosts 惯例一致
    let mut content = lines.join("\n");
    if !content.ends_with('\n') {
        content.push('\n');
    }
    fs::write(path, content)
}
