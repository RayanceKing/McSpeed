mod author;
mod caddy;
mod caddyfile;
mod cert;
mod check;
mod config;
mod hosts;
mod probe;
mod traffic;

use author::AuthorInfo;
use caddy::StopResult;
use check::CheckResult;
use config::{ProxyRule, UPSTREAM};
use hosts::{default_hosts_path, HostsApplyResult, HostsEntry};
use serde::Serialize;
use std::process::Command;

/// 各组件的当前状态，供前端一次性渲染。
#[derive(Debug, Clone, Serialize)]
pub struct ProxyStatus {
    /// 整条链路是否在运行：Caddy 在听且 hosts 已接管。
    /// 前端的总开关直接绑这个值。
    pub active: bool,
    pub caddy_running: bool,
    pub caddy_admin_up: bool,
    pub certificate_ready: bool,
    pub caddy_installed: bool,
    pub mkcert_installed: bool,
    pub hosts_applied: bool,
    pub hosts_path: String,
    pub caddyfile_path: String,
    pub upstream: String,
    /// 代理环境变量警告。None 表示没有会绕过 hosts 的代理。
    pub proxy_warning: Option<String>,
    /// 用户保存的全部规则，含同组里被压制的那条。
    /// 要拿给 Caddy / hosts 用的生效列表请走 config::active_rules。
    pub rules: Vec<ProxyRule>,
    pub managed_entries: Vec<HostsEntry>,
}

/// 判断链路是否算「开着」。
///
/// 两个条件都要满足：Caddy 在听，且 hosts 已接管。
/// 只有其中一个说明处于中间态，此时总开关显示为关更安全——
/// 用户点一下就会把另一个也补齐或清掉。
fn is_active(caddy_running: bool, hosts_applied: bool) -> bool {
    caddy_running && hosts_applied
}

#[tauri::command]
async fn get_status() -> Result<ProxyStatus, String> {
    let hosts_path = default_hosts_path();
    let settings = config::load_settings();
    // 界面要看到每一条规则（含被压制的），所以这里返回原始列表
    let rules = config::rules(&settings);
    let active = config::resolve_active(&rules);

    let caddy_running = caddy::is_listening();
    let caddy_admin_up = caddy::is_admin_up().await;
    let certificate_ready = cert::exists();
    let caddy_installed = caddy::caddy_path().exists();
    let mkcert_installed = cert::mkcert_installed();
    let hosts_applied = hosts::is_applied(&hosts_path, &active);
    let managed_entries = hosts::read_managed(&hosts_path).unwrap_or_default();

    Ok(ProxyStatus {
        active: is_active(caddy_running, hosts_applied),
        caddy_running,
        caddy_admin_up,
        certificate_ready,
        caddy_installed,
        mkcert_installed,
        hosts_applied,
        hosts_path: hosts_path.to_string_lossy().to_string(),
        caddyfile_path: caddy::caddyfile_path().to_string_lossy().to_string(),
        upstream: UPSTREAM.into(),
        proxy_warning: config::proxy_env_warning(),
        rules,
        managed_entries,
    })
}

/// 把规则落到磁盘并同步 hosts 与 Caddy。
///
/// `sync_hosts` 为 false 时跳过 hosts 写入——纯排序不改域名，没必要
/// 重写系统文件。返回的状态直接从已知数据拼出，不再重新探测网络，
/// 避免每次保存都等 TCP + HTTP 超时。
async fn persist_and_sync(rules: Vec<ProxyRule>, sync_hosts: bool) -> Result<ProxyStatus, String> {
    for rule in &rules {
        rule.validate()
            .map_err(|e| format!("「{}」：{}", rule.name, e))?;
    }

    let mut settings = config::load_settings();
    let previous = settings.rules.clone();
    settings.rules = rules;
    config::save_settings(&settings).map_err(|e| format!("保存设置失败：{}", e))?;

    let active = config::resolve_active(&settings.rules);
    let hosts_path = default_hosts_path();

    // 只有已经接管过 hosts 时才同步；没接管说明用户没启用过本地代理，
    // 这时不该擅自往 hosts 里写东西。
    let already_managed = hosts::read_managed(&hosts_path)
        .map(|e| !e.is_empty())
        .unwrap_or(false);

    if sync_hosts && already_managed {
        if let Err(e) = hosts::apply(&hosts_path, &hosts::proxy_entries(&active)) {
            rollback_rules(previous);
            return Err(format!("更新 hosts 失败：{}", e));
        }
    }

    // Caddy 在跑才热加载；没跑时下次启动会读到新配置
    let caddy_running = caddy::is_listening();
    if caddy_running {
        if let Err(e) = caddy::write_caddyfile(&active) {
            rollback_rules(previous);
            return Err(format!("生成 Caddyfile 失败：{}", e));
        }
        if let Err(e) = caddy::reload().await {
            rollback_rules(previous);
            return Err(format!("热加载 Caddy 失败：{}", e));
        }
    }

    Ok(build_status(settings.rules, caddy_running, already_managed))
}

/// 从已知数据拼出状态，不再重新探测网络。
///
/// `get_status()` 里的 is_listening / is_admin_up 各有一次网络超时，
/// 每次保存都调会让操作卡住。这里复用调用方已经拿到的结果。
fn build_status(rules: Vec<ProxyRule>, caddy_running: bool, hosts_applied: bool) -> ProxyStatus {
    let hosts_path = default_hosts_path();
    let managed_entries = hosts::read_managed(&hosts_path).unwrap_or_default();

    ProxyStatus {
        active: is_active(caddy_running, hosts_applied),
        caddy_running,
        // 刚热加载成功说明管理端点一定活着，不必再探测
        caddy_admin_up: caddy_running,
        certificate_ready: cert::exists(),
        caddy_installed: caddy::caddy_path().exists(),
        mkcert_installed: cert::mkcert_installed(),
        hosts_applied,
        hosts_path: hosts_path.to_string_lossy().to_string(),
        caddyfile_path: caddy::caddyfile_path().to_string_lossy().to_string(),
        upstream: UPSTREAM.into(),
        proxy_warning: config::proxy_env_warning(),
        rules,
        managed_entries,
    }
}

/// 把规则列表改回旧值，用于操作失败时回滚。
fn rollback_rules(previous: Vec<ProxyRule>) {
    let mut settings = config::load_settings();
    settings.rules = previous;
    config::save_settings(&settings).ok();
}

/// 新增一条规则，返回最新状态。
#[tauri::command]
async fn add_rule(rule: ProxyRule) -> Result<ProxyStatus, String> {
    rule.validate().map_err(|e| e.to_string())?;

    let mut settings = config::load_settings();
    // 直接读 settings.rules 而不是 config::rules()：迁移出来的默认规则
    // 不该在用户新增第一条规则时被悄悄写进设置文件
    if settings.rules.is_empty() {
        settings.rules = config::rules(&settings);
    }
    settings.rules.push(rule);
    persist_and_sync(settings.rules, true).await
}

/// 更新一条规则（按 id 匹配），返回最新状态。
#[tauri::command]
async fn update_rule(rule: ProxyRule) -> Result<ProxyStatus, String> {
    rule.validate().map_err(|e| e.to_string())?;

    let mut settings = config::load_settings();
    let index = settings
        .rules
        .iter()
        .position(|r| r.id == rule.id)
        .ok_or_else(|| format!("找不到 id 为 {} 的规则", rule.id))?;
    settings.rules[index] = rule;
    persist_and_sync(settings.rules, true).await
}

/// 删除一条规则，返回最新状态。
#[tauri::command]
async fn delete_rule(id: String) -> Result<ProxyStatus, String> {
    let mut settings = config::load_settings();
    let before = settings.rules.len();
    settings.rules.retain(|r| r.id != id);

    if settings.rules.len() == before {
        return Err(format!("找不到 id 为 {} 的规则", id));
    }

    persist_and_sync(settings.rules, true).await
}

/// 整体替换规则列表，用于拖拽排序后的提交。
#[tauri::command]
async fn set_rules(rules: Vec<ProxyRule>) -> Result<ProxyStatus, String> {
    persist_and_sync(rules, false).await
}

/// 测速。`ids` 缺省或为空时测全部规则，否则只测给定的几条。
///
/// 测速会真实下载每个规则自己上游的一段内容，用下载用时衡量快慢；
/// 测完在每个组内按用时升序重排（最快在前、没测过的在后），
/// 结果与排序一起落盘，重启后依然生效。
/// 测速会改变同组里哪条规则胜出，所以保存后必须重写 Caddyfile 并热加载，
/// 否则用户看到的「已选最快」和实际生效的配置会不一致。
#[tauri::command]
async fn probe_rules(ids: Option<Vec<String>>) -> Result<ProxyStatus, String> {
    let mut settings = config::load_settings();
    // 先把迁移默认规则材料化：settings.rules 为空时 config::rules 会返回
    // 旧版迁移出来的规则，测速结果必须落到真实列表上才能持久化
    let mut rules = config::rules(&settings);

    let targets: Vec<ProxyRule> = match &ids {
        Some(ids) if !ids.is_empty() => rules
            .iter()
            .filter(|r| ids.contains(&r.id))
            .cloned()
            .collect(),
        _ => rules.clone(),
    };
    if targets.is_empty() {
        return Err("没有要测速的规则。".into());
    }

    let results = probe::probe(&targets).await;
    probe::apply_results(&mut rules, &results);
    settings.rules = rules;
    config::save_settings(&settings).map_err(|e| format!("保存测速结果失败：{}", e))?;

    let active = config::resolve_active(&settings.rules);

    if caddy::is_listening() {
        caddy::write_caddyfile(&active).map_err(|e| e.to_string())?;
        caddy::reload().await.map_err(|e| e.to_string())?;
    }

    get_status().await
}

/// 一键开关：开=拉起整条链路，关=停 Caddy 并清 hosts。
///
/// 关的时候保留设置文件，下次开不用重选规则。
#[tauri::command]
async fn toggle_proxy(enable: bool) -> Result<ProxyStatus, String> {
    if enable {
        return setup_all().await;
    }

    // 关：先停 Caddy，再清 hosts。顺序反了会出现端口已释放但 hosts
    // 还指向本机的窗口，那时访问域名会直接连不上。
    caddy::stop();

    let hosts_path = default_hosts_path();
    if hosts_path.exists() {
        hosts::remove_managed(&hosts_path).map_err(|e| e.to_string())?;
    }

    get_status().await
}

/// 一键把整套链路拉起来：证书 → hosts → Caddy。
#[tauri::command]
async fn setup_all() -> Result<ProxyStatus, String> {
    if !caddy::caddy_path().exists() {
        return Err(format!(
            "找不到 caddy.exe：{}",
            caddy::caddy_path().display()
        ));
    }

    if !cert::exists() {
        cert::issue().map_err(|e| e.to_string())?;
    }

    let hosts_path = default_hosts_path();
    if !hosts_path.exists() {
        return Err(format!("hosts 文件不存在：{}", hosts_path.display()));
    }
    let settings = config::load_settings();
    let active = config::resolve_active(&config::rules(&settings));
    hosts::apply(&hosts_path, &hosts::proxy_entries(&active)).map_err(|e| e.to_string())?;

    caddy::write_caddyfile(&active).map_err(|e| e.to_string())?;
    caddy::start().map_err(|e| e.to_string())?;

    get_status().await
}

#[tauri::command]
async fn start_caddy() -> Result<ProxyStatus, String> {
    let settings = config::load_settings();
    let active = config::resolve_active(&config::rules(&settings));
    caddy::write_caddyfile(&active).map_err(|e| e.to_string())?;
    caddy::start().map_err(|e| e.to_string())?;
    get_status().await
}

#[tauri::command]
fn stop_caddy() -> StopResult {
    caddy::stop()
}

#[tauri::command]
async fn reload_caddy() -> Result<(), String> {
    caddy::reload().await
}

#[tauri::command]
async fn apply_hosts() -> Result<HostsApplyResult, String> {
    let hosts_path = default_hosts_path();
    if !hosts_path.exists() {
        return Err(format!("hosts 文件不存在：{}", hosts_path.display()));
    }
    let settings = config::load_settings();
    let active = config::resolve_active(&config::rules(&settings));
    hosts::apply(&hosts_path, &hosts::proxy_entries(&active)).map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_hosts() -> Result<HostsApplyResult, String> {
    let hosts_path = default_hosts_path();
    if !hosts_path.exists() {
        return Err(format!("hosts 文件不存在：{}", hosts_path.display()));
    }
    hosts::remove_managed(&hosts_path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn issue_certificate() -> Result<String, String> {
    cert::issue().map_err(|e| e.to_string())?;
    Ok(caddy::cert_file().to_string_lossy().to_string())
}

/// 卸载本地 CA，把 mkcert 的根证书从系统信任库里移除。
///
/// 只动信任库，CAROOT 下的 CA 文件保留，重新签发时会自动装回去。
/// 卸载后浏览器等客户端不再信任本机签发的证书，
/// 但 Caddy 仍可用现有证书文件工作（客户端会报不受信任）。
#[tauri::command]
async fn uninstall_certificate() -> Result<(), String> {
    cert::uninstall().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_traffic() -> Result<traffic::TrafficReport, String> {
    Ok(traffic::analyze())
}

#[tauri::command]
async fn run_check() -> Result<Vec<CheckResult>, String> {
    let settings = config::load_settings();
    let active = config::resolve_active(&config::rules(&settings));
    Ok(check::run(&active).await)
}

/// 「关于」页的作者信息：GitHub 资料 + Minecraft 头像。
///
/// 断网或接口被墙时后端自动回落内置快照，所以永远成功、不返回 Err。
#[tauri::command]
async fn get_author_info() -> AuthorInfo {
    author::fetch().await
}

/// 导出的完整配置文档。带格式标识与版本号，导入端按它兼容解析。
#[derive(Serialize)]
struct ConfigExport<'a> {
    format: &'static str,
    version: u32,
    exported_at: u64,
    rules: &'a [ProxyRule],
}

/// 解析结果：可导入的规则列表 + 不影响导入的提示。
#[derive(Serialize)]
struct ConfigPreview {
    rules: Vec<ProxyRule>,
    warnings: Vec<String>,
}

/// 导出完整配置（全部规则，含顺序与测速结果）为 JSON 文本。
#[tauri::command]
fn export_config_text() -> Result<String, String> {
    let settings = config::load_settings();
    let rules = config::rules(&settings);
    let doc = ConfigExport {
        format: "mcspeed-rules",
        version: 1,
        exported_at: config::now_secs(),
        rules: &rules,
    };
    serde_json::to_string_pretty(&doc).map_err(|e| format!("序列化配置失败：{}", e))
}

/// 把导出的配置文本写进用户选好的文件。
#[tauri::command]
fn write_config_file(path: String, text: String) -> Result<(), String> {
    std::fs::write(&path, text).map_err(|e| format!("写入文件失败：{}", e))
}

/// 读取用户选中的配置文件全文，交给 parse_config 解析。
#[tauri::command]
fn read_config_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("读取文件失败：{}", e))
}

/// 从链接下载配置全文，交给 parse_config 解析。
#[tauri::command]
async fn fetch_config_text(url: String) -> Result<String, String> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err("只支持 http:// 或 https:// 链接。".into());
    }

    // 这里保留系统代理：导入配置是普通的上网下载，
    // 与测速不同，用户挂代理反而更容易拉到链接
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败：{}", e))?;

    let response = client
        .get(trimmed)
        .send()
        .await
        .map_err(|e| format!("下载失败：{}", e))?;
    if !response.status().is_success() {
        return Err(format!("链接返回 HTTP {}", response.status().as_u16()));
    }
    response
        .text()
        .await
        .map_err(|e| format!("读取链接内容失败：{}", e))
}

/// 解析配置文本（文件 / 剪贴板 / 链接三个来源共用）。
///
/// 兼容三种形态：完整导出信封、`{"rules": [...]}`、裸规则数组
/// （也就是 settings.json 的样子）。逐条做与手填规则一致的校验，
/// id 缺失或重复时自动补一个新的并给出提示。
#[tauri::command]
fn parse_config(text: String) -> Result<ConfigPreview, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("内容是空的。".into());
    }

    let value: serde_json::Value =
        serde_json::from_str(trimmed).map_err(|e| format!("不是合法的 JSON：{}", e))?;

    let items = match &value {
        serde_json::Value::Array(items) => items.clone(),
        serde_json::Value::Object(map) => map
            .get("rules")
            .and_then(|r| r.as_array())
            .cloned()
            .ok_or("找不到 rules 字段：配置里必须包含规则列表。")?,
        _ => return Err("顶层必须是规则数组，或包含 rules 字段的对象。".into()),
    };

    if items.is_empty() {
        return Err("配置里没有规则。".into());
    }

    let mut warnings = Vec::new();
    let mut rules = Vec::with_capacity(items.len());
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (index, item) in items.iter().enumerate() {
        let no = index + 1;
        let mut rule: ProxyRule = serde_json::from_value(item.clone())
            .map_err(|e| format!("第 {} 条规则格式不对：{}", no, e))?;

        // 与手填规则同一套归一化，避免导入一份带空格的配置后
        // 后续匹配行为和界面上看到的不一致
        rule.name = rule.name.trim().to_string();
        rule.path = rule.path.trim().to_string();
        rule.upstream = rule.upstream.trim().to_string();
        rule.route_prefix = rule.route_prefix.trim().to_string();
        rule.speed_group = rule.speed_group.trim().to_string();
        rule.domains = rule.domains.iter().map(|d| d.trim().to_string()).collect();

        if rule.id.trim().is_empty() || !seen.insert(rule.id.clone()) {
            if !rule.id.trim().is_empty() {
                warnings.push(format!("第 {} 条规则的 id 与其他规则重复，已重新生成。", no));
            }
            rule.id = config::new_rule_id();
            seen.insert(rule.id.clone());
        }

        rule.validate()
            .map_err(|e| format!("第 {} 条规则「{}」：{}", no, rule.name, e))?;
        rules.push(rule);
    }

    Ok(ConfigPreview { rules, warnings })
}

/// 用导入的规则整体替换当前配置，并同步 hosts 与 Caddy。
#[tauri::command]
async fn import_config(rules: Vec<ProxyRule>) -> Result<ProxyStatus, String> {
    if rules.is_empty() {
        return Err("导入的规则列表是空的。".into());
    }
    persist_and_sync(rules, true).await
}

/// 打开外链：用系统默认浏览器打开 http/https 地址。
///
/// 不走 @tauri-apps/plugin-opener 的 JS API：那边是动态 import，
/// 打包后容易整个失败且错误被吞掉，表现为「点了没反应」。
/// 这里直接调 opencrate 的 opener 入口，失败原因能带回前端弹窗。
#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err(format!("不支持打开这类地址：{}", trimmed));
    }

    tauri_plugin_opener::open_url(trimmed, None::<&str>).map_err(|e| e.to_string())
}

/// 打开 hosts 文件所在目录，方便用户手工查看。
#[tauri::command]
fn open_hosts_folder() -> Result<(), String> {
    let hosts_path = default_hosts_path();
    let folder = hosts_path.parent().unwrap_or(&hosts_path);

    Command::new("explorer")
        .arg(folder)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 远程调试端口。设了 MCSPEED_DEVTOOLS_PORT 就开，否则不传参数。
    //
    // 走 WebviewWindowBuilder::additional_browser_args 而不是环境变量：
    // WebView2 不读 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS，必须通过
    // ICoreWebView2EnvironmentOptions 设置，Tauri 只是把这个入口透传出来。
    let devtools_port = std::env::var("MCSPEED_DEVTOOLS_PORT").ok();

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_clipboard_manager::init());

    #[cfg(debug_assertions)]
    {
        builder = builder.plugin(tauri_plugin_mcp_bridge::init());
    }

    builder
        .setup(move |app| {
            use tauri::WebviewUrl;
            use tauri::WebviewWindowBuilder;

            let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("McSpeed")
                .inner_size(960.0, 780.0)
                .min_inner_size(760.0, 640.0)
                .decorations(false)
                .shadow(true)
                .center();

            if let Some(port) = &devtools_port {
                // --remote-allow-origins 必须显式放开，否则 WebView2 会拒绝
                // 非 localhost 来源的 CDP 连接请求
                let args = format!("--remote-debugging-port={port} --remote-allow-origins=*");
                builder = builder.additional_browser_args(&args);
                eprintln!(
                    "[mcspeed] CDP 调试端点：http://127.0.0.1:{}/json/list",
                    port
                );
            }

            let window = builder.build()?;

            // 无边框窗口下 WebView2 边缘会露出默认白底，
            // 这里把窗口背景压成深色，与前端 body 一致。
            let _ = window.set_background_color(Some(window_background_color()));

            // 带 MCSPEED_DEVTOOLS=1 时自动打开 DevTools 面板（仅 debug 构建）
            #[cfg(debug_assertions)]
            if std::env::var("MCSPEED_DEVTOOLS").is_ok() {
                window.open_devtools();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            add_rule,
            update_rule,
            delete_rule,
            set_rules,
            probe_rules,
            export_config_text,
            write_config_file,
            read_config_file,
            fetch_config_text,
            parse_config,
            import_config,
            toggle_proxy,
            setup_all,
            start_caddy,
            stop_caddy,
            reload_caddy,
            apply_hosts,
            remove_hosts,
            issue_certificate,
            uninstall_certificate,
            run_check,
            get_traffic,
            get_author_info,
            open_external_url,
            open_hosts_folder,
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}

/// 与前端 styles.css 里的 body 背景保持同一个颜色，避免边缘出现色差。
fn window_background_color() -> tauri::webview::Color {
    tauri::webview::Color(28, 28, 31, 255)
}
