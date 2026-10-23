use crate::config::{CADDY_ADMIN, LISTEN_PORT};
use serde::Serialize;
use std::io;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// 工程根目录。从当前 exe 位置向上找含 MCProxy\local 的目录。
pub fn project_root() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(PathBuf::from);
        while let Some(current) = dir {
            if current.join("MCProxy").join("local").is_dir() {
                return current;
            }
            dir = current.parent().map(PathBuf::from);
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn caddy_path() -> PathBuf {
    project_root()
        .join("MCProxy")
        .join("local")
        .join("caddy")
        .join("caddy.exe")
}

pub fn mkcert_path() -> PathBuf {
    project_root()
        .join("MCProxy")
        .join("local")
        .join("mkcert.exe")
}

pub fn caddyfile_path() -> PathBuf {
    project_root()
        .join("MCProxy")
        .join("local")
        .join("Caddyfile")
}

pub fn cert_dir() -> PathBuf {
    project_root()
        .join("MCProxy")
        .join("local")
        .join("certs")
}

pub fn cert_file() -> PathBuf {
    cert_dir().join("mcproxy.local.pem")
}

pub fn cert_key_file() -> PathBuf {
    cert_dir().join("mcproxy.local-key.pem")
}

pub fn access_log_path() -> PathBuf {
    project_root()
        .join("MCProxy")
        .join("local")
        .join("caddy-access.log")
}

/// 按当前启用的规则重新生成 Caddyfile。
pub fn write_caddyfile(rules: &[crate::config::ProxyRule]) -> std::io::Result<()> {
    let text = crate::caddyfile::render(rules, &access_log_path());
    std::fs::write(caddyfile_path(), text)
}

/// 443 是否在监听。
pub fn is_listening() -> bool {
    TcpStream::connect_timeout(
        &format!("127.0.0.1:{}", LISTEN_PORT)
            .parse()
            .expect("合法地址"),
        Duration::from_millis(200),
    )
    .is_ok()
}

/// Caddy 管理端点是否可用。
pub async fn is_admin_up() -> bool {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(800))
        .build();

    let client = match client {
        Ok(c) => c,
        Err(_) => return false,
    };

    client
        .get(format!("{}/config/", CADDY_ADMIN))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// 启动 Caddy。已在监听时直接返回。
pub fn start() -> io::Result<()> {
    if is_listening() {
        return Ok(());
    }

    let exe = caddy_path();
    if !exe.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("找不到 caddy.exe：{}", exe.display()),
        ));
    }

    let config = caddyfile_path();
    if !config.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("找不到 Caddyfile：{}", config.display()),
        ));
    }

    Command::new(&exe)
        .args([
            "run",
            "--config",
            &config.to_string_lossy(),
            "--adapter",
            "caddyfile",
        ])
        .current_dir(exe.parent().unwrap_or(std::path::Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    // 等端口起来，最多 15 秒
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
        if is_listening() {
            return Ok(());
        }
    }

    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "Caddy 启动超时：15 秒内未监听 443",
    ))
}

/// 停止 Caddy：按进程名结束整个进程树。
pub fn stop() -> StopResult {
    let mut killed = Vec::new();

    // Windows 上没有 SIGTERM，用 taskkill 结束进程树
    let output = Command::new("taskkill")
        .args(["/IM", "caddy.exe", "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output();

    match output {
        Ok(out) => {
            if out.status.success() {
                killed.push("caddy.exe".to_string());
            }
        }
        Err(e) => {
            return StopResult {
                killed,
                error: Some(e.to_string()),
            };
        }
    }

    StopResult { killed, error: None }
}

#[derive(Debug, Clone, Serialize)]
pub struct StopResult {
    pub killed: Vec<String>,
    pub error: Option<String>,
}

/// 通过管理端点热加载 Caddyfile。
pub async fn reload() -> Result<(), String> {
    let config = caddyfile_path();
    if !config.exists() {
        return Err(format!("找不到 Caddyfile：{}", config.display()));
    }

    let caddyfile = std::fs::read_to_string(&config)
        .map_err(|e| format!("读取 Caddyfile 失败：{}", e))?;

    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "config": caddyfile,
        "adapter": "caddyfile",
    });

    let response = client
        .post(format!("{}/load", CADDY_ADMIN))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求 Caddy 管理端点失败：{}", e))?;

    if !response.status().is_success() {
        let text = response
            .text()
            .await
            .unwrap_or_else(|_| "无响应体".into());
        return Err(format!("Caddy 热加载失败：{}", text));
    }

    Ok(())
}
