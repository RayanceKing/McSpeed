<script setup lang="ts">
/**
 * 加速规则页：用户自定义「劫持哪些域名、命中什么路径、命中后做什么」。
 *
 * 每条规则是一个独立的三元组（域名 + 路径 + 上游），同一域名同路径的多条
 * 规则自动归为一组，组内测速最快的一条生效。测速是真实下载上游文件计时，
 * 测完「测速」按钮直接变成速度，组内按用时升序排好，结果与顺序都落盘。
 *
 * 页面还带搜索过滤，以及完整配置的导入（文件 / 剪贴板 / 链接）
 * 与导出（文件 / 剪贴板）。
 */
import { computed, ref, watch } from "vue";
import { open as openFile, save as saveFile } from "@tauri-apps/plugin-dialog";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import HelpTip from "../components/HelpTip.vue";
import { showDialog } from "../composables/useDialog";
import {
  addRule,
  deleteRule,
  exportConfigText,
  fetchConfigText,
  importConfig,
  parseConfig,
  probeRules,
  readConfigFile,
  setRules,
  updateRule,
  writeConfigFile,
} from "../api";
import type { ProxyRule, ProxyStatus, RuleGroupItem } from "../types";
import {
  emptyRule,
  formatBytes,
  formatSpeed,
  groupRules,
  ruleSummary,
} from "../types";

const props = defineProps<{
  status: ProxyStatus | null;
}>();

const emit = defineEmits<{
  (e: "refresh"): void;
}>();

const rules = computed<ProxyRule[]>(() => rulesOverride.value ?? props.status?.rules ?? []);
/** 按同域名+同路径归组后的展示列表。 */
const grouped = computed<RuleGroupItem[]>(() => groupRules(rules.value));
/** 本地规则副本。拖拽等操作后立刻用它显示新顺序，不等后端返回。 */
const rulesOverride = ref<ProxyRule[] | null>(null);
/** 副本生成时刻。父组件刷新只在副本过期后才让位，避免旧数据冲掉刚落位的顺序。 */
const overrideAt = ref(0);

watch(
  () => props.status,
  () => {
    // 1.5 秒内的副本一律保留：这个窗口足够 setRules 返回并更新副本，
    // 期间父组件拿到的 status 仍是旧顺序，不能用它覆盖
    if (rulesOverride.value && Date.now() - overrideAt.value < 1500) return;
    rulesOverride.value = null;
  }
);

// ── 搜索过滤 ──────────────────────────────────────────────────────────
/** 搜索关键字。命中名称、域名、上游、路径、分组任意一项就保留该组。 */
const search = ref("");
const searchQuery = computed(() => search.value.trim().toLowerCase());
const filtering = computed(() => searchQuery.value !== "");
const filteredGroups = computed<RuleGroupItem[]>(() => {
  const q = searchQuery.value;
  if (q === "") return grouped.value;
  return grouped.value.filter((item) =>
    item.rules.some(
      (r) =>
        r.name.toLowerCase().includes(q) ||
        r.domains.some((d) => d.toLowerCase().includes(q)) ||
        r.upstream.toLowerCase().includes(q) ||
        r.route_prefix.toLowerCase().includes(q) ||
        r.speed_group.toLowerCase().includes(q) ||
        r.path.toLowerCase().includes(q)
    )
  );
});
/** 当前显示出来的规则条数（按整组显示）。 */
const visibleCount = computed(() =>
  filteredGroups.value.reduce((n, g) => n + g.rules.length, 0)
);

// ── 拖拽排序 ─────────────────────────────────────────────────────────
/** 拖拽排序：被拖动的规则下标。 */
const dragIndex = ref<number | null>(null);
/** 拖拽排序：插入位置（0 ~ rules.length）。 */
const dropIndex = ref<number | null>(null);
/** 鼠标相对拖拽起点的 Y 偏移，被拖卡片跟手。 */
const dragOffsetY = ref(0);
/** 被拖卡片高度（含间距），其余卡片按这个距离让位。 */
const dragHeight = ref(0);
/** 拖拽开始时各卡片的原始 top 坐标。拖动过程中卡片被 transform 移位，
 *  getBoundingClientRect 会跟着变，导致 dropIndex 算不准——
 *  所以固定用这份原始坐标来判断插入位置。 */
let originalTops: number[] = [];
let originalBottoms: number[] = [];

function swallowNextClick() {
  const handler = (ev: Event) => {
    ev.preventDefault();
    ev.stopPropagation();
    ev.stopImmediatePropagation();
  };
  document.addEventListener("click", handler, { capture: true, once: true });
  window.setTimeout(() => document.removeEventListener("click", handler, true), 0);
}

function onHandleDown(index: number, event: PointerEvent) {
  // 搜索过滤时不拖拽：过滤后的下标对不上完整列表，落点会错位
  if (filtering.value) return;

  // 点在按钮、开关等可交互元素上时不触发拖拽。
  // 用 mc-button 而不是 button：McExpansionPanel 的标题也是 button，
  // 用 button 会把整个分组的拖拽都挡住
  const target = event.target as HTMLElement;
  if (target.closest("mc-button, input, textarea, select, a")) return;

  event.preventDefault();
  const startX = event.clientX;
  const startY = event.clientY;
  let moved = false;
  dragIndex.value = index;
  dropIndex.value = index;
  dragOffsetY.value = 0;

  // 拖拽开始时一次性记录所有卡片的原始位置。
  // 用 top + bottom 算真实中点，不能用统一的 dragHeight——
  // 组卡片比单条高，统一高度会让末尾判定区跑到卡片外面去
  const items = document.querySelectorAll<HTMLElement>(".rule-item");
  originalTops = [];
  originalBottoms = [];
  items.forEach((el) => {
    const rect = el.getBoundingClientRect();
    originalTops.push(rect.top);
    originalBottoms.push(rect.bottom);
  });
  if (originalTops.length >= 2) {
    dragHeight.value = originalTops[1] - originalTops[0];
  } else if (items.length > 0) {
    dragHeight.value = items[0].getBoundingClientRect().height;
  }

  const onMove = (ev: PointerEvent) => {
    if (Math.abs(ev.clientX - startX) > 4 || Math.abs(ev.clientY - startY) > 4) moved = true;
    dragOffsetY.value = ev.clientY - startY;

    const from = dragIndex.value;
    const lastBottom = originalBottoms[originalBottoms.length - 1];
    if (lastBottom !== undefined && ev.clientY >= lastBottom) {
      dropIndex.value = originalTops.length;
      return;
    }

    // 方向感知的插入位置判定：
    // - 往下拖（from < i）：鼠标越过目标的顶边 → 插到它前面（即和它交换）
    // - 往上拖（from > i）：鼠标越过目标的底边 → 插到它前面（即和它交换）
    // 不这样做的话，向上拖到目标中间会被算成「原地不动」
    for (let i = 0; i < originalTops.length; i++) {
      if (from != null && from < i) {
        if (ev.clientY < originalTops[i]) {
          dropIndex.value = i;
          return;
        }
      } else if (from != null && from > i) {
        if (ev.clientY < originalBottoms[i]) {
          dropIndex.value = i;
          return;
        }
      }
    }
    dropIndex.value = originalTops.length;
  };

  const onUp = () => {
    document.removeEventListener("pointermove", onMove);
    document.removeEventListener("pointerup", onUp);
    if (moved) swallowNextClick();

    const from = dragIndex.value;
    const to = dropIndex.value;
    dragIndex.value = null;
    dropIndex.value = null;
    dragOffsetY.value = 0;

    if (from == null || to == null) return;

    const target = from < to ? to - 1 : to;
    if (target === from) return;

    const items = [...grouped.value];
    const [movedItem] = items.splice(from, 1);
    items.splice(target, 0, movedItem);
    const list = items.flatMap((item) => [...item.rules]);
    // 先本地更新顺序，卡片立刻就位；后台再持久化
    rulesOverride.value = list; overrideAt.value = Date.now();
    void persistOrder(list);
  };

  document.addEventListener("pointermove", onMove);
  document.addEventListener("pointerup", onUp);
}

/**
 * 拖拽排序：按住任意位置拖动调整优先级。
 *
 * 被拖的卡片跟手（translateY = 鼠标偏移）；
 * 原位置和目标位置之间的卡片整体上/下让位一个卡片距离，
 * 空出来的槽位就是松手后的落点。
 *
 * 注意：这里必须用最终落点 index（与 onUp 提交的 target 一致），
 * 不能用 dropIndex——dropIndex 是插入位置，下移时要 -1。
 * 两者对不上时视觉槽位和实际落点会错开一格，看起来像「回弹」。
 */
function cardShift(index: number): Record<string, string> {
  const from = dragIndex.value;
  const drop = dropIndex.value;
  if (from == null || drop == null) return {};

  if (index === from) {
    return { transform: `translateY(${dragOffsetY.value}px)`, zIndex: "10" };
  }

  const h = dragHeight.value;
  if (h === 0) return {};

  // 换算成最终落点，与 onUp 提交时用的是同一个公式
  const to = from < drop ? drop - 1 : drop;

  // 往上拖（to < from）：经过的卡片下移让位
  // 往下拖（to > from）：经过的卡片上移让位
  if (to < from && index >= to && index < from) {
    return { transform: `translateY(${h}px)` };
  }
  if (to > from && index > from && index <= to) {
    return { transform: `translateY(-${h}px)` };
  }
  return {};
}

/** Minecraft 风格像素图标（inline SVG，shape-rendering=crispEdges 确保硬边）。 */
const ICONS: Record<string, string> = {
  drag: `<svg width="10" height="12" viewBox="0 0 10 12" shape-rendering="crispEdges"><rect x="1" y="1" width="2" height="2" fill="currentColor"/><rect x="7" y="1" width="2" height="2" fill="currentColor"/><rect x="1" y="5" width="2" height="2" fill="currentColor"/><rect x="7" y="5" width="2" height="2" fill="currentColor"/><rect x="1" y="9" width="2" height="2" fill="currentColor"/><rect x="7" y="9" width="2" height="2" fill="currentColor"/></svg>`,
};

/** 把调整后的顺序提交给后端。排序不改域名，后端跳过 hosts 写入。 */
async function persistOrder(list: ProxyRule[]) {
  try {
    const status = await setRules(list);
    rulesOverride.value = status.rules; overrideAt.value = Date.now();
  } catch (err) {
    showDialog("排序失败", err);
    emit("refresh");
  }
}

/** 折叠的分组 id 集合。 */
const collapsed = ref<Set<string>>(new Set());
function setCollapsed(key: string, value: boolean) {
  const next = new Set(collapsed.value);
  if (value) next.add(key);
  else next.delete(key);
  collapsed.value = next;
}
function groupKeyOf(item: RuleGroupItem): string {
  return item.ids.join("|");
}
function isCollapsed(item: RuleGroupItem): boolean {
  return collapsed.value.has(groupKeyOf(item));
}

// ── 测速 ─────────────────────────────────────────────────────────────
/** 测速中：规则 id -> true。用于给按钮加 loading 态，同时当全局互斥。 */
const probingIds = ref<Set<string>>(new Set());
function isProbing(id: string): boolean {
  return probingIds.value.has(id);
}
function isProbingGroup(item: RuleGroupItem): boolean {
  return item.rules.some((r) => isProbing(r.id));
}
const probingAny = computed(() => probingIds.value.size > 0);

/**
 * 测速：不传 ids 测全部，传 ids 测单条或整组。
 *
 * 测完后端会把每个组内按下载用时升序重排（最快在前），并把
 * 结果与顺序一起落盘；返回的 status.rules 就是排好的新顺序。
 */
async function runProbe(ids?: string[]) {
  if (probingAny.value) return;
  const scoped = ids && ids.length > 0;
  const targets = scoped ? ids : rules.value.map((r) => r.id);
  if (targets.length === 0) return;

  probingIds.value = new Set(targets);
  try {
    const status = await probeRules(scoped ? ids : undefined);
    rulesOverride.value = status.rules; overrideAt.value = Date.now();
  } catch (err) {
    showDialog("测速失败", err);
    emit("refresh");
  } finally {
    probingIds.value = new Set();
  }
}

/** 给一组规则测速（组内每条都测，测完自动按用时排好）。 */
function onProbeGroup(item: RuleGroupItem) {
  void runProbe(item.rules.map((r) => r.id));
}

/** 测速按钮上的文字：测完直接是速度，没测过才是「测速」。 */
function probeLabel(rule: ProxyRule): string {
  return formatSpeed(rule) ?? "测速";
}

/** 悬停提示：展示速度背后的下载用时与体积。 */
function probeTitle(rule: ProxyRule): string {
  if (rule.probe_ms == null || rule.probe_bytes <= 0) {
    return "点击测速：下载上游文件计时，用时越小越快";
  }
  return `下载用时 ${rule.probe_ms} ms · 文件 ${formatBytes(rule.probe_bytes)}`;
}

/** 组内候选的转发目标，用来区分同组的几条规则。 */
function routeTarget(rule: ProxyRule): string {
  const prefix = rule.route_prefix.trim();
  return prefix === "" ? rule.upstream.trim() : `${rule.upstream.trim()}${prefix}`;
}

// ── 规则编辑 ─────────────────────────────────────────────────────────
/** 正在编辑的规则草稿；null 表示对话框关闭。 */
const draft = ref<ProxyRule | null>(null);
/** 是否正在新增（影响标题与提交按钮文字）。 */
const creating = ref(false);
/** 域名输入框的临时文本，提交时按行拆分。 */
const domainText = ref("");
const busy = ref(false);
const formError = ref("");

function openCreate() {
  const rule = emptyRule();
  // 新规则默认复用当前上游，省得用户每次手打一遍
  rule.upstream = props.status?.upstream ?? "";
  draft.value = rule;
  domainText.value = "";
  formError.value = "";
  creating.value = true;
}

function openEdit(rule: ProxyRule) {
  draft.value = { ...rule, domains: [...rule.domains] };
  domainText.value = rule.domains.join("\n");
  formError.value = "";
  creating.value = false;
}

function closeDialog() {
  draft.value = null;
  formError.value = "";
}

/** 把域名输入框拆成数组：一行一个，去空行去首尾空格。 */
function parseDomains(): string[] {
  return domainText.value
    .split(/[\n,]/)
    .map((d) => d.trim())
    .filter((d) => d.length > 0);
}

/** 提交前做一次前端校验，错误直接显示在表单里。 */
function validateDraft(): string {
  const rule = draft.value;
  if (!rule) return "表单已关闭。";

  if (rule.name.trim() === "") return "请填写规则名称。";

  const domains = parseDomains();
  if (domains.length === 0) return "请至少填写一个域名。";

  for (const domain of domains) {
    const lower = domain.toLowerCase();
    const body = lower.startsWith("*.") ? lower.slice(2) : lower;
    if (!/^[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(body) || body.includes("..")) {
      return `域名「${domain}」不合法：只能包含小写字母、数字、连字符和点，且不能带协议或端口。`;
    }
  }

  const path = rule.path.trim();
  if (path !== "" && !path.startsWith("/")) return "路径前缀必须以 / 开头。";

  const upstream = rule.upstream.trim();
  if (!upstream.startsWith("http://") && !upstream.startsWith("https://")) {
    return "转发目标必须是 http:// 或 https:// 开头的地址。";
  }
  const prefix = rule.route_prefix.trim();
  if (prefix !== "" && !prefix.startsWith("/")) {
    return "路径前缀必须以 / 开头，例如 /fabric-maven。";
  }

  const group = rule.speed_group.trim();
  if (group !== "" && !/^[A-Za-z0-9._-]+$/.test(group)) {
    return "测速分组名只能包含字母、数字、点、连字符和下划线。";
  }

  return "";
}

async function submit() {
  if (!draft.value || busy.value) return;

  const error = validateDraft();
  if (error) {
    formError.value = error;
    return;
  }

  busy.value = true;
  try {
    const payload: ProxyRule = {
      ...draft.value,
      name: draft.value.name.trim(),
      domains: parseDomains(),
      path: draft.value.path.trim(),
      upstream: draft.value.upstream.trim(),
      route_prefix: draft.value.route_prefix.trim(),
      speed_group: draft.value.speed_group.trim(),
    };

    const status = creating.value
      ? await addRule(payload)
      : await updateRule(payload);
    if (creating.value) {
      showDialog("规则已添加", `「${payload.name}」已保存并生效。`, "知道了");
    } else {
      showDialog("规则已更新", `「${payload.name}」已保存并生效。`, "知道了");
    }
    closeDialog();
    rulesOverride.value = status.rules; overrideAt.value = Date.now();
  } catch (err) {
    // 后端校验（域名重复、hosts 写入失败等）原样展示
    formError.value = String(err);
  } finally {
    busy.value = false;
  }
}

/** 删除前二次确认：规则对应的 hosts 记录会被一并清掉。 */
const confirmingDelete = ref<ProxyRule | null>(null);

function askDelete(rule: ProxyRule) {
  confirmingDelete.value = rule;
}

async function onDeleteConfirmed() {
  const rule = confirmingDelete.value;
  confirmingDelete.value = null;
  if (!rule || busy.value) return;

  busy.value = true;
  try {
    const status = await deleteRule(rule.id);
    showDialog("规则已删除", `「${rule.name}」已移除。`, "知道了");
    rulesOverride.value = status.rules; overrideAt.value = Date.now();
  } catch (err) {
    showDialog("删除失败", err);
  } finally {
    busy.value = false;
  }
}

// ── 导入 / 导出 ──────────────────────────────────────────────────────
/** 当前打开的导入导出对话框；null 表示关闭。 */
const ioTab = ref<"import" | "export" | null>(null);
/** 从链接导入时的地址输入。 */
const importUrl = ref("");
/** 对话框内的错误提示。 */
const ioError = ref("");
/** 文件 / 网络 / 剪贴板操作进行中。 */
const ioBusy = ref(false);
/** 解析成功、等待用户确认导入的规则；null 表示还停留在选择来源那一步。 */
const pendingImport = ref<ProxyRule[] | null>(null);
/** 解析时的提示（id 重复被重生成之类），不影响导入。 */
const importWarnings = ref<string[]>([]);

function openIo(mode: "import" | "export") {
  closeIo();
  ioTab.value = mode;
}

function closeIo() {
  ioTab.value = null;
  pendingImport.value = null;
  importWarnings.value = [];
  ioError.value = "";
  importUrl.value = "";
  ioBusy.value = false;
}

/** 三个来源都汇到这：解析成功就切到确认页。 */
async function parseSource(text: string) {
  const preview = await parseConfig(text);
  pendingImport.value = preview.rules;
  importWarnings.value = preview.warnings;
  ioError.value = "";
}

async function importFromFile() {
  if (ioBusy.value) return;
  const picked = await openFile({
    multiple: false,
    filters: [{ name: "JSON 配置", extensions: ["json", "txt"] }],
  });
  if (!picked) return;
  const path = Array.isArray(picked) ? picked[0] : picked;

  ioBusy.value = true;
  try {
    await parseSource(await readConfigFile(path));
  } catch (err) {
    ioError.value = String(err);
  } finally {
    ioBusy.value = false;
  }
}

async function importFromClipboard() {
  if (ioBusy.value) return;
  ioBusy.value = true;
  try {
    await parseSource(await readText());
  } catch (err) {
    ioError.value = `读取剪贴板失败：${String(err)}`;
  } finally {
    ioBusy.value = false;
  }
}

async function importFromUrl() {
  if (ioBusy.value) return;
  const url = importUrl.value.trim();
  if (url === "") {
    ioError.value = "请先填写链接。";
    return;
  }
  ioBusy.value = true;
  try {
    await parseSource(await fetchConfigText(url));
  } catch (err) {
    ioError.value = String(err);
  } finally {
    ioBusy.value = false;
  }
}

/** 确认导入：整体替换当前规则（含顺序与测速结果）。 */
async function confirmImport() {
  const list = pendingImport.value;
  if (!list || ioBusy.value) return;

  ioBusy.value = true;
  try {
    const status = await importConfig(list);
    rulesOverride.value = status.rules; overrideAt.value = Date.now();
    closeIo();
    showDialog("导入成功", `已导入 ${list.length} 条规则并生效。`, "知道了");
  } catch (err) {
    showDialog("导入失败", err);
    ioBusy.value = false;
  }
}

async function exportToFile() {
  if (ioBusy.value) return;
  const picked = await saveFile({
    defaultPath: "mcspeed-rules.json",
    filters: [{ name: "JSON 配置", extensions: ["json"] }],
  });
  if (!picked) return;
  const path = Array.isArray(picked) ? picked[0] : picked;

  ioBusy.value = true;
  try {
    await writeConfigFile(path, await exportConfigText());
    closeIo();
    showDialog("导出成功", `完整配置已写入：${path}`, "知道了");
  } catch (err) {
    ioError.value = String(err);
    ioBusy.value = false;
  }
}

async function exportToClipboard() {
  if (ioBusy.value) return;
  ioBusy.value = true;
  try {
    await writeText(await exportConfigText());
    closeIo();
    showDialog("已复制", "完整配置已复制到剪贴板。", "知道了");
  } catch (err) {
    ioError.value = `复制到剪贴板失败：${String(err)}`;
    ioBusy.value = false;
  }
}
</script>
<template>
  <div class="page">
    <mc-card>
      <template #title>
        <span class="card-title">
          <mc-icon name="mc-list" :size="16" class="card-icon" />
          加速规则
          <HelpTip text="同一域名同路径的多条规则自动归为一组，组内下载用时最小的一条生效。测速后按钮直接显示速度，结果与排序都会保存。拖拽卡片可调整优先级。" />
        </span>
      </template>
      <template #text>
        <div class="toolbar">
          <span class="counts">
            <template v-if="filtering">
              匹配 {{ visibleCount }} / {{ rules.length }} 条 ·
              {{ filteredGroups.length }} / {{ grouped.length }} 组
            </template>
            <template v-else>
              {{ rules.length }} 条规则 · {{ grouped.filter((g) => g.isGroup).length }} 组
            </template>
          </span>
          <div class="toolbar-actions">
            <mc-button icon="mc-download" @click="openIo('import')">导入</mc-button>
            <mc-button icon="mc-export" @click="openIo('export')">导出</mc-button>
            <mc-button :loading="probingAny" icon="mc-fire" @click="runProbe()">一键测速</mc-button>
            <mc-button variant="primary" icon="mc-add" @click="openCreate">新增规则</mc-button>
          </div>
        </div>
        <div class="search-row">
          <mc-text-field
            v-model="search"
            label="搜索过滤"
            placeholder="按名称、域名、上游、路径、分组过滤…"
            hint="只影响显示，不改动已保存的顺序"
          />
        </div>
      </template>
    </mc-card>

    <mc-card v-if="rules.length === 0">
      <template #text>
        <p class="hint">还没有任何规则。点「新增规则」开始配置。</p>
      </template>
    </mc-card>

    <mc-card v-else-if="filteredGroups.length === 0">
      <template #text>
        <p class="hint">没有匹配「{{ search }}」的规则。</p>
      </template>
    </mc-card>

    <!-- 每条规则 / 每组 -->
    <div
      v-for="(item, gi) in filteredGroups"
      :key="groupKeyOf(item)"
      class="rule-item"
      :class="{ dragging: dragIndex === gi }"
      :style="cardShift(gi)"
      @pointerdown="onHandleDown(gi, $event)"
    >
      <!-- 单条规则 -->
      <mc-card v-if="!item.isGroup">
        <template #title>
          <span class="title-row">
            <span class="drag-handle" aria-hidden="true" v-html="ICONS.drag" />
            {{ item.rules[0].name }}
            <mc-chip v-if="item.rules[0].speed_group.trim()" size="small">
              {{ item.rules[0].speed_group.trim() }}
            </mc-chip>
          </span>
        </template>
        <template #text>
          <p class="mono-text">{{ ruleSummary(item.rules[0]) }}</p>
          <div class="chip-row">
            <mc-chip v-for="d in item.rules[0].domains" :key="d" size="small">{{ d }}</mc-chip>
          </div>
        </template>
        <template #actions>
          <mc-button
            size="small"
            icon="mc-fire"
            :loading="isProbing(item.rules[0].id)"
            :title="probeTitle(item.rules[0])"
            @click="runProbe([item.rules[0].id])"
          >{{ probeLabel(item.rules[0]) }}</mc-button>
          <mc-button size="small" @click="openEdit(item.rules[0])" icon="mc-edit">编辑</mc-button>
          <mc-button size="small" variant="error" @click="askDelete(item.rules[0])" icon="mc-trash-can">删除</mc-button>
        </template>
      </mc-card>

      <!-- 分组 -->
      <mc-expansion-panels v-else :model-value="isCollapsed(item) ? null : groupKeyOf(item)" @update:model-value="(v: any) => setCollapsed(groupKeyOf(item), v === null)">
        <mc-expansion-panel :value="groupKeyOf(item)">
          <template #title>
            <span class="title-row title-row--panel">
              <span class="drag-handle" aria-hidden="true" v-html="ICONS.drag" />
              {{ item.title }}
              <mc-chip size="small">{{ item.rules.length }} 个候选</mc-chip>
              <mc-icon
                class="expand-icon"
                :class="{ 'expand-icon--open': !isCollapsed(item) }"
                name="mc-chevron-right"
                :size="18"
                aria-hidden="true"
              />
            </span>
          </template>
          <mc-list>
            <mc-list-item
              label="整组测速"
              subtitle="按下载用时排序，最小的一条排最前"
              :value="'probe-' + groupKeyOf(item)"
            >
              <template #right>
                <mc-button
                  size="small"
                  icon="mc-fire"
                  :loading="isProbingGroup(item)"
                  @click="onProbeGroup(item)"
                >测速</mc-button>
              </template>
            </mc-list-item>
            <mc-list-item
              v-for="r in item.rules"
              :key="r.id"
              :label="r.name"
              :value="r.id"
              :subtitle="routeTarget(r)"
            >
              <template #right>
                <mc-button
                  size="small"
                  icon="mc-fire"
                  :loading="isProbing(r.id)"
                  :title="probeTitle(r)"
                  @click="runProbe([r.id])"
                >{{ probeLabel(r) }}</mc-button>
                <mc-button size="small" @click="openEdit(r)" icon="mc-edit">编辑</mc-button>
                <mc-button size="small" variant="error" @click="askDelete(r)" icon="mc-trash-can">删除</mc-button>
              </template>
            </mc-list-item>
          </mc-list>
        </mc-expansion-panel>
      </mc-expansion-panels>
    </div>

    <!-- 规则编辑对话框 -->
    <mc-dialog
      :model-value="draft !== null"
      :title="creating ? '新增规则' : '编辑规则'"
      :width="520"
      @update:model-value="(v: boolean) => { if (!v) closeDialog(); }"
    >
      <div v-if="draft" class="form">
        <mc-text-field v-model="draft.name" label="规则名称" placeholder="例如：Fabric Maven" :required="true" />
        <mc-textarea v-model="domainText" label="域名（每行一个）" placeholder="maven.fabricmc.net" hint="支持 *.example.com 通配" :required="true" :rows="2" />
        <mc-text-field v-model="draft.path" label="路径前缀（可留空）" placeholder="留空表示全部路径" hint="只命中该前缀下的请求" />
        <mc-divider />
        <mc-text-field v-model="draft.upstream" label="转发目标" placeholder="https://mcproxy.unmod.online" hint="请求最终发到的上游地址" :required="true" />
        <mc-text-field v-model="draft.route_prefix" label="路径前缀重写（可留空）" placeholder="/fabric-maven" hint="转发前补到路径前面" />
        <mc-text-field v-model="draft.speed_group" label="测速分组（可留空）" placeholder="例如：maven" hint="同组规则自动竞速，用时最小的生效" />
        <p v-if="formError" class="error-text">{{ formError }}</p>
      </div>
      <template #actions>
        <mc-button :disabled="busy" @click="closeDialog">取消</mc-button>
        <mc-button variant="primary" :loading="busy" :icon="creating ? 'mc-add' : 'mc-save'" @click="submit">
          {{ creating ? "添加" : "保存" }}
        </mc-button>
      </template>
    </mc-dialog>

    <!-- 导入 / 导出 -->
    <mc-dialog
      :model-value="ioTab !== null"
      :title="ioTab === 'import' ? '导入配置' : '导出配置'"
      :width="520"
      @update:model-value="(v: boolean) => { if (!v) closeIo(); }"
    >
      <!-- 导入第 1 步：选来源 -->
      <div v-if="ioTab === 'import' && pendingImport === null" class="form">
        <p class="hint">
          导入完整配置（规则、顺序、测速结果一并生效），将整体替换当前规则。
          支持 MC Speed 导出的 JSON，也兼容只含 rules 列表的配置。
        </p>
        <mc-button icon="mc-folder-open" :disabled="ioBusy" :loading="ioBusy" @click="importFromFile">从文件导入…</mc-button>
        <mc-button icon="mc-clipboard" :disabled="ioBusy" @click="importFromClipboard">从剪贴板导入</mc-button>
        <mc-divider />
        <mc-text-field
          v-model="importUrl"
          label="从链接导入"
          placeholder="https://example.com/mcspeed-rules.json"
          hint="粘贴配置文件的下载地址"
          :disabled="ioBusy"
        />
        <mc-button icon="mc-cloud-sync" :disabled="ioBusy" @click="importFromUrl">下载并解析</mc-button>
        <p v-if="ioError" class="error-text">{{ ioError }}</p>
      </div>

      <!-- 导入第 2 步：确认替换 -->
      <div v-else-if="ioTab === 'import'" class="form">
        <p>
          即将导入 <b>{{ pendingImport?.length }}</b> 条规则，
          替换当前 <b>{{ rules.length }}</b> 条规则。
          导入内容里的列表顺序与测速结果会一并写入。
        </p>
        <p v-for="w in importWarnings" :key="w" class="hint">{{ w }}</p>
        <p v-if="ioError" class="error-text">{{ ioError }}</p>
      </div>

      <!-- 导出 -->
      <div v-else-if="ioTab === 'export'" class="form">
        <p class="hint">
          导出完整配置：{{ rules.length }} 条规则（含列表顺序与测速结果）。
        </p>
        <mc-button icon="mc-save" :disabled="ioBusy" :loading="ioBusy" @click="exportToFile">导出到文件…</mc-button>
        <mc-button icon="mc-clipboard" :disabled="ioBusy" @click="exportToClipboard">导出到剪贴板</mc-button>
        <p v-if="ioError" class="error-text">{{ ioError }}</p>
      </div>

      <template #actions>
        <mc-button :disabled="ioBusy" @click="closeIo">
          {{ ioTab === "import" && pendingImport !== null ? "取消" : "关闭" }}
        </mc-button>
        <mc-button
          v-if="ioTab === 'import' && pendingImport !== null"
          :disabled="ioBusy"
          @click="pendingImport = null"
        >返回</mc-button>
        <mc-button
          v-if="ioTab === 'import' && pendingImport !== null"
          variant="primary"
          :icon="'mc-download'"
          :loading="ioBusy"
          @click="confirmImport"
        >确认导入</mc-button>
      </template>
    </mc-dialog>

    <!-- 删除确认 -->
    <mc-confirm
      :model-value="confirmingDelete !== null"
      title="删除规则"
      confirm-text="删除"
      cancel-text="取消"
      danger
      @update:model-value="(v: boolean) => { if (!v) confirmingDelete = null; }"
      @confirm="onDeleteConfirmed"
    >
      将删除「{{ confirmingDelete?.name }}」。如果此前接管过 hosts，对应记录会一并移除。
    </mc-confirm>
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.card-title {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.card-icon {
  opacity: 0.7;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-wrap: wrap;
}

.counts {
  font-size: 12px;
  opacity: 0.7;
}

.toolbar-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.search-row {
  margin-top: 10px;
}

.title-row {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.title-row--panel {
  flex: 1;
  width: 100%;
}

.expand-icon {
  margin-left: auto;
  flex-shrink: 0;
  transition: transform 150ms;
  transform: rotate(90deg);
}

.expand-icon--open {
  transform: rotate(-90deg);
}

:deep(.mc-expansion-panel__icon) {
  display: none;
}

.rule-item {
  transition: transform 180ms cubic-bezier(0.2, 0, 0, 1);
}

.rule-item.dragging {
  transition: none;
  opacity: 0.5;
  z-index: 10;
}

.drag-handle {
  display: inline-flex;
  align-items: center;
  cursor: grab;
  user-select: none;
  touch-action: none;
  color: rgba(255, 255, 255, 0.35);
}

.drag-handle:hover {
  color: rgba(255, 255, 255, 0.7);
}

.mono-text {
  font-family: Consolas, "Cascadia Mono", monospace;
  font-size: 12px;
  opacity: 0.8;
  margin: 0 0 8px;
  word-break: break-all;
}

.chip-row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}

.form {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.error-text {
  font-size: 12px;
  color: #e8837a;
  margin: 0;
  white-space: pre-wrap;
}

.hint {
  opacity: 0.6;
  font-size: 12px;
}
</style>
