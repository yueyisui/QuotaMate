use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const FIVE_HOUR_MINUTES: i64 = 300;
pub const WEEKLY_MINUTES: i64 = 10_080;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub used_percent: f64,
    pub remaining_percent: f64,
    pub reset_at: Option<i64>,
    pub window_minutes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitResetCredit {
    pub id: String,
    pub reset_type: String,
    pub status: String,
    pub granted_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitResetCredits {
    pub available_count: u64,
    pub credits: Option<Vec<RateLimitResetCredit>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodexUsage {
    pub five_hour: Option<UsageWindow>,
    pub weekly: Option<UsageWindow>,
    pub plan_type: Option<String>,
    pub rate_limit_reset_credits: Option<RateLimitResetCredits>,
    pub last_updated: Option<i64>,
    pub status: String,
    pub error: Option<String>,
}

impl CodexUsage {
    pub fn connecting() -> Self {
        Self {
            five_hour: None,
            weekly: None,
            plan_type: None,
            rate_limit_reset_credits: None,
            last_updated: None,
            status: "connecting".into(),
            error: None,
        }
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            five_hour: None,
            weekly: None,
            plan_type: None,
            rate_limit_reset_credits: None,
            last_updated: Some(Utc::now().timestamp()),
            status: "unavailable".into(),
            error: Some(message.into()),
        }
    }
}

pub fn parse_rate_limits_response(result: &Value) -> Result<CodexUsage, String> {
    let preferred = result
        .get("rateLimitsByLimitId")
        .and_then(|value| value.as_object())
        .and_then(|map| map.get("codex"));
    let snapshot = preferred
        .or_else(|| result.get("rateLimits"))
        .ok_or_else(|| {
            "The App Server response did not contain a Codex rate-limit snapshot".to_string()
        })?;

    let mut five_hour = None;
    let mut weekly = None;
    for key in ["primary", "secondary"] {
        let Some(window) = snapshot.get(key).filter(|value| !value.is_null()) else {
            continue;
        };
        let Some(window_minutes) = window.get("windowDurationMins").and_then(Value::as_i64) else {
            continue;
        };
        let used_percent = window
            .get("usedPercent")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .clamp(0.0, 100.0);
        let parsed = UsageWindow {
            used_percent,
            remaining_percent: (100.0 - used_percent).clamp(0.0, 100.0),
            reset_at: window.get("resetsAt").and_then(Value::as_i64),
            window_minutes,
        };
        match window_minutes {
            FIVE_HOUR_MINUTES => five_hour = Some(parsed),
            WEEKLY_MINUTES => weekly = Some(parsed),
            _ => log::warn!("Ignoring unknown Codex rate-limit window: {window_minutes} minutes"),
        }
    }

    if five_hour.is_none() {
        log::warn!("Five-hour Codex bucket unavailable");
    }
    if weekly.is_none() {
        log::warn!("Weekly Codex bucket unavailable");
    }

    let rate_limit_reset_credits = result
        .get("rateLimitResetCredits")
        .filter(|value| !value.is_null())
        .map(parse_reset_credits)
        .transpose()?;
    let plan_type = read_plan_type(snapshot).or_else(|| read_plan_type(result));

    Ok(CodexUsage {
        five_hour,
        weekly,
        plan_type,
        rate_limit_reset_credits,
        last_updated: Some(Utc::now().timestamp()),
        status: "available".into(),
        error: None,
    })
}

pub fn parse_account_plan_response(result: &Value) -> Option<String> {
    result.get("account").and_then(read_plan_type)
}

fn read_plan_type(value: &Value) -> Option<String> {
    value
        .get("planType")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_reset_credits(value: &Value) -> Result<RateLimitResetCredits, String> {
    let available_count = value
        .get("availableCount")
        .and_then(Value::as_u64)
        .ok_or_else(|| "Reset-credit details did not contain availableCount".to_string())?;
    let credits = match value.get("credits") {
        None | Some(Value::Null) => None,
        Some(Value::Array(items)) => Some(
            items
                .iter()
                .filter_map(|item| {
                    Some(RateLimitResetCredit {
                        id: item.get("id")?.as_str()?.to_string(),
                        reset_type: item
                            .get("resetType")
                            .and_then(Value::as_str)
                            .unwrap_or("codexRateLimits")
                            .to_string(),
                        status: item
                            .get("status")
                            .and_then(Value::as_str)
                            .unwrap_or("available")
                            .to_string(),
                        granted_at: item.get("grantedAt").and_then(Value::as_i64),
                        expires_at: item.get("expiresAt").and_then(Value::as_i64),
                        title: item
                            .get("title")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        description: item
                            .get("description")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    })
                })
                .collect(),
        ),
        Some(_) => return Err("Reset-credit details had an invalid credits value".into()),
    };
    Ok(RateLimitResetCredits {
        available_count,
        credits,
    })
}

pub fn response_from_notification(params: &Value) -> Value {
    serde_json::json!({ "rateLimits": params.get("rateLimits").cloned().unwrap_or(Value::Null) })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        FIVE_HOUR_MINUTES, WEEKLY_MINUTES, parse_account_plan_response, parse_rate_limits_response,
    };

    #[test]
    fn maps_windows_by_duration_not_position() {
        let response = json!({
            "rateLimits": {},
            "rateLimitsByLimitId": {
                "codex": {
                    "primary": { "usedPercent": 20, "windowDurationMins": WEEKLY_MINUTES, "resetsAt": 2 },
                    "secondary": { "usedPercent": 130, "windowDurationMins": FIVE_HOUR_MINUTES, "resetsAt": 1 }
                }
            }
        });
        let usage = parse_rate_limits_response(&response).unwrap();
        assert_eq!(usage.five_hour.unwrap().remaining_percent, 0.0);
        assert_eq!(usage.weekly.unwrap().remaining_percent, 80.0);
    }

    #[test]
    fn missing_window_is_supported() {
        let response = json!({
            "rateLimits": {
                "primary": { "usedPercent": 40, "windowDurationMins": WEEKLY_MINUTES, "resetsAt": null },
                "secondary": null
            }
        });
        let usage = parse_rate_limits_response(&response).unwrap();
        assert!(usage.five_hour.is_none());
        assert_eq!(usage.weekly.unwrap().remaining_percent, 60.0);
    }

    #[test]
    fn parses_reset_credit_count_and_times() {
        let response = json!({
            "rateLimits": {
                "primary": { "usedPercent": 40, "windowDurationMins": FIVE_HOUR_MINUTES, "resetsAt": 1 }
            },
            "rateLimitResetCredits": {
                "availableCount": 2,
                "credits": [{
                    "id": "credit-1",
                    "resetType": "codexRateLimits",
                    "status": "available",
                    "grantedAt": 10,
                    "expiresAt": 20,
                    "title": "Rate-limit reset",
                    "description": "Reset an eligible window."
                }]
            }
        });
        let credits = parse_rate_limits_response(&response)
            .unwrap()
            .rate_limit_reset_credits
            .unwrap();
        assert_eq!(credits.available_count, 2);
        let item = &credits.credits.unwrap()[0];
        assert_eq!(item.granted_at, Some(10));
        assert_eq!(item.expires_at, Some(20));
    }

    #[test]
    fn parses_plan_from_rate_limit_bucket() {
        let response = json!({
            "rateLimits": {
                "planType": "plus",
                "primary": { "usedPercent": 10, "windowDurationMins": FIVE_HOUR_MINUTES }
            }
        });
        assert_eq!(
            parse_rate_limits_response(&response).unwrap().plan_type,
            Some("plus".into())
        );
    }

    #[test]
    fn parses_plan_from_account_response() {
        let response = json!({
            "account": { "type": "chatgpt", "email": "hidden@example.com", "planType": "pro" }
        });
        assert_eq!(parse_account_plan_response(&response), Some("pro".into()));
    }
}
