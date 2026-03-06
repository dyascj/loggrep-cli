use colored::*;
use std::collections::HashMap;

use crate::parser::{LogLine, Severity};

#[derive(Default)]
pub struct Stats {
    pub total: usize,
    pub matched: usize,
    pub by_severity: HashMap<String, usize>,
    pub top_errors: HashMap<String, usize>,
    pub first_timestamp: Option<String>,
    pub last_timestamp: Option<String>,
}

impl Stats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, line: &LogLine, matched: bool) {
        self.total += 1;

        if matched {
            self.matched += 1;
        }

        // track severity counts (only matched lines)
        if matched {
            *self
                .by_severity
                .entry(line.severity.as_str().to_string())
                .or_insert(0) += 1;

            // track error messages
            if line.severity == Severity::Error {
                // normalize the error message (first 120 chars)
                let key = normalize_error(&line.message);
                *self.top_errors.entry(key).or_insert(0) += 1;
            }

            // track time range
            if let Some(ref ts) = line.timestamp {
                let ts_str = ts.format("%Y-%m-%d %H:%M:%S").to_string();
                if self.first_timestamp.is_none() {
                    self.first_timestamp = Some(ts_str.clone());
                }
                self.last_timestamp = Some(ts_str);
            }
        }
    }

    pub fn print_summary(&self) {
        println!();
        println!("{}", "━".repeat(60).dimmed());
        println!("{}", " STATS".bold());
        println!("{}", "━".repeat(60).dimmed());

        // overview
        println!(
            "  {} total lines, {} matched",
            self.total.to_string().bold(),
            self.matched.to_string().bold().cyan()
        );

        if let (Some(ref first), Some(ref last)) = (&self.first_timestamp, &self.last_timestamp) {
            println!("  {} → {}", first.dimmed(), last.dimmed());
        }

        println!();

        // severity breakdown
        println!("{}", " SEVERITY".bold());
        let severity_order = ["ERROR", "WARN", "INFO", "DEBUG", "TRACE", "???"];
        for sev in &severity_order {
            if let Some(&count) = self.by_severity.get(*sev) {
                let bar_len = (count as f64 / self.matched.max(1) as f64 * 30.0) as usize;
                let bar = "█".repeat(bar_len);

                let (label, bar_colored) = match *sev {
                    "ERROR" => ("ERR".red().bold().to_string(), bar.red().to_string()),
                    "WARN" => ("WRN".yellow().bold().to_string(), bar.yellow().to_string()),
                    "INFO" => ("INF".blue().to_string(), bar.blue().to_string()),
                    "DEBUG" => ("DBG".dimmed().to_string(), bar.dimmed().to_string()),
                    "TRACE" => ("TRC".dimmed().to_string(), bar.dimmed().to_string()),
                    _ => ("???".dimmed().to_string(), bar.dimmed().to_string()),
                };

                println!("  {} {:>5} {}", label, count, bar_colored);
            }
        }

        // top errors
        if !self.top_errors.is_empty() {
            println!();
            println!("{}", " TOP ERRORS".bold());

            let mut errors: Vec<_> = self.top_errors.iter().collect();
            errors.sort_by(|a, b| b.1.cmp(a.1));

            for (msg, count) in errors.iter().take(10) {
                println!(
                    "  {} {}",
                    format!("{:>4}×", count).red().bold(),
                    truncate(msg, 70).dimmed()
                );
            }
        }

        println!("{}", "━".repeat(60).dimmed());
    }
}

fn normalize_error(msg: &str) -> String {
    // strip timestamps, PIDs, memory addresses for deduplication
    let re_numbers =
        regex::Regex::new(r"(0x[0-9a-fA-F]+|\b\d{4,}\b|PID \d+)").unwrap();
    let normalized = re_numbers.replace_all(msg, "…");

    // strip common log prefixes
    let stripped = normalized
        .trim()
        .trim_start_matches(|c: char| c == '[' || c == ']' || c.is_whitespace());

    truncate(stripped, 120)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}
