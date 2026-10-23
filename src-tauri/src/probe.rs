//! 规则测速：下载每个规则自己的上游内容，用「下载一个文件的用时」衡量快慢。
//!
//! 用户可能给同一个域名配了多条转发规则指向不同上游（例如两个 Worker
//! 域名），此时只有下载最快（用时最小）的那条真正生效，见
//! `config::resolve_active`。测速结果连同组内排序一起落盘，
//! 重启后按钮上依然是上次的速度。

use crate::config::{
    now_secs, sort_groups_by_probe, ProxyRule, PROBE_MAX_BYTES, PROBE_TIMEOUT_SECS,
};
use serde::Serialize;
use std::time::{Duration, Instant};

/// 一条规则的测速结果。
#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    /// 被测规则的 id。
    pub rule_id: String,
    /// 规则名，方便界面直接展示。
    pub rule_name: String,
    /// 所属测速分组；空串表示不参与竞争。
    pub speed_group: String,
    /// 测速用的 URL（本条规则自己的上游）。
    pub url: String,
    /// 下载用时（毫秒）。None 表示测速失败。
    pub ms: Option<u64>,
    /// 下载的字节数，与用时一起换算成速度。
    pub bytes: u64,
    /// 失败原因，成功时为空串。
    pub detail: String,
    /// 是否是同组里当前最快的一条。
    pub selected: bool,
}

/// 测速给定的转发规则，返回每条的结果（与入参同序）。
///
/// 每条规则打各自的上游、真实下载一段内容，并行执行互不等待。
pub async fn probe(rules: &[ProxyRule]) -> Vec<ProbeResult> {
    // 客户端不能带代理：走系统代理会绕开真实出口路径，
    // 测出来的就不是用户实际会遇到的速度了
    let client = match reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(PROBE_TIMEOUT_SECS))
        // 接受自签证书：上游可能是本机 Caddy 用 mkcert 签的证书，测速不该被它挡住
        .danger_accept_invalid_certs(true)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return rules
                .iter()
                .map(|r| failed(r, format!("创建 HTTP 客户端失败：{}", e)))
                .collect();
        }
    };

    // 并行测：每条规则一个任务，按入参顺序收集结果。
    // 串行的话每条都要等满超时预算，规则一多整个测速要几十秒。
    let mut handles = Vec::with_capacity(rules.len());
    for rule in rules {
        let client = client.clone();
        let rule = rule.clone();
        handles.push(tokio::spawn(async move { probe_one(&client, &rule).await }));
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        if let Ok(result) = handle.await {
            results.push(result);
        }
    }

    mark_selected(&mut results, rules);
    results
}

async fn probe_one(client: &reqwest::Client, rule: &ProxyRule) -> ProbeResult {
    let url = rule.probe_url();
    let started = Instant::now();

    let mut response = match client.get(&url).send().await {
        Ok(response) => response,
        Err(e) => {
            let elapsed = started.elapsed().as_millis() as u64;
            let detail = if e.is_timeout() {
                format!("超时（{} ms）", elapsed)
            } else {
                e.to_string()
            };
            return failed(rule, detail).with_url(url);
        }
    };

    let status = response.status().as_u16();
    // 4xx 说明链路是通的，只是这个具体路径没有内容，测速数据依然有效
    if !(response.status().is_success() || response.status().is_client_error()) {
        let elapsed = started.elapsed().as_millis() as u64;
        return failed(rule, format!("HTTP {}（{} ms）", status, elapsed)).with_url(url);
    }

    // 真实下载正文：速度 = 字节数 ÷ 用时，用时也用来在组内排序
    let mut bytes: u64 = 0;
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                bytes += chunk.len() as u64;
                if bytes >= PROBE_MAX_BYTES {
                    break;
                }
            }
            Ok(None) => break,
            Err(e) => {
                let elapsed = started.elapsed().as_millis() as u64;
                return failed(rule, format!("下载中断：{}（{} ms）", e, elapsed)).with_url(url);
            }
        }
    }

    let elapsed = started.elapsed().as_millis() as u64;
    if bytes == 0 {
        return failed(rule, format!("HTTP {} 但响应为空，测不出速度", status)).with_url(url);
    }

    ProbeResult {
        rule_id: rule.id.clone(),
        rule_name: rule.name.clone(),
        speed_group: rule.speed_group.trim().to_string(),
        url,
        // 用时至少记 1ms，前端换算速度时不会除以 0
        ms: Some(elapsed.max(1)),
        bytes,
        detail: String::new(),
        selected: false,
    }
}

/// 标记同组里下载用时最小的那条，供界面高亮。
fn mark_selected(results: &mut [ProbeResult], rules: &[ProxyRule]) {
    // 组名 -> (用时, 结果下标)
    let mut winners: std::collections::HashMap<String, (u64, usize)> =
        std::collections::HashMap::new();

    for (index, result) in results.iter().enumerate() {
        if result.speed_group.is_empty() {
            continue;
        }
        let Some(ms) = result.ms else { continue };

        match winners.get(&result.speed_group) {
            None => {
                winners.insert(result.speed_group.clone(), (ms, index));
            }
            Some(&(best, _)) if ms < best => {
                winners.insert(result.speed_group.clone(), (ms, index));
            }
            _ => {}
        }
    }

    for (_, (_, index)) in winners {
        results[index].selected = true;
    }

    // 组里没有一条测成功时不能全都不选：那样用户看不出当前走哪条。
    // 退化成组内第一条规则，与 resolve_active 的兜底保持一致
    let mut groups: std::collections::HashSet<String> = std::collections::HashSet::new();
    for result in results.iter() {
        if !result.speed_group.is_empty() {
            groups.insert(result.speed_group.clone());
        }
    }

    for group in groups {
        if results.iter().any(|r| r.speed_group == group && r.selected) {
            continue;
        }
        let fallback = rules.iter().find(|r| r.speed_group.trim() == group);
        if let Some(rule) = fallback {
            if let Some(result) = results.iter_mut().find(|r| r.rule_id == rule.id) {
                result.selected = true;
                result.detail = "组内无可用测速结果，暂时使用本条".into();
            }
        }
    }
}

/// 构造一条失败结果。
fn failed(rule: &ProxyRule, detail: String) -> ProbeResult {
    ProbeResult {
        rule_id: rule.id.clone(),
        rule_name: rule.name.clone(),
        speed_group: rule.speed_group.trim().to_string(),
        url: String::new(),
        ms: None,
        bytes: 0,
        detail,
        selected: false,
    }
}

impl ProbeResult {
    fn with_url(mut self, url: String) -> Self {
        self.url = url;
        self
    }
}

/// 把测速结果写回规则并保存。
///
/// 失败的规则清掉旧数据：留着一段旧数据会让它在组内排序时
/// 凭「有数据」排在真正测通的对手前面。
/// 写完结果后按用时把本次测过的组内部重排，最快的一条落到组内第一位，
/// 调用方把整个列表持久化即可同时保住测速结果和排序。
pub fn apply_results(rules: &mut [ProxyRule], results: &[ProbeResult]) {
    use std::collections::HashSet;

    let now = now_secs();
    // 只重排本次测过的组，没测的组保持用户手动排的顺序
    let mut touched: HashSet<String> = HashSet::new();

    for rule in rules.iter_mut() {
        let Some(result) = results.iter().find(|r| r.rule_id == rule.id) else {
            continue;
        };
        rule.probe_ms = result.ms;
        rule.probe_bytes = result.bytes;
        rule.probe_at = if result.ms.is_some() { now } else { 0 };
        touched.insert(crate::config::group_key(rule));
    }

    sort_groups_by_probe(rules, &touched);
}
