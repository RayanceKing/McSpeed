use crate::config::UPSTREAM;
use serde::Serialize;
use std::time::{Duration, Instant};

/// 一次连通性自检结果。
#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub name: String,
    pub success: bool,
    pub status_code: Option<u16>,
    pub detail: String,
    pub elapsed_ms: u128,
}

/// 逐个规则做真实请求，再确认线上 Worker 可达。
pub async fn run(rules: &[crate::config::ProxyRule]) -> Vec<CheckResult> {
    let mut results = Vec::new();

    for rule in rules.iter() {
        for domain in &rule.domains {
            // 通配域名没有具体主机可测，跳过
            if domain.trim().starts_with("*.") {
                continue;
            }
            results.push(check_domain(&domain.trim().to_ascii_lowercase()).await);
        }
    }

    results.push(check_upstream().await);
    results.push(check_proxy_env());
    results
}

/// 检查是否有代理环境变量会让请求绕过 hosts。
fn check_proxy_env() -> CheckResult {
    match crate::config::proxy_env_warning() {
        Some(warning) => CheckResult {
            name: "代理环境变量".into(),
            success: false,
            status_code: None,
            detail: warning,
            elapsed_ms: 0,
        },
        None => CheckResult {
            name: "代理环境变量".into(),
            success: true,
            status_code: None,
            detail: "未检测到会绕过 hosts 的代理".into(),
            elapsed_ms: 0,
        },
    }
}

async fn check_domain(domain: &str) -> CheckResult {
    let started = Instant::now();

    // 必须直连：走系统代理会绕过 hosts 直达线上，测的就不是本地链路
    let client = match reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(20))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return CheckResult {
                name: domain.into(),
                success: false,
                status_code: None,
                detail: e.to_string(),
                elapsed_ms: started.elapsed().as_millis(),
            };
        }
    };

    let response = match client
        .get(format!("https://{}/", domain))
        .header("User-Agent", "MCSpeed/2.0")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return CheckResult {
                name: domain.into(),
                success: false,
                status_code: None,
                detail: e.to_string(),
                elapsed_ms: started.elapsed().as_millis(),
            };
        }
    };

    let elapsed = started.elapsed().as_millis();
    let code = response.status().as_u16();
    let headers = response.headers().clone();

    if !response.status().is_success() {
        return CheckResult {
            name: domain.into(),
            success: false,
            status_code: Some(code),
            detail: format!("上游返回 {}", code),
            elapsed_ms: elapsed,
        };
    }

    let via_local = headers
        .get("x-local-proxy")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("MCProxy/Caddy"))
        .unwrap_or(false);

    if !via_local {
        return CheckResult {
            name: domain.into(),
            success: false,
            status_code: Some(code),
            detail: "响应未经过本地 Caddy，hosts 可能未生效".into(),
            elapsed_ms: elapsed,
        };
    }

    let route = headers
        .get("x-proxy-route")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("?")
        .to_string();

    CheckResult {
        name: domain.into(),
        success: true,
        status_code: Some(code),
        detail: format!("路由 {}，经本地代理", route),
        elapsed_ms: elapsed,
    }
}

async fn check_upstream() -> CheckResult {
    let started = Instant::now();
    let name = "线上 Worker".to_string();

    let client = match reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(20))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return CheckResult {
                name,
                success: false,
                status_code: None,
                detail: e.to_string(),
                elapsed_ms: started.elapsed().as_millis(),
            };
        }
    };

    let response = match client.get(format!("{}/__ping", UPSTREAM)).send().await {
        Ok(r) => r,
        Err(e) => {
            return CheckResult {
                name,
                success: false,
                status_code: None,
                detail: e.to_string(),
                elapsed_ms: started.elapsed().as_millis(),
            };
        }
    };

    let elapsed = started.elapsed().as_millis();
    let code = response.status().as_u16();

    if !response.status().is_success() {
        return CheckResult {
            name,
            success: false,
            status_code: Some(code),
            detail: "Worker 无响应".into(),
            elapsed_ms: elapsed,
        };
    }

    let body = response.text().await.unwrap_or_default();
    let colo = body
        .split("\"colo\"")
        .nth(1)
        .and_then(|s| s.split('"').nth(1))
        .unwrap_or("?")
        .to_string();

    CheckResult {
        name,
        success: true,
        status_code: Some(code),
        detail: format!("节点 {}", colo),
        elapsed_ms: elapsed,
    }
}
