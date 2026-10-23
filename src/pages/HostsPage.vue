<script setup lang="ts">
import { ref } from "vue";
import HelpTip from "../components/HelpTip.vue";
import { showDialog } from "../composables/useDialog";
import { applyHosts, openHostsFolder, removeHosts } from "../api";
import type { ProxyStatus } from "../types";

const props = defineProps<{
  status: ProxyStatus | null;
}>();

const emit = defineEmits<{
  (e: "refresh"): void;
}>();

const busy = ref(false);

async function onApply() {
  if (busy.value) return;
  busy.value = true;
  try {
    const result = await applyHosts();
    emit("refresh");
    const detail = result.backup_path
      ? `共 ${result.added.length} 条。原文件已备份到：\n${result.backup_path}`
      : `共 ${result.added.length} 条。`;
    showDialog("hosts 已写入", detail, "知道了");
  } catch (err) {
    showDialog("写入 hosts 失败", err);
  } finally {
    busy.value = false;
  }
}

async function onRemove() {
  if (busy.value) return;
  busy.value = true;
  try {
    const result = await removeHosts();
    emit("refresh");
    showDialog(
      result.removed.length === 0 ? "没有记录" : "hosts 记录已删除",
      result.removed.length === 0
        ? "hosts 中没有 McSpeed 管理的条目。"
        : `已删除 ${result.removed.length} 条记录。原文件已备份到：\n${result.backup_path ?? "（无备份）"}`,
      "知道了"
    );
  } catch (err) {
    showDialog("删除 hosts 记录失败", err);
  } finally {
    busy.value = false;
  }
}

async function onOpenFolder() {
  try {
    await openHostsFolder();
  } catch (err) {
    showDialog("打开目录失败", err);
  }
}
</script>

<template>
  <div class="page">
    <mc-card>
      <template #title>
        <span class="card-title">
          <mc-icon name="mc-clipboard" :size="16" class="card-icon" />
          hosts 管理
          <HelpTip text="只操作带 # McSpeed managed 标记的行块，你自己的 hosts 内容不会被碰。每次写入前自动备份到 hosts.mcspeed.bak，出问题把备份内容粘回去即可。" />
        </span>
      </template>
      <template #text>
        <div class="actions">
          <mc-button variant="primary" icon="mc-save" :loading="busy" @click="onApply">写入 hosts</mc-button>
          <mc-button icon="mc-trash-can" :loading="busy" @click="onRemove">删除全部记录</mc-button>
          <mc-button icon="mc-folder-open" :disabled="busy" @click="onOpenFolder">打开目录</mc-button>
        </div>
      </template>
    </mc-card>

    <mc-card>
      <template #title>
        <span class="card-title">
          <mc-icon name="mc-list" :size="16" class="card-icon" />
          当前记录（{{ status?.managed_entries.length ?? 0 }}）
          <HelpTip text="hosts 里当前由 McSpeed 管理的条目。每个启用的域名占两条：一条 IPv4、一条 IPv6，都指向 127.0.0.1 或 ::1。" />
        </span>
      </template>
      <template #text>
        <p v-if="(status?.managed_entries.length ?? 0) === 0" class="hint">
          hosts 中没有 McSpeed 管理的条目。
        </p>
        <mc-list v-else>
          <mc-list-item
            v-for="e in status?.managed_entries ?? []"
            :key="e.domain + e.ip"
            :label="e.domain"
            :subtitle="e.ip"
            :value="e.domain + e.ip"
            icon="mc-world"
            :interactive="false"
          />
        </mc-list>
      </template>
    </mc-card>

    <mc-card>
      <template #title>
        <span class="card-title">
          <mc-icon name="mc-folder-open" :size="16" class="card-icon" />
          文件位置
        </span>
      </template>
      <template #text>
        <p class="path mono">{{ status?.hosts_path ?? "-" }}</p>
      </template>
    </mc-card>
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
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

.card-title {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.card-icon {
  opacity: 0.7;
}

.path {
  font-size: 12px;
  opacity: 0.8;
  word-break: break-all;
  margin: 0;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 12px;
}
</style>
