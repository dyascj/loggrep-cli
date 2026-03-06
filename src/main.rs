mod display;
mod filter;
mod follow;
mod parser;
mod stats;

use anyhow::{Context, Result};
use clap::Parser;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "loggrep")]
#[command(about = "A smarter log parser with color-coded severity, time filtering, regex matching, and stats")]
#[command(version)]
#[command(after_help = r#"EXAMPLES:
  loggrep app.log                          Show all lines, color-coded
  loggrep app.log -l error                 Show only errors
  loggrep app.log -l warn+                 Show warnings and above
  loggrep app.log -p "GPU|crash"           Regex filter
  loggrep app.log --from "11:00" --to "12:00"  Time range
  loggrep app.log --stats                  Show stats summary
  loggrep app.log -f                       Follow mode (like tail -f)
  loggrep app.log -l error -p "OOM" -s     Combine filters + stats
  cat app.log | loggrep                    Read from stdin"#)]
struct Cli {
    /// Log file(s) to parse. Omit to read from stdin.
    files: Vec<PathBuf>,

    /// Filter by severity (error, warn, info, debug, trace).
    /// Use comma-separated for multiple: "error,warn"
    /// Use "+" suffix for threshold: "warn+" means warn and above
    #[arg(short = 'l', long = "level")]
    level: Option<String>,

    /// Regex pattern to filter/search lines
    #[arg(short, long)]
    pattern: Option<String>,

    /// Show only lines from this time (e.g. "11:00", "2026-02-24 11:00:00")
    #[arg(long)]
    from: Option<String>,

    /// Show only lines until this time
    #[arg(long)]
    to: Option<String>,

    /// Show stats summary after output
    #[arg(short = 's', long = "stats")]
    show_stats: bool,

    /// Stats only — don't print individual lines
    #[arg(short = 'S', long = "stats-only")]
    stats_only: bool,

    /// Follow the file (like tail -f)
    #[arg(short = 'f', long = "follow")]
    follow: bool,

    /// Show line numbers
    #[arg(short = 'n', long = "line-numbers")]
    line_numbers: bool,

    /// Invert filter (show lines that DON'T match)
    #[arg(short = 'v', long = "invert")]
    invert: bool,

    /// Output matching lines as JSON (useful for piping)
    #[arg(long = "json")]
    json_output: bool,

    /// Print only the count of matching lines
    #[arg(short = 'c', long = "count")]
    count_only: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // build filter
    let mut log_filter = filter::Filter::new();

    if let Some(ref level) = cli.level {
        log_filter.severity = Some(filter::parse_severity_filter(level));
    }

    if let Some(ref pat) = cli.pattern {
        log_filter.pattern = Some(
            regex::Regex::new(pat).context(format!("Invalid regex pattern: {}", pat))?,
        );
    }

    if let Some(ref from) = cli.from {
        log_filter.from = filter::parse_datetime(from);
        if log_filter.from.is_none() {
            eprintln!("Warning: couldn't parse --from '{}', ignoring", from);
        }
    }

    if let Some(ref to) = cli.to {
        log_filter.to = filter::parse_datetime(to);
        if log_filter.to.is_none() {
            eprintln!("Warning: couldn't parse --to '{}', ignoring", to);
        }
    }

    log_filter.invert = cli.invert;

    // highlight pattern (used for display, same as filter pattern)
    let highlight = log_filter.pattern.as_ref();

    // follow mode
    if cli.follow {
        if cli.files.is_empty() {
            eprintln!("Error: --follow requires a file argument");
            std::process::exit(1);
        }
        for path in &cli.files {
            follow::follow_file(path, &log_filter, cli.line_numbers, highlight)?;
        }
        return Ok(());
    }

    // normal mode: process files or stdin
    let mut log_stats = stats::Stats::new();

    if cli.files.is_empty() {
        // read from stdin
        let stdin = io::stdin();
        let reader = stdin.lock();
        process_lines(
            reader,
            &log_filter,
            &mut log_stats,
            &cli,
            highlight,
        )?;
    } else {
        for path in &cli.files {
            if cli.files.len() > 1 {
                println!(
                    "{}",
                    colored::Colorize::bold(
                        colored::Colorize::cyan(
                            format!("═══ {} ═══", path.display()).as_str()
                        )
                    )
                );
            }
            let file = File::open(path)
                .context(format!("Failed to open file: {}", path.display()))?;
            let reader = BufReader::new(file);
            process_lines(
                reader,
                &log_filter,
                &mut log_stats,
                &cli,
                highlight,
            )?;
        }
    }

    // output modes
    if cli.count_only {
        println!("{}", log_stats.matched);
    } else if cli.show_stats || cli.stats_only {
        log_stats.print_summary();
    }

    Ok(())
}

fn process_lines<R: BufRead>(
    reader: R,
    log_filter: &filter::Filter,
    log_stats: &mut stats::Stats,
    cli: &Cli,
    highlight: Option<&regex::Regex>,
) -> Result<()> {
    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            continue;
        }

        let parsed = parser::parse_line(trimmed, i + 1);
        let matched = log_filter.matches(&parsed);

        log_stats.record(&parsed, matched);

        if matched && !cli.stats_only && !cli.count_only {
            if cli.json_output {
                print_json_line(&parsed);
            } else {
                display::print_line(&parsed, cli.line_numbers, highlight);
            }
        }
    }

    Ok(())
}

fn print_json_line(line: &parser::LogLine) {
    let obj = serde_json::json!({
        "line": line.line_number,
        "timestamp": line.timestamp.map(|t| t.format("%Y-%m-%dT%H:%M:%S").to_string()),
        "severity": line.severity.as_str(),
        "message": line.message,
        "raw": line.raw,
    });
    println!("{}", obj);
}
