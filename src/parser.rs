use chrono::NaiveDateTime;
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    Unknown,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Error => "ERROR",
            Severity::Warn => "WARN",
            Severity::Info => "INFO",
            Severity::Debug => "DEBUG",
            Severity::Trace => "TRACE",
            Severity::Unknown => "???",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogLine {
    pub raw: String,
    pub timestamp: Option<NaiveDateTime>,
    pub severity: Severity,
    pub message: String,
    pub line_number: usize,
    pub is_json: bool,
    pub json_value: Option<serde_json::Value>,
}

// common timestamp formats
static TIMESTAMP_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        // [2026-02-24 11:09:48.583]
        (
            Regex::new(r"\[(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3})\]").unwrap(),
            "%Y-%m-%d %H:%M:%S%.3f",
        ),
        // [2026-02-24 11:09:48]
        (
            Regex::new(r"\[(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\]").unwrap(),
            "%Y-%m-%d %H:%M:%S",
        ),
        // 2026-02-24T11:09:48.583Z (ISO 8601)
        (
            Regex::new(r"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3})Z?").unwrap(),
            "%Y-%m-%dT%H:%M:%S%.3f",
        ),
        // 2026-02-24T11:09:48Z
        (
            Regex::new(r"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2})Z?").unwrap(),
            "%Y-%m-%dT%H:%M:%S",
        ),
        // Feb 24 11:09:48 (syslog style - year will default to 2000, but relative filtering still works)
        (
            Regex::new(r"([A-Z][a-z]{2}\s+\d{1,2} \d{2}:\d{2}:\d{2})").unwrap(),
            "%b %d %H:%M:%S",
        ),
    ]
});

static SEVERITY_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(error|err|fatal|critical|warn|warning|info|debug|trace|verbose)\b")
        .unwrap()
});

pub fn parse_line(raw: &str, line_number: usize) -> LogLine {
    // try JSON first
    if let Some(json_line) = try_parse_json(raw) {
        return LogLine {
            raw: raw.to_string(),
            timestamp: json_line.timestamp,
            severity: json_line.severity,
            message: json_line.message,
            line_number,
            is_json: true,
            json_value: json_line.json_value,
        };
    }

    let timestamp = extract_timestamp(raw);
    let severity = extract_severity(raw);
    let message = raw.to_string();

    LogLine {
        raw: raw.to_string(),
        timestamp,
        severity,
        message,
        line_number,
        is_json: false,
        json_value: None,
    }
}

fn extract_timestamp(line: &str) -> Option<NaiveDateTime> {
    for (pattern, fmt) in TIMESTAMP_PATTERNS.iter() {
        if let Some(caps) = pattern.captures(line) {
            if let Some(m) = caps.get(1) {
                if let Ok(dt) = NaiveDateTime::parse_from_str(m.as_str(), fmt) {
                    return Some(dt);
                }
            }
        }
    }
    None
}

fn extract_severity(line: &str) -> Severity {
    if let Some(caps) = SEVERITY_PATTERN.captures(line) {
        if let Some(m) = caps.get(1) {
            return match m.as_str().to_lowercase().as_str() {
                "error" | "err" | "fatal" | "critical" => Severity::Error,
                "warn" | "warning" => Severity::Warn,
                "info" => Severity::Info,
                "debug" | "verbose" => Severity::Debug,
                "trace" => Severity::Trace,
                _ => Severity::Unknown,
            };
        }
    }
    Severity::Unknown
}

struct JsonParsed {
    timestamp: Option<NaiveDateTime>,
    severity: Severity,
    message: String,
    json_value: Option<serde_json::Value>,
}

fn try_parse_json(line: &str) -> Option<JsonParsed> {
    let trimmed = line.trim();
    if !trimmed.starts_with('{') {
        return None;
    }

    let val: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    let obj = val.as_object()?;

    // extract message from common fields
    let message = obj
        .get("message")
        .or_else(|| obj.get("msg"))
        .or_else(|| obj.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or(trimmed)
        .to_string();

    // extract timestamp
    let timestamp = obj
        .get("timestamp")
        .or_else(|| obj.get("time"))
        .or_else(|| obj.get("ts"))
        .or_else(|| obj.get("@timestamp"))
        .or_else(|| obj.get("datetime"))
        .and_then(|v| v.as_str())
        .and_then(|s| extract_timestamp(s).or_else(|| {
            // try parsing ISO directly
            NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.fZ").ok()
                .or_else(|| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").ok())
                .or_else(|| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok())
        }));

    // extract severity
    let severity = obj
        .get("level")
        .or_else(|| obj.get("severity"))
        .or_else(|| obj.get("loglevel"))
        .and_then(|v| v.as_str())
        .map(|s| match s.to_lowercase().as_str() {
            "error" | "err" | "fatal" | "critical" => Severity::Error,
            "warn" | "warning" => Severity::Warn,
            "info" | "information" => Severity::Info,
            "debug" => Severity::Debug,
            "trace" => Severity::Trace,
            _ => Severity::Unknown,
        })
        .unwrap_or_else(|| extract_severity(&message));

    Some(JsonParsed {
        timestamp,
        severity,
        message,
        json_value: Some(val),
    })
}
