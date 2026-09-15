use serde_json::{Value, json};

pub fn initialize_request(id: u64) -> Value {
    json!({
        "id": id,
        "method": "initialize",
        "params": {
            "clientInfo": {
                "name": "quotamate",
                "title": "QuotaMate",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "experimentalApi": false,
                "optOutNotificationMethods": []
            }
        }
    })
}

pub fn initialized_notification() -> Value {
    json!({ "method": "initialized" })
}

pub fn rate_limits_request(id: u64) -> Value {
    json!({ "id": id, "method": "account/rateLimits/read" })
}

pub fn account_request(id: u64) -> Value {
    json!({ "id": id, "method": "account/read", "params": { "refreshToken": false } })
}
