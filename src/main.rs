mod config;
mod display;
mod filter;
mod follow;
mod parser;
mod stats;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use flate2::read::GzDecoder;
use std::collections::VecDeque;
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
  loggrep app.log -p "panic" -C 5          Context lines around matches
  loggrep app.log.gz -l error              Read gzip files directly
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

    /// Show N lines of context around matches (like grep -C)
    #[arg(short = 'C', long = "context", value_name = "N")]
    context: Option<usize>,

    /// Show N lines before each match (like grep -B)
    #[arg(short = 'B', long = "before-context", value_name = "N")]
    before_context: Option<usize>,

    /// Show N lines after each match (like grep -A)
    #[arg(short = 'A', long = "after-context", value_name = "N")]
    after_context: Option<usize>,

    /// Generate shell completions (bash, zsh, fish, elvish, powershell)
    #[arg(long = "completions", value_name = "SHELL")]
    completions: Option<Shell>,
}

fn main() -> Result<()> {
    let mut cli = Cli::parse();

    // shell completions
    if let Some(shell) = cli.completions {
        let mut cmd = Cli::command();
        generate(shell, &mut cmd, "loggrep", &mut io::stdout());
        return Ok(());
    }

    // apply config file defaults
    let cfg = config::load_config();
    if cli.level.is_none() {
        if let Some(ref level) = cfg.level {
            cli.level = Some(level.clone());
        }
    }
    if !cli.line_numbers && cfg.line_numbers {
        cli.line_numbers = true;
    }

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

    let file_prefix = cli.files.len() > 1;

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
            None,
        )?;
    } else {
        for path in &cli.files {
            if file_prefix {
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

            let filename = if file_prefix {
                Some(path.display().to_string())
            } else {
                None
            };

            // handle gzip files
            if path.extension().is_some_and(|ext| ext == "gz") {
                let decoder = GzDecoder::new(file);
                let reader = BufReader::new(decoder);
                process_lines(reader, &log_filter, &mut log_stats, &cli, highlight, filename.as_deref())?;
            } else {
                let reader = BufReader::new(file);
                process_lines(reader, &log_filter, &mut log_stats, &cli, highlight, filename.as_deref())?;
            }
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
    filename: Option<&str>,
) -> Result<()> {
    let before = cli.before_context.or(cli.context).unwrap_or(0);
    let after = cli.after_context.or(cli.context).unwrap_or(0);
    let use_context = before > 0 || after > 0;

    let mut before_buf: VecDeque<parser::LogLine> = VecDeque::new();
    let mut after_remaining: usize = 0;
    let mut last_printed_line: usize = 0;

    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            continue;
        }

        let parsed = parser::parse_line(trimmed, i + 1);
        let matched = log_filter.matches(&parsed);

        log_stats.record(&parsed, matched);

        if cli.stats_only || cli.count_only {
            continue;
        }

        if !use_context {
            if matched {
                print_output_line(&parsed, cli, highlight, filename);
            }
        } else if matched {
            // print separator if there's a gap
            if last_printed_line > 0 && parsed.line_number > last_printed_line + 1 {
                let first_context_line = if before_buf.is_empty() {
                    parsed.line_number
                } else {
                    before_buf.front().unwrap().line_number
                };
                if first_context_line > last_printed_line + 1 {
                    println!("{}", colored::Colorize::dimmed("--"));
                }
            }

            // print before-context lines
            for ctx_line in before_buf.drain(..) {
                if ctx_line.line_number > last_printed_line {
                    print_output_line(&ctx_line, cli, None, filename);
                    last_printed_line = ctx_line.line_number;
                }
            }

            print_output_line(&parsed, cli, highlight, filename);
            last_printed_line = parsed.line_number;
            after_remaining = after;
        } else if after_remaining > 0 {
            print_output_line(&parsed, cli, None, filename);
            last_printed_line = parsed.line_number;
            after_remaining -= 1;
        } else {
            before_buf.push_back(parsed);
            if before_buf.len() > before {
                before_buf.pop_front();
            }
        }
    }

    Ok(())
}

fn print_output_line(
    parsed: &parser::LogLine,
    cli: &Cli,
    highlight: Option<&regex::Regex>,
    filename: Option<&str>,
) {
    if cli.json_output {
        print_json_line(parsed);
    } else {
        if let Some(fname) = filename {
            print!(
                "{} ",
                colored::Colorize::magenta(
                    colored::Colorize::bold(format!("{}:", fname).as_str())
                )
            );
        }
        display::print_line(parsed, cli.line_numbers, highlight);
    }
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
