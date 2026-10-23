use crate::caddy::access_log_path;
use serde::Serialize;
use std::io::BufRead;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// 一条从 access log 里解析出来的请求记录。
#[derive(Debug, Clone)]
pub struct RequestRecord {
    /// Unix 时间戳（秒）
    pub ts: f64,
    pub host: String,
    pub uri: String,
    pub status: u16,
    /// 响应体字节数
    pub size: u64,
    /// 耗时（秒）
    pub duration: f64,
}

/// 流量汇总。
#[derive(Debug, Clone, Default, Serialize)]
pub struct TrafficSummary {
    /// 总请求数
    pub requests: u64,
    /// 总下载字节
    pub bytes: u64,
    /// 2xx 的请求数
    pub success: u64,
    /// 非 2xx 的请求数
    pub failed: u64,
    /// 本地 Caddy 缓存命中数（X-Proxy-Cache: HIT）
    pub cache_hits: u64,
    /// Cloudflare 边缘命中数（Cf-Cache-Status: HIT）
    pub cf_hits: u64,
    /// 平均耗时（毫秒）
    pub avg_ms: u64,
}

/// 按域名分组的流量。
#[derive(Debug, Clone, Serialize)]
pub struct DomainTraffic {
    pub domain: String,
    pub requests: u64,
    pub bytes: u64,
}

/// 一个时间桶，用于画折线图。
#[derive(Debug, Clone, Serialize)]
pub struct TrafficBucket {
    /// 桶起点，Unix 秒
    pub ts: u64,
    pub requests: u64,
    pub bytes: u64,
}

/// 完整的流量视图。
#[derive(Debug, Clone, Serialize)]
pub struct TrafficReport {
    pub summary: TrafficSummary,
    pub per_domain: Vec<DomainTraffic>,
    /// 最近 N 分钟，按 30 秒分桶
    pub buckets: Vec<TrafficBucket>,
    /// 最近若干条请求，便于排查
    pub recent: Vec<RecentRequest>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecentRequest {
    pub host: String,
    pub uri: String,
    pub status: u16,
    pub size: u64,
    pub duration_ms: u64,
    /// 距离现在多少秒
    pub ago_secs: u64,
}

/// 桶宽度（秒）。30 秒一个点，5 分钟窗口就是 10 个点。
const BUCKET_SECS: u64 = 30;
const BUCKET_COUNT: usize = 20;
const RECENT_LIMIT: usize = 12;

/// 读取并统计 access log。
///
/// 日志是 JSON Lines，每行一个对象。解析失败的行直接跳过——
/// 日志可能在滚动中被截断，出现半行是正常的。
pub fn analyze() -> TrafficReport {
    let path = access_log_path();
    let records = read_records(&path);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut report = build_report(records, now);

    // 缓存命中数在 resp_headers 里，单独扫一遍再合并
    let (local_hits, cf_hits) = count_cache_hits(&path);
    report.summary.cache_hits = local_hits;
    report.summary.cf_hits = cf_hits;

    report
}

/// 解析日志文件，只保留能认出来的行。
fn read_records(path: &Path) -> Vec<RequestRecord> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let reader = std::io::BufReader::new(file);
    let mut out = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if let Some(record) = parse_line(&line) {
            out.push(record);
        }
    }

    // 日志追加写入，理论上有序；仍排一次以防 roll 导致乱序
    out.sort_by(|a, b| a.ts.partial_cmp(&b.ts).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// 从一行 JSON 里抠出需要的字段。
///
/// 不用 serde derive：日志字段很多，而我们只关心五六个，
/// 手写提取比定义一整套结构体更省事，也不会因为上游加字段而挂。
fn parse_line(line: &str) -> Option<RequestRecord> {
    let ts = extract_number(line, "\"ts\":")?;
    let status = extract_number(line, "\"status\":")? as u16;
    let size = extract_number(line, "\"size\":")? as u64;
    let duration = extract_number(line, "\"duration\":").unwrap_or(0.0);

    // request.host 和 request.uri 嵌在嵌套对象里，按 key 定位即可
    let host = extract_string(line, "\"host\":")?;
    let uri = extract_string(line, "\"uri\":").unwrap_or_default();

    Some(RequestRecord {
        ts,
        host,
        uri,
        status,
        size,
        duration,
    })
}

fn build_report(records: Vec<RequestRecord>, now: u64) -> TrafficReport {
    let mut summary = TrafficSummary::default();
    let mut per_domain: std::collections::HashMap<String, DomainTraffic> =
        std::collections::HashMap::new();

    // 桶从 now 往前推 BUCKET_COUNT 个宽度
    let window_start = now.saturating_sub(BUCKET_SECS * BUCKET_COUNT as u64);
    let mut buckets: Vec<TrafficBucket> = (0..BUCKET_COUNT)
        .map(|i| TrafficBucket {
            ts: window_start + i as u64 * BUCKET_SECS,
            requests: 0,
            bytes: 0,
        })
        .collect();

    let mut recent: Vec<RecentRequest> = Vec::new();

    for record in &records {
        summary.requests += 1;
        summary.bytes += record.size;
        if (200..300).contains(&record.status) {
            summary.success += 1;
        } else {
            summary.failed += 1;
        }

        let entry = per_domain
            .entry(record.host.clone())
            .or_insert_with(|| DomainTraffic {
                domain: record.host.clone(),
                requests: 0,
                bytes: 0,
            });
        entry.requests += 1;
        entry.bytes += record.size;

        // 只把落在窗口内的记录放进桶，避免历史数据把图表压平
        let ts = record.ts as u64;
        if ts >= window_start && ts <= now {
            let index = ((ts - window_start) / BUCKET_SECS) as usize;
            if index < buckets.len() {
                buckets[index].requests += 1;
                buckets[index].bytes += record.size;
            }
        }

        if record.ts as u64 + 300 >= now {
            recent.push(RecentRequest {
                host: record.host.clone(),
                uri: record.uri.clone(),
                status: record.status,
                size: record.size,
                duration_ms: (record.duration * 1000.0) as u64,
                ago_secs: now.saturating_sub(record.ts as u64),
            });
        }
    }

    if summary.requests > 0 {
        let total_duration: f64 = records.iter().map(|r| r.duration).sum();
        summary.avg_ms = ((total_duration / summary.requests as f64) * 1000.0) as u64;
    }

    let mut per_domain: Vec<DomainTraffic> = per_domain.into_values().collect();
    per_domain.sort_by(|a, b| b.bytes.cmp(&a.bytes));

    recent.sort_by(|a, b| b.ago_secs.cmp(&a.ago_secs));
    recent.truncate(RECENT_LIMIT);

    TrafficReport {
        summary,
        per_domain,
        buckets,
        recent,
    }
}

/// 提取 `"key": 123.45` 形式的数字。
fn extract_number(line: &str, key: &str) -> Option<f64> {
    let start = line.find(key)? + key.len();
    let rest = &line[start..];
    let end = rest
        .find(|c: char| c == ',' || c == '}' || c.is_whitespace())
        .unwrap_or(rest.len());
    rest[..end].trim().parse::<f64>().ok()
}

/// 提取 `"key": "value"` 形式的字符串。
fn extract_string(line: &str, key: &str) -> Option<String> {
    let start = line.find(key)? + key.len();
    let rest = &line[start..];
    let quote_start = rest.find('"')? + 1;
    let after = &rest[quote_start..];
    let quote_end = after.find('"')?;
    Some(after[..quote_end].to_string())
}

/// 统计缓存命中。日志里 cache 信息在 resp_headers 中，
/// 这里单独扫一遍，避免污染上面的提取逻辑。
pub fn count_cache_hits(path: &Path) -> (u64, u64) {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return (0, 0),
    };

    let reader = std::io::BufReader::new(file);
    let mut local_hits = 0;
    let mut cf_hits = 0;

    for line in reader.lines().flatten() {
        if line.contains("\"X-Proxy-Cache\":[\"HIT\"]") {
            local_hits += 1;
        }
        if line.contains("Cf-Cache-Status") && line.contains("HIT") {
            cf_hits += 1;
        }
    }

    (local_hits, cf_hits)
}
