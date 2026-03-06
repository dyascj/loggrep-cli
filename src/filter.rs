use chrono::NaiveDateTime;
use regex::Regex;

use crate::parser::{LogLine, Severity};

pub struct Filter {
    pub from: Option<NaiveDateTime>,
    pub to: Option<NaiveDateTime>,
    pub severity: Option<Vec<Severity>>,
    pub pattern: Option<Regex>,
    pub invert: bool,
}

impl Filter {
    pub fn new() -> Self {
        Self {
            from: None,
            to: None,
            severity: None,
            pattern: None,
            invert: false,
        }
    }

    pub fn matches(&self, line: &LogLine) -> bool {
        let result = self.matches_inner(line);
        if self.invert { !result } else { result }
    }

    fn matches_inner(&self, line: &LogLine) -> bool {
        // time range filter
        if let Some(ref from) = self.from {
            if let Some(ref ts) = line.timestamp {
                if ts < from {
                    return false;
                }
            }
        }

        if let Some(ref to) = self.to {
            if let Some(ref ts) = line.timestamp {
                if ts > to {
                    return false;
                }
            }
        }

        // severity filter
        if let Some(ref severities) = self.severity {
            if !severities.contains(&line.severity) {
                return false;
            }
        }

        // regex pattern filter
        if let Some(ref pattern) = self.pattern {
            if !pattern.is_match(&line.raw) {
                return false;
            }
        }

        true
    }
}

/// Parse a flexible datetime string into NaiveDateTime
/// Supports:
///   "2026-02-24 11:00:00"
///   "2026-02-24T11:00:00"
///   "2026-02-24 11:00"
///   "2026-02-24"
///   "11:00:00" (today)
///   "11:00" (today)
pub fn parse_datetime(s: &str) -> Option<NaiveDateTime> {
    let formats = [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d",
    ];

    for fmt in &formats {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt);
        }
    }

    // try time-only (assume today)
    let time_formats = ["%H:%M:%S", "%H:%M"];
    for fmt in &time_formats {
        if let Ok(t) = chrono::NaiveTime::parse_from_str(s, fmt) {
            let today = chrono::Local::now().date_naive();
            return Some(today.and_time(t));
        }
    }

    None
}

/// Parse severity filter string like "error", "error,warn", "error+"
pub fn parse_severity_filter(s: &str) -> Vec<Severity> {
    // "error+" means error and above
    if s.ends_with('+') {
        let base = &s[..s.len() - 1];
        return match base.to_lowercase().as_str() {
            "trace" => vec![
                Severity::Trace,
                Severity::Debug,
                Severity::Info,
                Severity::Warn,
                Severity::Error,
            ],
            "debug" => vec![
                Severity::Debug,
                Severity::Info,
                Severity::Warn,
                Severity::Error,
            ],
            "info" => vec![Severity::Info, Severity::Warn, Severity::Error],
            "warn" | "warning" => vec![Severity::Warn, Severity::Error],
            "error" | "err" => vec![Severity::Error],
            _ => vec![],
        };
    }

    // comma-separated
    s.split(',')
        .filter_map(|part| match part.trim().to_lowercase().as_str() {
            "error" | "err" | "fatal" => Some(Severity::Error),
            "warn" | "warning" => Some(Severity::Warn),
            "info" => Some(Severity::Info),
            "debug" => Some(Severity::Debug),
            "trace" => Some(Severity::Trace),
            _ => None,
        })
        .collect()
}
