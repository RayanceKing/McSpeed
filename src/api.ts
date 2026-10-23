import { invoke } from "@tauri-apps/api/core";
import type {
  AuthorInfo,
  CheckResult,
  ConfigPreview,
  HostsApplyResult,
  ProxyRule,
  ProxyStatus,
  StopResult,
  TrafficReport,
} from "./types";

/** 读取所有组件的当前状态。 */
export function getStatus(): Promise<ProxyStatus> {
  return invoke<ProxyStatus>("get_status");
}

/** 一键开关：true 拉起整条链路，false 停 Caddy 并清 hosts。 */
export function toggleProxy(enable: boolean): Promise<ProxyStatus> {
  return invoke<ProxyStatus>("toggle_proxy", { enable });
}

/** 新增一条规则，返回最新状态。 */
export function addRule(rule: ProxyRule): Promise<ProxyStatus> {
  return invoke<ProxyStatus>("add_rule", { rule });
}

/** 更新一条规则（按 id 匹配），返回最新状态。 */
export function updateRule(rule: ProxyRule): Promise<ProxyStatus> {
  return invoke<ProxyStatus>("update_rule", { rule });
}

/** 删除一条规则，返回最新状态。 */
export function deleteRule(id: string): Promise<ProxyStatus> {
  return invoke<ProxyStatus>("delete_rule", { id });
}

/** 整体替换规则列表，用于排序后的提交。 */
export function setRules(rules: ProxyRule[]): Promise<ProxyStatus> {
  return invoke<ProxyStatus>("set_rules", { rules });
}

/**
 * 测速：真实下载每个规则上游的一段内容，按下载用时在组内排序。
 * 不传 ids 测全部；传 ids 只测指定的几条（单条或整组）。
 * 测速可能改变同组里哪条规则胜出，所以返回的状态已含最新生效配置与新顺序。
 */
export function probeRules(ids?: string[]): Promise<ProxyStatus> {
  return invoke<ProxyStatus>("probe_rules", { ids: ids && ids.length > 0 ? ids : null });
}

/** 导出完整配置（全部规则，含顺序与测速结果）为 JSON 文本。 */
export function exportConfigText(): Promise<string> {
  return invoke<string>("export_config_text");
}

/** 把导出的配置文本写入用户选好的文件。 */
export function writeConfigFile(path: string, text: string): Promise<void> {
  return invoke("write_config_file", { path, text });
}

/** 读取用户选中的配置文件全文。 */
export function readConfigFile(path: string): Promise<string> {
  return invoke<string>("read_config_file", { path });
}

/** 从链接下载配置全文。 */
export function fetchConfigText(url: string): Promise<string> {
  return invoke<string>("fetch_config_text", { url });
}

/** 解析配置文本（文件 / 剪贴板 / 链接三个来源共用），返回可导入的规则列表。 */
export function parseConfig(text: string): Promise<ConfigPreview> {
  return invoke<ConfigPreview>("parse_config", { text });
}

/** 用导入的规则整体替换当前配置，并同步 hosts 与 Caddy。 */
export function importConfig(rules: ProxyRule[]): Promise<ProxyStatus> {
  return invoke<ProxyStatus>("import_config", { rules });
}

/** 读取流量统计。 */
export function getTraffic(): Promise<TrafficReport> {
  return invoke<TrafficReport>("get_traffic");
}

/** 一键配置：证书 → hosts → Caddy。 */
export function setupAll(): Promise<ProxyStatus> {
  return invoke<ProxyStatus>("setup_all");
}

export function startCaddy(): Promise<ProxyStatus> {
  return invoke<ProxyStatus>("start_caddy");
}

export function stopCaddy(): Promise<StopResult> {
  return invoke<StopResult>("stop_caddy");
}

export function reloadCaddy(): Promise<void> {
  return invoke("reload_caddy");
}

export function applyHosts(): Promise<HostsApplyResult> {
  return invoke<HostsApplyResult>("apply_hosts");
}

export function removeHosts(): Promise<HostsApplyResult> {
  return invoke<HostsApplyResult>("remove_hosts");
}

export function issueCertificate(): Promise<string> {
  return invoke<string>("issue_certificate");
}

/** 卸载本地 CA：把 mkcert 根证书从系统信任库里移除，CA 文件本身保留。 */
export function uninstallCertificate(): Promise<void> {
  return invoke("uninstall_certificate");
}

export function runCheck(): Promise<CheckResult[]> {
  return invoke<CheckResult[]>("run_check");
}

export function openHostsFolder(): Promise<void> {
  return invoke("open_hosts_folder");
}

/** 作者信息（GitHub 资料 + Minecraft 头像）。断网时后端回落内置快照，不会失败。 */
export function getAuthorInfo(): Promise<AuthorInfo> {
  return invoke<AuthorInfo>("get_author_info");
}
