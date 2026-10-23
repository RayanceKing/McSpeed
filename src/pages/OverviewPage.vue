<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import HelpTip from "../components/HelpTip.vue";
import TrafficChart from "../components/TrafficChart.vue";
import { showDialog } from "../composables/useDialog";
import { getTraffic, toggleProxy } from "../api";
import type { ProxyStatus, TrafficReport } from "../types";
import { formatBytes } from "../types";

const props = defineProps<{
  status: ProxyStatus | null;
}>();

const emit = defineEmits<{
  (e: "refresh"): void;
}>();

const traffic = ref<TrafficReport | null>(null);
const toggling = ref(false);

let timer: number | undefined;

onMounted(() => {
  void loadTraffic();
  // 每 5 秒刷新一次流量，图表看起来是连续在动的
  timer = window.setInterval(() => void loadTraffic(), 5000);
});

onUnmounted(() => {
  if (timer) window.clearInterval(timer);
});

async function loadTraffic() {
  try {
    traffic.value = await getTraffic();
  } catch {
    // 日志读取失败不该打断界面，保持上次的数据
  }
}

async function onToggle(enable: boolean) {
  if (toggling.value) return;

  // 想开启但前置条件不满足：禁用开关 + 说明缺什么
  if (enable) {
    const blocker = startupBlocker();
    if (blocker) {
      showDialog("无法启动加速", blocker);
      return;
    }
  }

  toggling.value = true;
  try {
    await toggleProxy(enable);
    emit("refresh");
    await loadTraffic();
  } catch (err) {
    showDialog("操作失败", err);
  } finally {
    toggling.value = false;
  }
}

/**
 * 检查能否启动。返回 null 表示可以，否则返回人类可读的原因。
 */
function startupBlocker(): string | null {
  const s = props.status;
  if (!s) return "组件状态尚未读取完成，请稍候再试。";
  if (!s.caddy_installed) {
    return `找不到 caddy.exe，请确认 FabricProxy\\local\\caddy\\ 目录完整。\n\n期望路径：\n${s.caddyfile_path.replace(/Caddyfile$/, "caddy\\caddy.exe")}`;
  }
  if (!s.mkcert_installed) {
    return "找不到 mkcert.exe，无法签发本地证书。请确认 FabricProxy\\local\\mkcert.exe 存在。";
  }
  return null;
}

const active = () => props.status?.active ?? false;

/** 总开关是否可点击。已在运行或前置条件不满足时禁用。 */
const canStart = () => {
  const s = props.status;
  if (!s) return false;
  return s.caddy_installed && s.mkcert_installed;
};
</script>

<template>
  <div class="page">
    <!-- 总开关 -->
    <mc-card>
      <template #title>
        <span class="card-title">
          <mc-icon name="mc-activate" :size="16" class="card-icon" />
          加速总开关
          <HelpTip text="开启后会签发本地证书、把 Fabric 域名写入 hosts 指向本机，并启动 Caddy 监听 443 接管流量。关闭时反向清理：停 Caddy 并移除 hosts 记录。整个过程需要管理员权限。" />
        </span>
      </template>
      <template #text>
        <div class="switch-row">
          <span class="switch-text">{{ active() ? "加速运行中" : "加速已停止" }}</span>
          <mc-switch
            :model-value="active()"
            :disabled="toggling || !canStart()"
            aria-label="加速总开关"
            @update:model-value="onToggle"
          />
        </div>

        <div v-if="status" class="pills">
          <span class="pill" :class="status.caddy_running ? 'ok' : 'idle'">
            Caddy {{ status.caddy_running ? "运行中" : "未运行" }}
          </span>
          <span class="pill" :class="status.certificate_ready ? 'ok' : 'idle'">
            证书 {{ status.certificate_ready ? "已签发" : "缺失" }}
          </span>
          <span class="pill" :class="status.hosts_applied ? 'ok' : 'idle'">
            hosts {{ status.hosts_applied ? "已写入" : "未写入" }}
          </span>
        </div>
      </template>
    </mc-card>

    <!-- 流量统计 -->
    <mc-card>
      <template #title>
        <span class="card-title">
          <mc-icon name="mc-download" :size="16" class="card-icon" />
          网络流量
          <HelpTip text="统计来自 Caddy 的 access log：所有经过本地代理的请求数与响应字节数。CDN 命中指 Cloudflare 边缘缓存命中，这类请求不回流到 Fabric 官方服务器，速度最快。" />
        </span>
      </template>
      <template #text>
        <div v-if="traffic" class="metrics">
          <div class="metric">
            <span class="metric-value mono">{{ formatBytes(traffic.summary.bytes) }}</span>
            <span class="metric-label"><mc-icon name="mc-download" :size="12" /> 总下载</span>
          </div>
          <div class="metric">
            <span class="metric-value mono">{{ traffic.summary.requests }}</span>
            <span class="metric-label"><mc-icon name="mc-list" :size="12" /> 请求数</span>
          </div>
          <div class="metric">
            <span class="metric-value mono">{{ traffic.summary.cf_hits }}</span>
            <span class="metric-label"><mc-icon name="mc-cloud-sync" :size="12" /> CDN 命中</span>
          </div>
          <div class="metric">
            <span class="metric-value mono">{{ traffic.summary.avg_ms }} ms</span>
            <span class="metric-label"><mc-icon name="mc-fire" :size="12" /> 平均耗时</span>
          </div>
        </div>

        <TrafficChart v-if="traffic" :buckets="traffic.buckets" />

        <p v-if="traffic && traffic.summary.failed > 0" class="warn">
          {{ traffic.summary.failed }} 个请求失败，去「诊断」页查看详情。
        </p>
      </template>
    </mc-card>

    <!-- 按域名 -->
    <mc-card v-if="traffic && traffic.per_domain.length > 0">
      <template #title>
        <span class="card-title">
          <mc-icon name="mc-world" :size="16" class="card-icon" />
          按域名
          <HelpTip text="各域名分别产生了多少流量。数字只统计本代理处理过的请求，绕过本地链路的流量不计入。" />
        </span>
      </template>
      <template #text>
        <mc-list>
          <mc-list-item
            v-for="d in traffic.per_domain"
            :key="d.domain"
            :label="d.domain"
            :subtitle="`${d.requests} 次请求`"
            :value="d.domain"
            icon="mc-world"
            :interactive="false"
          >
            <template #right>
              <span class="mono dim">{{ formatBytes(d.bytes) }}</span>
            </template>
          </mc-list-item>
        </mc-list>
      </template>
    </mc-card>
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

.switch-row {
  display: flex;
  align-items: center;
  gap: 14px;
}

/*
 * mc-card-text 带 overflow-wrap: anywhere，中文短句会被逐字折断。
 * 这里锁死不换行，并让文字按内容宽度占位，把剩下的空间留给开关。
 */
.switch-text {
  font-size: 13px;
  opacity: 0.8;
  white-space: nowrap;
  flex: 0 0 auto;
}

.pills {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 14px;
}

.pill {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 3px;
  background: rgba(255, 255, 255, 0.08);
  color: rgba(255, 255, 255, 0.6);
}

.pill.ok {
  background: rgba(60, 133, 39, 0.18);
  color: #7fd06a;
}

.pill.idle {
  background: rgba(255, 255, 255, 0.06);
}

.metrics {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 8px;
  margin-bottom: 16px;
}

.metric {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.metric-value {
  font-size: 17px;
  font-weight: 600;
}

.metric-label {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  opacity: 0.6;
}

.mono {
  font-family: Consolas, "Cascadia Mono", monospace;
}

.dim {
  opacity: 0.65;
  font-size: 13px;
}

.warn {
  font-size: 12px;
  color: #e8837a;
  margin: 12px 0 0;
}
</style>
