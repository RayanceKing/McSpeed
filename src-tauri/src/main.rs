// 二进制入口：逻辑都在 lib.rs，这里只负责调用
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 调试开关都在 lib.rs 的 setup 里处理：
    //   MCSPEED_DEVTOOLS_PORT=9333  开远程 CDP，可用脚本连上操作页面
    //   MCSPEED_DEVTOOLS=1          自动弹 DevTools 面板（仅 debug 构建）
    //
    // 曾经试过设 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS 环境变量，无效：
    // WebView2 只认 ICoreWebView2EnvironmentOptions，不读环境变量。
    // 现在走 WebviewWindowBuilder::additional_browser_args，是 Tauri 透传的入口。
    mcspeed_lib::run()
}
