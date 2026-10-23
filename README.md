# McSpeed

本地加速代理的管理工具。

`maven.fabricmc.net` 等域名在国内直连经常只有几十 KB/s，瓶颈在跨境线路而不是 DNS。McSpeed 的做法是**把域名劫持到本机，用本地 Caddy 接管，再转发到部署在 Cloudflare 边缘的 Worker**：

```
Gradle / 启动器
   ↓ https://maven.fabricmc.net/...
hosts → 127.0.0.1
   ↓
Caddy :443（mkcert 本地证书，TLS 终止）
   ↓ 路径前补 /fabric-maven，Host 改为 mcproxy.unmod.online
Cloudflare Worker（mcproxy.unmod.online）
   ↓
maven.fabricmc.net
```

配套的 Worker 项目在 [../FabricProxy](../FabricProxy)，只开放 Fabric 三条路由。

## 功能

- **一键配置**：签发本地证书 → 写入 hosts → 启动 Caddy，一条命令拉通整条链路。
- **分步操作**：证书、hosts、Caddy 的启动/停止/热加载都可单独执行。
- **连通性自检**：对三个域名发真实请求，通过 `X-Local-Proxy` 与 `X-Proxy-Route` 响应头确认请求确实经过了本地 Caddy 和 Worker，顺带测出 Worker 所在节点。
- **hosts 管理**：只操作带 `# McSpeed managed` 标记的块，写入前自动备份，不影响你自己的 hosts 内容。

## 界面

用 [mcui-oreui](https://github.com/ShenYuanOR/mcui-oreui)（Minecraft 基岩版 Ore UI 设计语言的 Vue 3 组件库）做界面，跑在 Tauri 里。

## 环境要求

- Node 20+
- Rust 1.77+
- 同级目录存在 `FabricProxy/local/`，内含 `caddy.exe`、`mkcert.exe`、`Caddyfile`
- 管理员权限（写 hosts、装 CA、占 443 都需要）

## 开发

```powershell
npm install
npm run tauri:dev
```

## 构建

```powershell
npm run tauri:build
```

产物在 `src-tauri/target/release/`。

## 检查

```powershell
npx vue-tsc --noEmit   # 前端类型检查
```

前端类型检查确认与 mcui-oreui 2.0 的 props/插槽契约一致。

## 目录结构

```
McSpeed/
  src/
    main.ts        Vue 入口，注册 mcui-oreui 插件
    App.vue        主界面：状态、操作、域名表、自检、hosts 记录
    api.ts         Tauri 命令封装
    types.ts       与 Rust 结构体对应的类型 + checkSummary()
  src-tauri/
    src/
      config.rs    常量与域名清单
      hosts.rs     hosts 读写，只管理标记块
      caddy.rs     Caddy 进程控制与热加载
      cert.rs      mkcert 证书签发
      check.rs     连通性自检
      lib.rs       Tauri 命令注册
    Cargo.toml
    tauri.conf.json
```

## 使用

1. 以管理员身份启动（首次会弹 UAC）。
2. 点「一键配置」。
3. 在 Gradle 里把仓库地址从 `https://maven.fabricmc.net/` 换成 `https://maven.fabricmc.net/`——对，不用改，hosts 已经指过来了。

验证是否生效：

```powershell
curl.exe -s -D - -o NUL https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.15.0/fabric-loader-0.15.0.jar
```

响应头里应看到 `x-local-proxy: FabricProxy/Caddy` 和 `x-proxy-route: /fabric-maven`。前者证明走了本地 Caddy，后者证明 Worker 正确识别了路由。

## 已知限制

- hosts 劫持是**全局**的：本机所有程序访问这三个域名都会走本地链路。Caddy 没启动时这些域名直接不可达，不会回落到直连。
- 443 被占用时 Caddy 起不来，先用「停止 Caddy」或释放端口。
- 自检走 `no_proxy` 直连。如果系统设了强制代理，可能绕过 hosts。
- 证书有效期到 2029-01-22，过期后点「重新签发证书」。
- `fabricmc.net` 整站代理收益有限，主要是官网页面；真正有用的是 `maven.fabricmc.net` 和 `meta.fabricmc.net`。
