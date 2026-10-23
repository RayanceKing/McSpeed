use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 线上 Cloudflare Worker 地址。
pub const UPSTREAM: &str = "https://mcproxy.unmod.online";

/// Caddy 管理端点，用于热加载与状态查询。
pub const CADDY_ADMIN: &str = "http://localhost:2019";

/// Caddy 监听的本地端口。
pub const LISTEN_PORT: u16 = 443;

/// hosts 标记注释。
pub const MARKER: &str = "# MC Speed managed";

/// 旧版本写进 hosts 的标记。读的时候一并识别，
/// 否则改名后旧的托管条目会被当成用户自己的内容，清理不掉也认不出来。
pub const LEGACY_MARKERS: &[&str] = &["# McSpeed managed"];

/// 本机环回地址。IPv4 与 IPv6 都写，避免只解析到 v6 时失配。
pub const LOOPBACK_V4: &str = "127.0.0.1";
pub const LOOPBACK_V6: &str = "::1";

/// 域名通配符前缀。用户写 `*.example.com` 时只用于匹配，不写 hosts。
pub const WILDCARD_PREFIX: &str = "*.";

/// 单次测速下载的超时（秒）。测速会真实下载一段内容，
/// 超时即视为不可用，不会让用户干等。
pub const PROBE_TIMEOUT_SECS: u64 = 5;

/// 测速下载的体积上限（字节）。测速只需要一段数据算速度，
/// 不能为一次探测拉爆流量。
pub const PROBE_MAX_BYTES: u64 = 5 * 1024 * 1024;

/// 一条用户自定义的加速规则。
///
/// 规则回答两个问题：劫持哪些域名、命中什么路径；命中后统一转发到
/// 指定上游。所有字段都带 `#[serde(default)]`，老版本 settings.json
/// 缺字段也能读。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProxyRule {
    /// 稳定标识，同时用作 Caddy matcher 名，只允许字母数字下划线连字符。
    pub id: String,
    /// 规则名，仅用于展示。
    pub name: String,
    /// 作用的域名列表。支持 `*.example.com` 通配（只影响 Caddy 匹配，不写 hosts）。
    pub domains: Vec<String>,
    /// 命中的路径前缀。空或 `/` 表示全部路径。
    pub path: String,
    /// 上游 origin，如 `https://mcproxy.unmod.online`。
    pub upstream: String,
    /// 转发前补到路径前的前缀，Worker 靠它选路由。
    pub route_prefix: String,
    /// 测速分组名。同名的规则互相竞争，由 `resolve_active` 按最近一次
    /// 测速的下载用时挑出唯一一条生效；空串表示不参与竞争，按普通规则
    /// 顺序匹配。
    pub speed_group: String,
    /// 最近一次测速的下载用时（毫秒）。None 表示还没测过或上次失败。
    pub probe_ms: Option<u64>,
    /// 最近一次测速下载的字节数，与用时一起换算成速度展示。
    pub probe_bytes: u64,
    /// 测速时间，Unix 秒。
    pub probe_at: u64,
}

impl ProxyRule {
    /// 归一化后的路径前缀：空串表示「全部路径」。
    pub fn path_prefix(&self) -> &str {
        let p = self.path.trim();
        if p == "/" {
            ""
        } else {
            p
        }
    }

    /// 上游的 host，用于 Host 头与 SNI。
    pub fn upstream_host(&self) -> String {
        host_of(self.upstream.trim())
    }

    /// 写入 Caddy matcher 的安全名字。
    pub fn matcher_name(&self) -> String {
        format!("rule_{}", sanitize_id(&self.id))
    }

    /// 界面用的一行摘要，说明这条规则最终把请求送到哪里。
    pub fn summary(&self) -> String {
        let domains = self.domains.join("、");
        let path = {
            let p = self.path_prefix();
            if p.is_empty() {
                "/*".to_string()
            } else {
                format!("{}*", p)
            }
        };

        let prefix = self.route_prefix.trim();
        let target = if prefix.is_empty() {
            self.upstream.trim().to_string()
        } else {
            format!("{}{}", self.upstream.trim(), prefix)
        };

        let group = self.speed_group.trim();
        if group.is_empty() {
            format!("{} {} → {}", domains, path, target)
        } else {
            format!("{} {} → {}（测速组 {}）", domains, path, target, group)
        }
    }

    /// 测速用的 URL：本条规则自己的转发目标（上游 + 路由前缀 + 路径前缀）。
    ///
    /// 测速要比较的是「同组里走哪个上游更快」，所以必须打各自的上游，
    /// 而不是打被劫持的域名——域名只有一个，命中的是当前生效的那条规则，
    /// 所有候选测出来都是同一个数，没法比较。
    pub fn probe_url(&self) -> String {
        let upstream = self.upstream.trim().trim_end_matches('/');
        let prefix = self.route_prefix.trim();
        let path = {
            let p = self.path_prefix();
            if p.is_empty() {
                "/"
            } else {
                p
            }
        };
        format!("{}{}{}", upstream, prefix, path)
    }

    /// 校验用户输入。错误信息直接展示给用户，所以说清怎么改。
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("规则名称不能为空。".into());
        }

        if self.domains.is_empty() {
            return Err("至少填写一个域名。".into());
        }

        let mut seen: Vec<String> = Vec::new();
        for domain in &self.domains {
            let raw = domain.trim();
            // 校验前先判断原始输入：大小写会被归一化掉，
            // 直接校验小写结果会把「UPPER.COM」当成合法输入放过
            if raw != raw.to_ascii_lowercase() {
                return Err(format!(
                    "域名「{}」含有大写字母：hosts 与 Caddy 只认小写，请改成全小写。",
                    raw
                ));
            }

            let d = raw.to_ascii_lowercase();
            if !is_valid_host_pattern(&d) {
                return Err(format!(
                    "域名「{}」不合法：只能包含小写字母、数字、连字符和点，不能带协议、端口或路径。",
                    raw
                ));
            }
            if seen.contains(&d) {
                return Err(format!("域名「{}」重复了。", d));
            }
            seen.push(d);
        }

        let path = self.path.trim();
        if !path.is_empty() && !path.starts_with('/') {
            return Err("路径前缀必须以 / 开头，例如 /net。".into());
        }
        if path.contains([' ', '?', '#']) {
            return Err("路径前缀不能包含空格、? 或 #。".into());
        }

        let upstream = self.upstream.trim();
        if !upstream.starts_with("http://") && !upstream.starts_with("https://") {
            return Err("转发目标必须是 http:// 或 https:// 开头的地址。".into());
        }
        if self.upstream_host().is_empty() || !is_valid_host(&self.upstream_host()) {
            return Err(format!("转发目标「{}」里的主机名不合法。", upstream));
        }

        let prefix = self.route_prefix.trim();
        if !prefix.is_empty() && !prefix.starts_with('/') {
            return Err("路径前缀必须以 / 开头，例如 /fabric-maven。".into());
        }
        if prefix.contains([' ', '?', '#']) {
            return Err("路径前缀不能包含空格、? 或 #。".into());
        }

        // 测速分组名只用于界面分组与内部索引，限制成安全字符，
        // 避免它被拼进 Caddy matcher 之类的地方时出问题
        let group = self.speed_group.trim();
        if !group.is_empty()
            && !group
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            return Err(
                "测速分组名只能包含字母、数字、点、连字符和下划线，例如 maven。".into(),
            );
        }

        Ok(())
    }
}

/// 用户设置。`domains` 是旧版的域名启停记录，仅作迁移痕迹保留。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    /// 旧版字段：domain -> enabled
    #[serde(default)]
    pub domains: HashMap<String, bool>,
    /// 新版字段：用户自定义规则，顺序即匹配优先级。
    #[serde(default)]
    pub rules: Vec<ProxyRule>,
}

/// 取出用户保存的全部规则（未做测速收敛）。
///
/// 界面要展示每一条规则及其探测延迟，包括同组里当前被压制的那条，
/// 所以这里返回原始列表。要拿给 Caddy / hosts 用的生效列表，
/// 请走 `resolve_active`。
///
/// 老用户升级时 settings.json 里没有 rules，这时按旧域名清单生成
/// 三条等价的 Fabric 规则，避免升级后链路突然失效。
pub fn rules(settings: &Settings) -> Vec<ProxyRule> {
    if settings.rules.is_empty() {
        legacy_rules(settings)
    } else {
        settings.rules.clone()
    }
}

/// 由旧版域名清单迁移出来的默认规则。
fn legacy_rules(_settings: &Settings) -> Vec<ProxyRule> {
    vec![
        ProxyRule {
            id: "fabric-maven".into(),
            name: "Fabric Maven 仓库".into(),
            domains: vec!["maven.fabricmc.net".into()],
            path: String::new(),
            upstream: UPSTREAM.into(),
            route_prefix: "/fabric-maven".into(),
            speed_group: String::new(),
            probe_ms: None,
            probe_bytes: 0,
            probe_at: 0,
        },
        ProxyRule {
            id: "fabric-meta".into(),
            name: "Fabric 元数据".into(),
            domains: vec!["meta.fabricmc.net".into()],
            path: String::new(),
            upstream: UPSTREAM.into(),
            route_prefix: "/fabric-meta".into(),
            speed_group: String::new(),
            probe_ms: None,
            probe_bytes: 0,
            probe_at: 0,
        },
        ProxyRule {
            id: "fabric-site".into(),
            name: "Fabric 官网".into(),
            domains: vec!["fabricmc.net".into()],
            path: String::new(),
            upstream: UPSTREAM.into(),
            route_prefix: "/fabric".into(),
            speed_group: String::new(),
            probe_ms: None,
            probe_bytes: 0,
            probe_at: 0,
        },
    ]
}

/// 生成新规则 id。时间戳足够唯一，且不带机器相关信息。
pub fn new_rule_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("rule-{:x}", nanos)
}

/// 把任意 id 压成 Caddy matcher 可用的名字。
fn sanitize_id(id: &str) -> String {
    let mut out: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        out.push_str("unnamed");
    }
    out
}

/// 判断是否为合法主机名（不含通配符）。
pub fn is_valid_host(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    if !host.contains('.') {
        // 顶层标签（localhost）无法用于 hosts 劫持，直接拒绝
        return false;
    }

    for label in host.split('.') {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        if label.starts_with('-') || label.ends_with('-') {
            return false;
        }
        if !label
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return false;
        }
    }
    true
}

/// 判断是否为合法域名或通配域名。
pub fn is_valid_host_pattern(pattern: &str) -> bool {
    if let Some(rest) = pattern.strip_prefix(WILDCARD_PREFIX) {
        is_valid_host(rest)
    } else {
        is_valid_host(pattern)
    }
}

/// 从 URL 里取出 host，用于 Host 头与 SNI。
pub fn host_of(url: &str) -> String {
    let without_scheme = url.split("//").nth(1).unwrap_or(url);
    let host_port = without_scheme.split('/').next().unwrap_or(url);
    // 去掉用户信息与端口，IPv6 字面量保持原样
    let host = host_port.rsplit('@').next().unwrap_or(host_port);
    match host.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(v6).to_string(),
        None => host.split(':').next().unwrap_or(host).to_string(),
    }
}

/// 当前 Unix 秒。取不到时钟时退到 0，配合饱和减法不会 panic。
pub fn now_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 按测速分组收敛出真正生效的规则列表。
///
/// 同一 `speed_group` 内可能有多条 proxy 规则指向不同上游（例如两个
/// 不同的 Worker 域名），但同一域名同一路径只能有一个 Caddy matcher，
/// 否则后一条会静默覆盖前一条。这里按下载用时挑出最快的一条留下，
/// 其余同组规则从生效列表里剔除——它们仍然保存在设置里，
/// 下次测速结果变化时可能重新胜出。
///
/// 测速数据缺失时退化成「取组内第一条规则」，保证任何配置下都能生成
/// 可用的 Caddyfile。测速结果长期有效，不做过期回退：
/// 界面展示与排序都以落盘的测速结果为准。
pub fn resolve_active(rules: &[ProxyRule]) -> Vec<ProxyRule> {
    // 组名 -> 该组胜出规则在 rules 中的下标
    let mut winners: HashMap<String, usize> = HashMap::new();

    for (index, rule) in rules.iter().enumerate() {
        let group = rule.speed_group.trim();
        if group.is_empty() {
            continue;
        }

        match winners.get(group) {
            None => {
                winners.insert(group.to_string(), index);
            }
            Some(&current) => {
                if faster(&rules[current], rule) {
                    winners.insert(group.to_string(), index);
                }
            }
        }
    }

    // 胜出者留下，同组其余的被压制；未参与分组的规则一律保留。
    // 被压制的规则仍存在于设置文件中，下次测速可能重新胜出。
    let kept: Vec<usize> = winners.into_values().collect();

    rules
        .iter()
        .enumerate()
        .filter(|(index, rule)| {
            rule.speed_group.trim().is_empty() || kept.contains(index)
        })
        .map(|(_, rule)| rule.clone())
        .collect()
}

/// 判断 candidate 是否比 current 更适合作为组内生效规则。
///
/// 两边都有测速数据时比下载用时；只有一边有数据时信有数据的那边；
/// 都没有数据时保留先出现的一条，让结果可预测。
fn faster(current: &ProxyRule, candidate: &ProxyRule) -> bool {
    match (candidate.probe_ms, current.probe_ms) {
        (Some(candidate_ms), Some(current_ms)) => candidate_ms < current_ms,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => false,
    }
}

/// 组 key：域名集合（排序后拼接）+ 路径前缀。
///
/// 与前端 `groupRules` 的归组口径一致：同域名同路径的规则在界面上
/// 显示为一组，测速后也按这个 key 在组内重排。
pub fn group_key(rule: &ProxyRule) -> String {
    let mut domains: Vec<String> = rule
        .domains
        .iter()
        .map(|d| d.trim().to_ascii_lowercase())
        .collect();
    domains.sort();
    let path = rule.path.trim();
    let path = if path == "/" { "" } else { path };
    format!("{}|{}", domains.join(","), path)
}

/// 组内按下载用时升序重排：用时最小的排最前，没测过或失败的排最后。
///
/// 只重排 `keys` 里点名的组——本次没测过的组保持用户手动排的顺序，
/// 不去动它。重排后的顺序随设置一起落盘，重启后依然是排好的。
/// 只在组内成员各自的原位置之间换序，不动组与组之间的相对次序。
pub fn sort_groups_by_probe(rules: &mut [ProxyRule], keys: &std::collections::HashSet<String>) {
    use std::collections::HashMap;

    // 组 key -> 成员在 rules 中的下标（按出现顺序）
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, rule) in rules.iter().enumerate() {
        let key = group_key(rule);
        if keys.contains(&key) {
            groups.entry(key).or_default().push(index);
        }
    }

    for indices in groups.values() {
        let mut order = indices.clone();
        // 没测过的用 u64::MAX 排到末尾；用时相同保持原有先后
        order.sort_by_key(|&i| (rules[i].probe_ms.unwrap_or(u64::MAX), rules[i].probe_at));

        let reordered: Vec<ProxyRule> = order.iter().map(|&i| rules[i].clone()).collect();
        for (slot, &target) in indices.iter().enumerate() {
            rules[target] = reordered[slot].clone();
        }
    }
}

/// 设置文件路径：`%LOCALAPPDATA%\McSpeed\settings.json`。
///
/// 目录名沿用改名前的 `McSpeed`，为的是让已有安装的设置原样读得到。
/// 想彻底换名需要先做一次迁移（读到旧目录则复制到新目录），否则用户配置会丢。
pub fn settings_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("McSpeed").join("settings.json")
}

pub fn load_settings() -> Settings {
    let path = settings_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save_settings(settings: &Settings) -> std::io::Result<()> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(path, text)
}

/// 检测是否存在会绕过 hosts 劫持的代理环境变量。
///
/// curl、PowerShell、部分 Node 工具会读 HTTP_PROXY / HTTPS_PROXY，
/// 命中后请求直接走代理出境，根本不查 hosts，本地链路被整个跳过。
/// Java（Gradle / 启动器）不读这些变量，所以照样走本地。
pub fn proxy_env_warning() -> Option<String> {
    let found: Vec<String> = ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"]
        .iter()
        .filter_map(|key| {
            std::env::var(key)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .map(|v| format!("{}={}", key, v))
        })
        .collect();

    if found.is_empty() {
        None
    } else {
        Some(format!(
            "检测到 {}。curl 等工具会绕过 hosts 直连，本地链路对它们不生效；Gradle 与启动器不受影响。",
            found.join("、")
        ))
    }
}
