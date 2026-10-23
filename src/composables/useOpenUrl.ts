/**
 * 用系统默认浏览器打开 URL。
 *
 * 走 Rust 命令而不是 @tauri-apps/plugin-opener 的 JS API：动态 import 在
 * prod 产物里会被打成 chunk 加载，WebView2 里容易整体失败且错误被吞掉，
 * 表现为「点了没有任何反应」。Rust 侧还能做只放行 http/https 的白名单。
 */
import { invoke } from "@tauri-apps/api/core";
import { showDialog } from "./useDialog";

export async function openExternal(url: string): Promise<void> {
  try {
    await invoke("open_external_url", { url });
  } catch (err) {
    showDialog("无法打开链接", err);
  }
}

