use crate::app_server::RpcClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub key: String,
    pub limit_id: String,
    pub limit_name: Option<String>,
    pub kind: String,
    pub used_percent: u8,
    pub remaining_percent: u8,
    pub window_duration_mins: u64,
    pub resets_at: u64,
    pub plan_type: Option<String>,
    pub limit_amount: Option<f64>,
    pub used_amount: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaSnapshot {
    pub windows: Vec<QuotaWindow>,
    pub fetched_at: u64,
    pub source: String,
    pub stale: bool,
}

pub fn fetch_quota(client: &RpcClient) -> Result<QuotaSnapshot, String> {
    let account = client.request("account/read", json!({ "refreshToken": false }))?;
    let account_type = account
        .get("account")
        .and_then(|value| value.get("type"))
        .and_then(Value::as_str);
    if account_type.is_none() {
        return Err("AUTH_REQUIRED".into());
    }
    if !matches!(
        account_type,
        Some("chatgpt" | "chatgptAuthTokens" | "personalAccessToken" | "agentIdentity")
    ) {
        return Err("AUTH_UNSUPPORTED".into());
    }
    let limits = client.request("account/rateLimits/read", json!({}))?;
    let plan = account
        .get("account")
        .and_then(|value| value.get("planType"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let windows = parse_limits(&limits, plan)?;
    if windows.is_empty() {
        return Err("RATE_LIMITS_EMPTY".into());
    }
    Ok(QuotaSnapshot {
        windows,
        fetched_at: now_millis(),
        source: "live".into(),
        stale: false,
    })
}

fn parse_limits(result: &Value, plan: Option<String>) -> Result<Vec<QuotaWindow>, String> {
    let mut windows = Vec::new();
    if let Some(buckets) = result.get("rateLimitsByLimitId").and_then(Value::as_object) {
        let has_individual_limit = buckets.values().any(|bucket| {
            bucket
                .get("individualLimit")
                .filter(|value| value.is_object())
                .is_some()
        });
        for (limit_id, bucket) in buckets {
            if has_individual_limit {
                push_individual_limit(&mut windows, limit_id, bucket, plan.clone());
            } else {
                push_bucket(&mut windows, limit_id, bucket, plan.clone());
            }
        }
    } else if let Some(bucket) = result.get("rateLimits") {
        let limit_id = bucket
            .get("limitId")
            .and_then(Value::as_str)
            .unwrap_or("codex");
        if bucket
            .get("individualLimit")
            .filter(|value| value.is_object())
            .is_some()
        {
            push_individual_limit(&mut windows, limit_id, bucket, plan);
        } else {
            push_bucket(&mut windows, limit_id, bucket, plan);
        }
    }
    Ok(windows)
}

fn push_individual_limit(
    output: &mut Vec<QuotaWindow>,
    limit_id: &str,
    bucket: &Value,
    plan: Option<String>,
) {
    let Some(limit) = bucket
        .get("individualLimit")
        .filter(|value| value.is_object())
    else {
        return;
    };
    let limit_amount = number(limit.get("limit"));
    let used_amount = number(limit.get("used"));
    let remaining = number(limit.get("remainingPercent"))
        .or_else(|| Some(100.0 - (used_amount? / limit_amount?).clamp(0.0, 1.0) * 100.0))
        .unwrap_or(0.0)
        .clamp(0.0, 100.0)
        .round() as u8;
    let limit_name = bucket
        .get("limitName")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| Some("Monthly usage limit".to_string()));
    let resets = timestamp_millis(limit.get("resetsAt"));
    output.push(QuotaWindow {
        key: format!("{limit_id}:individual"),
        limit_id: limit_id.into(),
        limit_name,
        kind: "individual".into(),
        used_percent: 100 - remaining,
        remaining_percent: remaining,
        window_duration_mins: 0,
        resets_at: resets,
        plan_type: plan,
        limit_amount,
        used_amount,
    });
}

fn push_bucket(
    output: &mut Vec<QuotaWindow>,
    limit_id: &str,
    bucket: &Value,
    plan: Option<String>,
) {
    let limit_name = bucket
        .get("limitName")
        .and_then(Value::as_str)
        .map(str::to_string);
    for kind in ["primary", "secondary"] {
        let Some(window) = bucket.get(kind).filter(|value| value.is_object()) else {
            continue;
        };
        let used = number(window.get("usedPercent"))
            .unwrap_or(0.0)
            .clamp(0.0, 100.0)
            .round() as u8;
        let duration = number(window.get("windowDurationMins"))
            .unwrap_or(0.0)
            .max(0.0) as u64;
        let resets = timestamp_millis(window.get("resetsAt"));
        output.push(QuotaWindow {
            key: format!("{limit_id}:{kind}"),
            limit_id: limit_id.into(),
            limit_name: limit_name.clone(),
            kind: kind.into(),
            used_percent: used,
            remaining_percent: 100 - used,
            window_duration_mins: duration,
            resets_at: resets,
            plan_type: plan.clone(),
            limit_amount: None,
            used_amount: None,
        });
    }
}

fn number(value: Option<&Value>) -> Option<f64> {
    value.and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_u64().map(|number| number as f64))
            .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
    })
}

fn timestamp_millis(value: Option<&Value>) -> u64 {
    let value = number(value).unwrap_or(0.0).max(0.0) as u64;
    if value > 10_u64.pow(12) {
        value
    } else {
        value.saturating_mul(1000)
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::parse_limits;
    use serde_json::json;

    #[test]
    fn parses_primary_and_secondary_windows() {
        let value = json!({
            "rateLimitsByLimitId": {
                "codex": {
                    "limitName": "Codex",
                    "primary": { "usedPercent": 36, "windowDurationMins": 300, "resetsAt": 1700000000 },
                    "secondary": { "usedPercent": 59, "windowDurationMins": 10080, "resetsAt": 1701000000000_u64 }
                }
            }
        });
        let windows = parse_limits(&value, Some("business".into())).expect("valid limits");
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].remaining_percent, 64);
        assert_eq!(windows[0].resets_at, 1_700_000_000_000);
        assert_eq!(windows[1].remaining_percent, 41);
        assert_eq!(windows[1].resets_at, 1_701_000_000_000);
        assert_eq!(windows[0].plan_type.as_deref(), Some("business"));
    }

    #[test]
    fn prefers_the_account_usage_limit_when_present() {
        let value = json!({
            "rateLimitsByLimitId": {
                "codex_bengalfox": {
                    "limitName": "GPT-5.3-Codex-Spark-Preview",
                    "primary": { "usedPercent": 0, "windowDurationMins": 300, "resetsAt": 1700000000 }
                },
                "codex": {
                    "individualLimit": {
                        "limit": "7500",
                        "used": "6558.67",
                        "remainingPercent": 13,
                        "resetsAt": 1701000000
                    }
                }
            }
        });
        let windows = parse_limits(&value, Some("business".into())).expect("valid limits");
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].kind, "individual");
        assert_eq!(windows[0].remaining_percent, 13);
        assert_eq!(windows[0].limit_amount, Some(7500.0));
        assert_eq!(windows[0].used_amount, Some(6558.67));
    }
}
