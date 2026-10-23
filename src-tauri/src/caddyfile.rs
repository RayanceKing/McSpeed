use crate::caddy::{cert_file, cert_key_file};
use crate::config::{is_valid_host_pattern, ProxyRule, UPSTREAM};
use std::path::Path;

/// 根据当前生效的规则生成 Caddyfile。
///
/// 之所以动态生成而不是维护一份静态文件：规则要能即时生效，
/// 而 Caddy 只认自己加载时读到的配置。
///
/// 每条规则渲染成一组站点块：一个域名一个，通配域名用 `*.` 站点地址。
/// 同一域名可能被多条规则命中，因此每个站点块内部按规则顺序
/// 用命名 matcher + handle 串起来，第一条命中的规则生效。
pub fn render(rules: &[ProxyRule], log_path: &Path) -> String {
    let cert = cert_file().to_string_lossy().replace('\\', "/");
    let key = cert_key_file().to_string_lossy().replace('\\', "/");
    let log = log_path.to_string_lossy().replace('\\', "/");

    let mut out = String::with_capacity(4096);

    out.push_str("# 本文件由 McSpeed 自动生成，请勿手工修改。\n");
    out.push_str("# 规则变化后会重新生成并热加载。\n\n");

    out.push_str("{\n");
    out.push_str("\tauto_https off\n");
    out.push_str("\tadmin localhost:2019\n");
    out.push_str("}\n\n");

    // 自测入口：本机直接访问 mcproxy.unmod.online 时用
    out.push_str("# 自测入口\n");
    out.push_str(&format!("https://{} {{\n", host_of(UPSTREAM)));
    out.push_str(&format!("\ttls {} {}\n\n", cert, key));
    out.push_str(&format!("\treverse_proxy {} {{\n", UPSTREAM));
    out.push_str(&format!("\t\theader_up Host {}\n", host_of(UPSTREAM)));
    out.push_str("\t\theader_up X-Forwarded-Proto https\n");
    out.push_str("\t\ttransport http {\n");
    out.push_str(&format!("\t\t\ttls_server_name {}\n", host_of(UPSTREAM)));
    out.push_str("\t\t}\n");
    out.push_str("\t}\n\n");
    out.push_str("\theader X-Local-Proxy \"FabricProxy/Caddy\"\n\n");
    push_log(&mut out, &log);
    out.push_str("}\n\n");

    if rules.is_empty() {
        out.push_str("# 当前没有配置任何规则，仅保留自测入口。\n");
        return out;
    }

    // 按站点地址分组，保持规则顺序；同一地址的规则在块内按顺序匹配
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<&ProxyRule>> =
        std::collections::HashMap::new();

    for rule in rules {
        for domain in &rule.domains {
            let site = normalize_site(domain);
            if site.is_empty() {
                continue;
            }
            if !groups.contains_key(&site) {
                order.push(site.clone());
            }
            groups.entry(site).or_default().push(rule);
        }
    }

    // 规则有域名但全部不合法时同样要给出能启动的空配置，
    // 否则 Caddy 会因为空站点地址拒绝加载
    if order.is_empty() {
        out.push_str("# 没有可用的域名，仅保留自测入口。\n");
        return out;
    }

    for site in order {
        let site_rules = &groups[&site];
        out.push_str(&format!("# {} ← {} 条规则\n", site, site_rules.len()));
        out.push_str(&format!("https://{} {{\n", site));
        out.push_str(&format!("\ttls {} {}\n", cert, key));

        for rule in site_rules {
            let matcher = rule.matcher_name();
            let path = rule.path_prefix();

            // 路径条件写进 matcher，handle 块才能按规则逐个尝试。
            // host 条件必须写：站点块内的 handle 只按 path 匹配时，
            // Caddy 会用「最长前缀」而不是规则顺序决定胜负。
            // path 后面跟 * 表示前缀匹配——Caddy 的 path 默认精确匹配，
            // 少了 * 会让 /only 这类规则漏掉 /only/x 子路径
            out.push_str(&format!("\t@{} {{\n", matcher));
            out.push_str(&format!("\t\thost {}\n", site));
            if !path.is_empty() {
                out.push_str(&format!("\t\tpath {}*\n", path));
            }
            out.push_str("\t}\n");

            out.push_str(&format!("\thandle @{} {{\n", matcher));
            push_rule_body(&mut out, rule);
            out.push_str("\t}\n\n");
        }

        // 所有规则都没命中时给出可读的 404，而不是 Caddy 默认的空响应
        out.push_str("\thandle {\n");
        out.push_str("\t\trespond \"McSpeed: 没有规则命中该请求\" 404\n");
        out.push_str("\t}\n\n");

        out.push_str("\theader X-Local-Proxy \"FabricProxy/Caddy\"\n\n");
        push_log(&mut out, &log);
        out.push_str("}\n\n");
    }

    out
}

/// 把规则的转发动作写进 handle 块。
fn push_rule_body(out: &mut String, rule: &ProxyRule) {
    let upstream = rule.upstream.trim();
    let host = rule.upstream_host();

    // 转发前给路径补上 Worker 需要的路由前缀
    let prefix = rule.route_prefix.trim();
    if !prefix.is_empty() {
        out.push_str(&format!("\t\trewrite * {}{{uri}}\n", prefix));
    }

    out.push_str(&format!("\t\treverse_proxy {} {{\n", upstream));
    out.push_str(&format!("\t\t\theader_up Host {}\n", host));
    out.push_str("\t\t\theader_up X-Forwarded-Proto https\n");

    if upstream.starts_with("https://") {
        out.push_str("\t\t\ttransport http {\n");
        out.push_str(&format!("\t\t\t\ttls_server_name {}\n", host));
        out.push_str("\t\t\t}\n");
    }

    out.push_str("\t\t}\n");
}

/// 把域名归一化成 Caddy 站点地址：小写、去端口、通配保留。
fn normalize_site(domain: &str) -> String {
    let d = domain.trim().to_ascii_lowercase();
    if d.is_empty() || !is_valid_host_pattern(&d) {
        return String::new();
    }
    d
}

/// 从 URL 里取出 host，用于 Host 头与 SNI。
fn host_of(url: &str) -> String {
    crate::config::host_of(url)
}

fn push_log(out: &mut String, log: &str) {
    out.push_str("\tlog {\n");
    out.push_str(&format!("\t\toutput file {} {{\n", log));
    out.push_str("\t\t\troll_size 10MiB\n");
    out.push_str("\t\t\troll_keep 3\n");
    out.push_str("\t\t}\n");
    out.push_str("\t}\n");
}
