use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::thread;
use std::time::Duration;

const MAX_DURATION_MS: u64 = 300_000;

#[derive(Debug, Deserialize)]
struct SleepInput {
    duration_ms: u64,
    #[serde(default)]
    reason: Option<String>,
}

pub(super) fn execute_sleep(input: Value) -> Result<String> {
    let input: SleepInput = serde_json::from_value(input)?;
    let duration_ms = normalize_duration_ms(input.duration_ms)?;
    thread::sleep(Duration::from_millis(duration_ms));
    Ok(serde_json::to_string_pretty(&json!({
        "duration_ms": duration_ms,
        "completed": true,
        "reason": input.reason,
    }))?)
}

fn normalize_duration_ms(duration_ms: u64) -> Result<u64> {
    if duration_ms == 0 {
        bail!("Sleep duration_ms must be greater than zero");
    }
    Ok(duration_ms.min(MAX_DURATION_MS))
}

#[cfg(test)]
mod tests {
    use super::{execute_sleep, normalize_duration_ms};
    use serde_json::json;

    #[test]
    fn sleep_rejects_zero_duration() {
        let error = execute_sleep(json!({"duration_ms": 0})).unwrap_err();
        assert!(error.to_string().contains("duration_ms"));
    }

    #[test]
    fn sleep_caps_duration_to_maximum() {
        assert_eq!(normalize_duration_ms(999_999).unwrap(), 300_000);
    }

    #[test]
    fn sleep_returns_completion_payload() {
        let output = execute_sleep(json!({"duration_ms": 1, "reason": "wait"})).unwrap();
        assert!(output.contains("\"duration_ms\": 1"));
        assert!(output.contains("\"completed\": true"));
        assert!(output.contains("\"reason\": \"wait\""));
    }
}
