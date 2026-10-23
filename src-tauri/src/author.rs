//! 「关于」页的作者信息：GitHub 资料 + Microsoft/Minecraft 头像。
//!
//! 头像按 Minecraft 正版用户名（Microsoft 账号同名）从 mc-heads 取皮肤头部渲染图，
//! GitHub 资料取自 api.github.com。两边都是公网请求，断网或被墙时回落到内置快照——
//! 关于页不该因为连不上外网就缺一块。

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// GitHub 登录名。Minecraft 正版用户名与它同名（Microsoft 账号）。
pub const LOGIN: &str = "Ayndpa";

/// Minecraft 正版用户名，头像按这个名字取皮肤。
pub const MC_NAME: &str = "Ayndpa";

/// GitHub 用户资料接口。
const GITHUB_API_URL: &str = "https://api.github.com/users/Ayndpa";

/// mc-heads 直接把皮肤渲染成头部头像，省去自己解析皮肤贴图。
const AVATAR_URL: &str = "https://mc-heads.net/avatar/Ayndpa/128";

/// 作者信息不是关键路径，取不到就回落，超时控制得短。
const TIMEOUT: Duration = Duration::from_secs(5);

/// 前端「关于」页直接渲染的结构，字段名与 types.ts 的 AuthorInfo 对齐。
#[derive(Debug, Clone, Serialize)]
pub struct AuthorInfo {
    /// 显示名（GitHub 资料里的 name）。
    pub name: String,
    /// GitHub 登录名。
    pub login: String,
    pub bio: String,
    pub location: String,
    pub github_url: String,
    /// 赞助 / 主页链接，取自 GitHub blog 字段或 bio 里的链接。
    pub sponsor_url: String,
    pub public_repos: u32,
    pub followers: u32,
    /// Minecraft 正版用户名，与 Microsoft 账号同名。
    pub mc_name: String,
    /// Minecraft 头像的 data URL。快照模式下为空串，前端显示占位。
    pub avatar: String,
    /// live=实时拉取；snapshot=内置快照。
    pub source: String,
}

/// GitHub 用户接口里我们关心的字段。缺关键字段就当解析失败。
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubUser {
    pub login: String,
    pub name: Option<String>,
    pub bio: Option<String>,
    pub location: Option<String>,
    pub blog: Option<String>,
    pub html_url: String,
    pub public_repos: u32,
    pub followers: u32,
}

/// 实时拉取作者信息。任何一环失败都不影响返回——最差也是内置快照。
pub async fn fetch() -> AuthorInfo {
    let client = match reqwest::Client::builder().timeout(TIMEOUT).build() {
        Ok(c) => c,
        Err(_) => return snapshot(),
    };

    fetch_from(&client, GITHUB_API_URL, AVATAR_URL).await
}

/// 可注入地址的版本，测试里指向 .invalid 验证失败回落。
///
/// GitHub 资料与头像并行拉，两个都慢时也只等一轮超时。
async fn fetch_from(client: &reqwest::Client, github_url: &str, avatar_url: &str) -> AuthorInfo {
    let (github, avatar) = tokio::join!(
        fetch_github(client, github_url),
        fetch_avatar(client, avatar_url)
    );

    let mut info = snapshot();
    if let Ok(user) = github {
        info = merge_github(&info, &user);
        info.source = "live".into();
    }
    if let Ok(avatar) = avatar {
        info.avatar = avatar;
    }
    info
}

/// 内置快照：实时请求失败时的兜底，取值来自 2026-10 的 GitHub 资料。
pub fn snapshot() -> AuthorInfo {
    AuthorInfo {
        name: "Ji Fuyao".into(),
        login: LOGIN.into(),
        bio: "Always in 爱发电：https://ifdian.net/a/ayndpa".into(),
        location: "山东青岛".into(),
        github_url: format!("https://github.com/{}", LOGIN),
        sponsor_url: "https://ifdian.net/a/ayndpa".into(),
        public_repos: 80,
        followers: 28,
        mc_name: MC_NAME.into(),
        avatar: String::new(),
        source: "snapshot".into(),
    }
}

/// 把实时 GitHub 资料盖到快照上；头像单独传，拉不到就留空。
fn merge_github(base: &AuthorInfo, user: &GitHubUser) -> AuthorInfo {
    let mut info = base.clone();
    info.name = user
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| user.login.clone());
    info.login = user.login.clone();
    info.bio = user.bio.clone().unwrap_or_default();
    info.location = user.location.clone().unwrap_or_default();
    info.github_url = user.html_url.clone();
    info.sponsor_url = extract_link(user);
    info.public_repos = user.public_repos;
    info.followers = user.followers;
    info
}

/// 主页 / 赞助链接：blog 字段优先，没有就从 bio 里抠第一段 http 链接。
fn extract_link(user: &GitHubUser) -> String {
    let blog = user.blog.as_deref().unwrap_or("").trim();
    if !blog.is_empty() {
        return blog.to_string();
    }

    let bio = user.bio.as_deref().unwrap_or("");
    if let Some(start) = bio.find("http") {
        let rest = &bio[start..];
        let end = rest
            .find(|c: char| c.is_whitespace())
            .unwrap_or(rest.len());
        return rest[..end].trim_end_matches(|c| c == '。' || c == '，' || c == '、').to_string();
    }
    String::new()
}

/// 解析 GitHub 用户接口响应体。字段缺失直接报错，方便测试兜底。
fn parse_github_user(text: &str) -> Result<GitHubUser, String> {
    serde_json::from_str(text).map_err(|e| format!("GitHub 响应解析失败：{}", e))
}

/// 标准 base64（带 padding）。只为把头像转 data URL，不值得为此引依赖。
fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((*chunk.get(1).unwrap_or(&0) as u32) << 8)
            | (*chunk.get(2).unwrap_or(&0) as u32);

        out.push(TABLE[(n >> 18) as usize & 0x3f] as char);
        out.push(TABLE[(n >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

/// 头像转 data URL。前端 CSP 只放行 self + data，不能直接引外链图片。
fn png_to_data_url(bytes: &[u8]) -> Result<String, String> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("头像响应不是 PNG 图片".into());
    }
    Ok(format!("data:image/png;base64,{}", encode_base64(bytes)))
}

/// 拉 GitHub 用户资料。
async fn fetch_github(client: &reqwest::Client, url: &str) -> Result<GitHubUser, String> {
    // GitHub API 不带 User-Agent 会 403
    let text = client
        .get(url)
        .header("User-Agent", "McSpeed")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("请求 GitHub 失败：{}", e))?
        .error_for_status()
        .map_err(|e| format!("GitHub 返回错误：{}", e))?
        .text()
        .await
        .map_err(|e| format!("读取 GitHub 响应失败：{}", e))?;

    parse_github_user(&text)
}

/// 拉 Minecraft 头像并转成 data URL。
async fn fetch_avatar(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let bytes = client
        .get(url)
        .header("User-Agent", "McSpeed")
        .send()
        .await
        .map_err(|e| format!("请求头像失败：{}", e))?
        .error_for_status()
        .map_err(|e| format!("头像服务返回错误：{}", e))?
        .bytes()
        .await
        .map_err(|e| format!("读取头像失败：{}", e))?;

    png_to_data_url(&bytes)
}
