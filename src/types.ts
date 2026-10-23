/** 一条用户自定义的加速规则。字段与 Rust 端 config::ProxyRule 一致。 */
export interface ProxyRule {
  id: string;
  name: string;
  /** 作用的域名列表，支持 `*.example.com` 通配（通配域名不写 hosts）。 */
  domains: string[];
  /** 命中的路径前缀。空串表示全部路径。 */
  path: string;
  /** 上游 origin。 */
  upstream: string;
  /** 转发前补到路径前的前缀。 */
  route_prefix: string;
  /** 测速分组名。同名的规则互相竞争，下载用时最小的一条生效；空串不参与。 */
  speed_group: string;
  /** 最近一次测速的下载用时（毫秒）；null 表示没测过或上次失败。 */
  probe_ms: number | null;
  /** 最近一次测速下载的字节数，与用时一起换算成速度展示。 */
  probe_bytes: number;
  /** 测速时间，Unix 秒；0 表示从未测速成功。 */
  probe_at: number;
}

export interface HostsEntry {
  ip: string;
  domain: string;
  managed: boolean;
  comment: string | null;
}

export interface ProxyStatus {
  /** 整条链路是否在运行，前端总开关绑这个值。 */
  active: boolean;
  caddy_running: boolean;
  caddy_admin_up: boolean;
  certificate_ready: boolean;
  caddy_installed: boolean;
  mkcert_installed: boolean;
  hosts_applied: boolean;
  hosts_path: string;
  caddyfile_path: string;
  upstream: string;
  /** 代理环境变量警告；null 表示没有会绕过 hosts 的代理。 */
  proxy_warning: string | null;
  /** 当前生效的规则，顺序即匹配优先级。 */
  rules: ProxyRule[];
  managed_entries: HostsEntry[];
}

/** 新建一条空白规则。 */
export function emptyRule(): ProxyRule {
  return {
    id: "",
    name: "",
    domains: [],
    path: "",
    upstream: "",
    route_prefix: "",
    speed_group: "",
    probe_ms: null,
    probe_bytes: 0,
    probe_at: 0,
  };
}

/**
 * 由原始字段拼出展示文本，与 Rust 端 ProxyRule::summary 保持一致。
 */
export function ruleSummary(rule: ProxyRule): string {
  const domains = rule.domains.join("、");
  const path = rule.path.trim();
  const scope = path === "" || path === "/" ? "/*" : `${path}*`;
  const upstream = rule.upstream.trim();
  const prefix = rule.route_prefix.trim();
  const target = prefix === "" ? upstream : `${upstream}${prefix}`;
  const group = rule.speed_group.trim();
  return group === ""
    ? `${domains} ${scope} → ${target}`
    : `${domains} ${scope} → ${target}（测速组 ${group}）`;
}

/**
 * 把测速结果换算成下载速度文本，直接放在「测速」按钮上。
 *
 * 速度 = 下载字节数 ÷ 下载用时（测速是真实下载一个文件算出来的）。
 * 没测过或上次失败时返回 null，按钮保持「测速」字样等待点击。
 */
export function formatSpeed(rule: ProxyRule): string | null {
  if (rule.probe_ms == null || rule.probe_ms <= 0 || rule.probe_bytes <= 0) {
    return null;
  }
  const bps = rule.probe_bytes / (rule.probe_ms / 1000);
  return `${formatBytes(bps)}/s`;
}

/** 当前 Unix 秒。 */
export function nowSecs(): number {
  return Math.floor(Date.now() / 1000);
}

/** 流量汇总。字段与 Rust 端 traffic::TrafficSummary 对应。 */
export interface TrafficSummary {
  requests: number;
  bytes: number;
  success: number;
  failed: number;
  cache_hits: number;
  cf_hits: number;
  avg_ms: number;
}

export interface DomainTraffic {
  domain: string;
  requests: number;
  bytes: number;
}

export interface TrafficBucket {
  ts: number;
  requests: number;
  bytes: number;
}

export interface RecentRequest {
  host: string;
  uri: string;
  status: number;
  size: number;
  duration_ms: number;
  ago_secs: number;
}

export interface TrafficReport {
  summary: TrafficSummary;
  per_domain: DomainTraffic[];
  buckets: TrafficBucket[];
  recent: RecentRequest[];
}

/** 把字节数格式化成人类可读的形式。 */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value < 10 ? 2 : 1)} ${units[unit]}`;
}

export interface CheckResult {
  name: string;
  success: boolean;
  status_code: number | null;
  detail: string;
  elapsed_ms: number;
}

/** 由 Rust 的原始字段拼出展示文本，避免把展示逻辑放进后端。 */
export function checkSummary(c: CheckResult): string {
  let out = c.success ? "通过" : "失败";
  if (c.status_code != null) out += `（${c.status_code}）`;
  if (c.detail) out += ` ${c.detail}`;
  out += ` ${c.elapsed_ms} ms`;
  return out;
}

/** 「关于」页的作者信息，字段名与 Rust 的 AuthorInfo 一致。 */
export interface AuthorInfo {
  /** 显示名（GitHub 资料里的 name），缺失时回落登录名。 */
  name: string;
  /** GitHub 登录名。 */
  login: string;
  bio: string;
  location: string;
  github_url: string;
  /** 赞助 / 主页链接，取自 GitHub blog 字段或 bio 里的链接。 */
  sponsor_url: string;
  public_repos: number;
  followers: number;
  /** Minecraft 正版用户名，与 Microsoft 账号同名。 */
  mc_name: string;
  /** Minecraft 头像的 data URL；拉取失败时为空串，前端显示占位。 */
  avatar: string;
  /** live=实时拉取；snapshot=内置快照。 */
  source: string;
}

export interface HostsApplyResult {
  added: HostsEntry[];
  removed: HostsEntry[];
  backup_path: string | null;
}

export interface StopResult {
  killed: string[];
  error: string | null;
}

/** 解析导入配置的结果：可导入的规则列表 + 不影响导入的提示。与 Rust 端 ConfigPreview 一致。 */
export interface ConfigPreview {
  rules: ProxyRule[];
  warnings: string[];
}

/** 一条规则展示用的分组条目。 */
export interface RuleGroupItem {
  /** 展示编号（1 起）。 */
  order: number;
  /** 该组内所有规则，按列表顺序。 */
  rules: ProxyRule[];
  /** 组内是否有多个候选（即会参与测速竞争）。 */
  isGroup: boolean;
  /** 组名：多个候选时用 shared speed_group，否则取第一条的 name。 */
  title: string;
  /** 组的域名集合。 */
  domains: string[];
  /** 组的路径前缀。 */
  path: string;
  /** 组内规则的 id 列表，用于计算插入位置。 */
  ids: string[];
}

/**
 * 把规则列表按「同域名 + 同路径」归组，供界面折叠展示。
 *
 * 自动识别用户配了多条同域名同路径的转发规则（典型是同一个 Maven
 * 仓库配了多个上游），它们属于同一组：组内只有一条真正生效，其余被
 * 测速结果压制。界面上把它们收进一个下拉框，避免列表刷屏。
 *
 * 未分组的规则保持原位；同组的规则连续排在一起（按首次出现的顺序）。
 */
export function groupRules(rules: ProxyRule[]): RuleGroupItem[] {
  // 组 key -> 组内规则下标
  const groups = new Map<string, number[]>();

  rules.forEach((rule, i) => {
    const key = groupKey(rule);
    const list = groups.get(key);
    if (list) list.push(i);
    else groups.set(key, [i]);
  });

  // 按首次出现的顺序输出
  const items: RuleGroupItem[] = [];
  const seen = new Set<string>();

  rules.forEach((rule, i) => {
    const key = groupKey(rule);
    if (seen.has(key)) return;
    seen.add(key);

    const indices = groups.get(key)!;
    const members = indices.map((j) => rules[j]);

    if (members.length === 1) {
      items.push({
        order: items.length + 1,
        rules: members,
        isGroup: false,
        title: members[0].name,
        domains: [...members[0].domains],
        path: members[0].path,
        ids: [members[0].id],
      });
    } else {
      const shared = members[0].speed_group.trim();
      items.push({
        order: items.length + 1,
        rules: members,
        isGroup: true,
        title: shared !== "" ? `组 ${shared}` : `多上游 ${members[0].domains[0] ?? ""}`,
        domains: [...members[0].domains],
        path: members[0].path,
        ids: members.map((m) => m.id),
      });
    }
  });

  return items;
}

/** 分组 key：域名集合（排序）+ 路径前缀。 */
function groupKey(rule: ProxyRule): string {
  const domains = [...rule.domains].map((d) => d.trim().toLowerCase()).sort().join(",");
  const path = rule.path.trim();
  return `${domains}|${path === "/" ? "" : path}`;
}
