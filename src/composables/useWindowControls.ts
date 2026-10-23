import { getCurrentWindow } from "@tauri-apps/api/window";
import { onMounted, ref } from "vue";

/**
 * 自绘标题栏用的窗口控制。
 *
 * 窗口配置为 decorations: false，系统标题栏被去掉，
 * 最小化/最大化/关闭/拖拽都由前端调用 Tauri 命令完成。
 */
export function useWindowControls() {
  const isMaximized = ref(false);

  onMounted(() => {
    void getCurrentWindow()
      .isMaximized()
      .then((v) => {
        isMaximized.value = v;
      })
      .catch(() => {
        isMaximized.value = false;
      });
  });

  async function minimize() {
    await getCurrentWindow().minimize();
  }

  async function toggleMaximize() {
    await getCurrentWindow().toggleMaximize();
    isMaximized.value = await getCurrentWindow().isMaximized();
  }

  async function close() {
    await getCurrentWindow().close();
  }

  /** 按住标题栏拖动窗口。需要 core:window:allow-start-dragging 权限。 */
  async function startDrag() {
    try {
      await getCurrentWindow().startDragging();
    } catch {
      // 拖拽失败不影响其他操作
    }
  }

  return { isMaximized, minimize, toggleMaximize, close, startDrag };
}
