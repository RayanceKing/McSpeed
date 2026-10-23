use crate::caddy::{cert_dir, cert_file, cert_key_file, mkcert_path};
use std::io;
use std::process::{Command, Stdio};

/// 证书是否已生成。
pub fn exists() -> bool {
    cert_file().exists() && cert_key_file().exists()
}

/// mkcert 是否存在。
pub fn mkcert_installed() -> bool {
    mkcert_path().exists()
}

/// 用 mkcert 生成本地证书并装入系统信任根。
///
/// `-install` 需要管理员权限，Windows 上会弹 UAC。
pub fn issue() -> io::Result<()> {
    let mkcert = mkcert_path();
    if !mkcert.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("找不到 mkcert.exe：{}", mkcert.display()),
        ));
    }

    std::fs::create_dir_all(cert_dir())
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("创建证书目录失败：{}", e)))?;

    // 1. 安装 CA 到系统信任根（提权）
    let install = Command::new(&mkcert)
        .arg("-install")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match install {
        Ok(status) if status.success() => {}
        Ok(status) => {
            // 用户取消 UAC 时退出码非 0，此时不应继续
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("mkcert -install 失败，退出码 {:?}（是否取消了 UAC？）", status.code()),
            ));
        }
        Err(e) => return Err(e),
    }

    // 2. 生成证书，SAN 覆盖所有 Fabric 域名与本机回环。
    // 域名清单固定写全：证书签宽一点，停用某个域名时不用重新签发。
    let mut names: Vec<String> = vec![
        "maven.fabricmc.net".into(),
        "meta.fabricmc.net".into(),
        "fabricmc.net".into(),
    ];
    names.extend(["localhost".into(), "127.0.0.1".into(), "::1".into()]);

    let mut command = Command::new(&mkcert);
    command
        .arg("-cert-file")
        .arg(cert_file())
        .arg("-key-file")
        .arg(cert_key_file())
        .args(&names)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let status = command.status()?;
    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("mkcert 生成证书失败，退出码 {:?}", status.code()),
        ));
    }

    if !exists() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "mkcert 执行完成但证书文件未生成",
        ));
    }

    Ok(())
}

/// 卸载 mkcert 的本地 CA，从系统信任根里移除。
///
/// 只动信任库，不删 `CAROOT` 下的 CA 文件——mkcert 的 `-uninstall`
/// 语义就是「保留 CA 但不再信任」。下次点重新签发时 `-install` 会把它
/// 装回去，用户不必重新生成一套 CA。
///
/// 需要管理员权限，Windows 上会弹 UAC。
pub fn uninstall() -> io::Result<()> {
    let mkcert = mkcert_path();
    if !mkcert.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("找不到 mkcert.exe：{}", mkcert.display()),
        ));
    }

    // TRUST_STORES 固定成 system：本项目的证书只装过系统根。
    // 不设的话 mkcert 会同时去碰 Java 与 NSS（Firefox）信任库，
    // 其中任何一步的非零退出码都会让整个卸载失败——
    // 实测在装了 JDK 的机器上，keytool -delete 会因为
    // 「别名不存在」返回 1，而 mkcert 只对英文输出做 contains 判断，
    // 中文系统上的提示信息匹配不上，于是误判成卸载失败。
    let status = Command::new(&mkcert)
        .arg("-uninstall")
        .env("TRUST_STORES", "system")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if !status.success() {
        // CA 本来就没装时 mkcert 会以非零码退出，这不算失败
        if is_ca_installed() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("mkcert -uninstall 失败，退出码 {:?}", status.code()),
            ));
        }
    }

    Ok(())
}

/// CA 是否还留在系统信任根里。
///
/// 用 certutil 查当前用户的 Root 存储。mkcert 装的就是这个位置：
/// 它调用 `CertOpenSystemStoreW(0, "ROOT")`，传 0 表示当前用户。
/// 查不到（或 certutil 本身不可用）时返回 false，交给调用方按「已卸载」处理。
fn is_ca_installed() -> bool {
    let output = Command::new("certutil")
        .args(["-user", "-store", "Root"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    match output {
        Ok(out) => certutil_lists_ca(&out.stdout),
        // certutil 不存在时无法判断，保守地当作已卸载
        Err(_) => false,
    }
}

/// 从 `certutil -user -store Root` 的输出里判断 mkcert 的 CA 是否还在。
///
/// 单独拆成函数是为了能单测：不能靠真的去改系统信任库来验证。
fn certutil_lists_ca(stdout: &[u8]) -> bool {
    // certutil 在中文 Windows 上按本地代码页输出，整段转 UTF-8 会丢字符，
    // 但 CA 的主题名是纯 ASCII（"mkcert development CA"），
    // 因此按 ASCII 匹配即可，不依赖系统语言。
    let text = String::from_utf8_lossy(stdout);
    text.contains(CA_SUBJECT_MARKER)
}

/// mkcert 根证书主题里的固定片段。
///
/// 取自 cert.go 的 `newCA()`：Organization 恒为 "mkcert development CA"，
/// 与用户名、机器名无关，所以可以安全地当作识别标记。
const CA_SUBJECT_MARKER: &str = "mkcert development CA";
