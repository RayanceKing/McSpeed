<script setup lang="ts">
import { onMounted, ref } from "vue";
import HelpTip from "../components/HelpTip.vue";
import { showDialog } from "../composables/useDialog";
import {
  getTraffic,
  issueCertificate,
  reloadCaddy,
  runCheck,
  startCaddy,
  stopCaddy,
  uninstallCertificate,
} from "../api";
import type { CheckResult, ProxyStatus, RecentRequest } from "../types";
import { checkSummary, formatBytes } from "../types";

const props = defineProps<{
  status: ProxyStatus | null;
}>();

const recent = ref<RecentRequest[]>([]);
const busy = ref(false);
// 卸载证书要二次确认：这一步会把系统信任根里的 CA 摘掉，
// 误点之后所有依赖该 CA 的本地 HTTPS 会立刻变成不受信任。
const confirmingUninstall = ref(false);

onMounted(() => {
  void loadRecent();
});

async function loadRecent() {
  try {
    const traffic = await getTraffic();
    recent.value = traffic.recent;
  } catch {
    recent.value = [];
  }
}

async function run(label: string, action: () => Promise<unknown>) {
  if (busy.value) return;
  busy.value = true;
  try {
    await action();
    showDialog(`${label}完成`, "操作已成功执行。", "知道了");
    await loadRecent();
  } catch (err) {
    showDialog(`${label}失败`, err);
  } finally {
    busy.value = false;
  }
}

function confirmUninstall() {
  if (busy.value) return;
  confirmingUninstall.value = true;
}

async function onUninstallConfirmed() {
  confirmingUninstall.value = false;
  await run("卸载证书", uninstallCertificate);
}

async function onCheck() {
  if (busy.value) return;
  busy.value = true;
  try {
    const results = await runCheck();

    const failed = results.filter((r) => !r.success);
    if (failed.length === 0) {
      showDialog(
        "自检通过",
        `${results.length} 项全部正常。\n\n${results.map((r) => `· ${r.name}：${checkSummary(r)}`).join("\n")}`,
        "知道了"
      );
    } else {
      // 把失败项排前面，用户第一眼看到问题
      const ordered = [...failed, ...results.filter((r) => r.success)];
      showDialog(
        `自检发现 ${failed.length} 项异常`,
        ordered
          .map((r) => `${r.success ? "·" : "×"} ${r.name}：${r.detail || checkSummary(r)}`)
          .join("\n"),
        "知道了"
      );
    }
  } catch (err) {
    showDialog("自检失败", err);
  } finally {
    busy.value = false;
  }
}

</script>

<template>
  <div class="page">
    <!-- 链路自检 -->
    <mc-card>
      <template #title>
        <span class="card-title">
          <mc-icon name="mc-magnifying-glass" :size="16" class="card-icon" />
          链路自检
          <HelpTip text="对每个启用的域名发一次真实请求，再检查线上 Worker。通过响应头判断请求是否真的经过了本地 Caddy 和 Worker：x-local-proxy 存在说明走了本地，x-proxy-route 说明 Worker 正确识别了路由。任一一环缺失都会标记为失败。" />
        </span>
      </template>
      <template #text>
        <!-- 代理警告只跟自检结果相关（curl 等会绕过 hosts 直连），放卡内不占页面顶部 -->
        <mc-alert v-if="status?.proxy_warning" class="proxy-warning" variant="warning">
          {{ status.proxy_warning }}
        </mc-alert>
        <div class="actions">
          <mc-button variant="primary" icon="mc-magnifying-glass" :loading="busy" @click="onCheck">运行自检</mc-button>
        </div>
      </template>
    </mc-card>

    <!-- 分步操作 -->
    <mc-card>
      <template #title>
        <span class="card-title">
          <mc-icon name="mc-settings" :size="16" class="card-icon" />
          分步操作
          <HelpTip text="总开关是这些步骤的组合。单独执行某一步用于排查问题，例如证书报错时只重签证书。每步都可以反复执行，不会互相干扰。" />
        </span>
      </template>
      <template #text>
        <div class="group">
          <span class="group-title">Caddy</span>
          <div class="actions">
            <mc-button icon="mc-activate" :disabled="busy" @click="run('启动 Caddy', startCaddy)">启动</mc-button>
            <mc-button icon="mc-stop" :disabled="busy" @click="run('停止 Caddy', stopCaddy)">停止</mc-button>
            <mc-button icon="mc-reload" :disabled="busy" @click="run('热加载配置', reloadCaddy)">热加载</mc-button>
          </div>
        </div>

        <div class="group">
          <span class="group-title">证书</span>
          <div class="actions">
            <mc-button icon="mc-locked" :disabled="busy" @click="run('签发证书', issueCertificate)">重新签发</mc-button>
            <mc-button variant="error" icon="mc-lock-unlocked" :disabled="busy" @click="confirmUninstall">
              卸载证书
            </mc-button>
          </div>
        </div>

        <p class="hint">
          写 hosts、装 CA、占用 443 都需要管理员权限，操作时会弹 UAC。
          hosts 的写入与删除在「hosts」页操作。
        </p>
      </template>
    </mc-card>

    <!-- 最近请求 -->
    <mc-card v-if="recent.length > 0">
      <template #title>
        <span class="card-title">
          <mc-icon name="mc-list" :size="16" class="card-icon" />
          最近请求
          <HelpTip text="从 access log 读到的最近若干条请求，含状态码、响应大小与耗时。用来确认流量确实在往本地链路走。" />
        </span>
      </template>
      <template #text>
        <mc-list>
          <mc-list-item
            v-for="(r, i) in recent"
            :key="`${r.host}-${i}`"
            :label="r.host"
            :subtitle="`${r.status} · ${formatBytes(r.size)} · ${r.duration_ms} ms`"
            :value="`${r.host}-${i}`"
            icon="mc-enter"
            :interactive="false"
          >
            <template #right>
              <span class="mono dim">{{ r.ago_secs }} 秒前</span>
            </template>
          </mc-list-item>
        </mc-list>
      </template>
    </mc-card>

    <!-- 卸载证书的二次确认。破坏性操作不能一个按钮就执行下去 -->
    <mc-confirm
      v-model="confirmingUninstall"
      title="卸载本地 CA"
      confirm-text="卸载"
      cancel-text="取消"
      danger
      @confirm="onUninstallConfirmed"
    >
      将把 mkcert 的根证书从系统信任库中移除，本机不再信任 MC Speed 签发的证书。

      磁盘上的 CA 与域名证书文件会保留，之后点「重新签发」即可自动装回并恢复信任。
      Caddy 若仍在运行，卸载后客户端访问 Fabric 域名会提示证书不受信任。
    </mc-confirm>
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
}

.card-title {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.card-icon {
  opacity: 0.7;
}

.hint {
  opacity: 0.7;
  font-size: 13px;
  line-height: 1.6;
  margin: 4px 0 12px;
}

.mono {
  font-family: Consolas, "Cascadia Mono", monospace;
  font-size: 12px;
}

.dim {
  opacity: 0.65;
  font-size: 12px;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.proxy-warning {
  margin-bottom: 12px;
}

.group {
  margin-bottom: 14px;
}

.group-title {
  display: block;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  opacity: 0.5;
  margin-bottom: 6px;
}
</style>
