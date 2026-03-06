use colored::*;

use crate::parser::{LogLine, Severity};

pub fn print_line(line: &LogLine, show_line_numbers: bool, highlight_pattern: Option<&regex::Regex>) {
    let mut output = String::new();

    // line number
    if show_line_numbers {
        output.push_str(&format!("{:>6} │ ", line.line_number.to_string().dimmed()));
    }

    // timestamp
    if let Some(ref ts) = line.timestamp {
        output.push_str(&format!("{} ", ts.format("%H:%M:%S").to_string().dimmed()));
    }

    // severity badge
    let badge = format_severity(&line.severity);
    output.push_str(&format!("{} ", badge));

    // message
    let msg = if line.is_json {
        format_json_message(line)
    } else {
        strip_metadata(&line.raw)
    };

    // highlight matching pattern
    let msg = if let Some(pattern) = highlight_pattern {
        highlight_matches(&msg, pattern)
    } else {
        colorize_message(&msg, &line.severity)
    };

    output.push_str(&msg);
    println!("{}", output);
}

fn format_severity(severity: &Severity) -> String {
    match severity {
        Severity::Error => "ERR".white().on_red().bold().to_string(),
        Severity::Warn => "WRN".black().on_yellow().bold().to_string(),
        Severity::Info => "INF".white().on_blue().to_string(),
        Severity::Debug => "DBG".white().on_bright_black().to_string(),
        Severity::Trace => "TRC".dimmed().to_string(),
        Severity::Unknown => "   ".to_string(),
    }
}

fn colorize_message(msg: &str, severity: &Severity) -> String {
    match severity {
        Severity::Error => msg.red().to_string(),
        Severity::Warn => msg.yellow().to_string(),
        Severity::Info => msg.normal().to_string(),
        Severity::Debug => msg.dimmed().to_string(),
        Severity::Trace => msg.dimmed().to_string(),
        Severity::Unknown => msg.normal().to_string(),
    }
}

fn highlight_matches(msg: &str, pattern: &regex::Regex) -> String {
    let mut result = String::new();
    let mut last_end = 0;

    for mat in pattern.find_iter(msg) {
        result.push_str(&msg[last_end..mat.start()]);
        result.push_str(&mat.as_str().black().on_yellow().bold().to_string());
        last_end = mat.end();
    }
    result.push_str(&msg[last_end..]);
    result
}

fn strip_metadata(raw: &str) -> String {
    // try to strip common prefixes like [2026-02-24 11:09:48.583] [info]
    // to show just the message portion
    let mut s = raw.to_string();

    // strip leading timestamp brackets
    if s.starts_with('[') {
        if let Some(end) = s.find(']') {
            s = s[end + 1..].to_string();
        }
    }

    // strip severity bracket
    let trimmed = s.trim_start();
    if trimmed.starts_with('[') {
        if let Some(end) = trimmed.find(']') {
            s = trimmed[end + 1..].to_string();
        }
    }

    s.trim().to_string()
}

fn format_json_message(line: &LogLine) -> String {
    if let Some(ref val) = line.json_value {
        if let Some(obj) = val.as_object() {
            // show message + any extra fields
            let mut parts = vec![line.message.clone()];

            for (k, v) in obj {
                match k.as_str() {
                    "message" | "msg" | "text" | "level" | "severity" | "loglevel"
                    | "timestamp" | "time" | "ts" | "@timestamp" | "datetime" => continue,
                    _ => {
                        let val_str = if v.is_string() {
                            v.as_str().unwrap_or("").to_string()
                        } else {
                            v.to_string()
                        };
                        parts.push(format!(
                            "{}={}",
                            k.dimmed(),
                            val_str.cyan()
                        ));
                    }
                }
            }

            return parts.join(" ");
        }
    }
    line.message.clone()
}
