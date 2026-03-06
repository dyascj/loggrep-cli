use anyhow::Result;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use crate::display;
use crate::filter::Filter;
use crate::parser;

/// Follow a file like `tail -f`, applying filters and coloring output
pub fn follow_file(
    path: &Path,
    filter: &Filter,
    show_line_numbers: bool,
    highlight_pattern: Option<&regex::Regex>,
) -> Result<()> {
    let mut file = File::open(path)?;
    let mut reader = BufReader::new(file.try_clone()?);

    // seek to end
    file.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::End(0))?;

    let mut line_number = count_lines(path)?;

    println!(
        "{}",
        colored::Colorize::dimmed(
            format!("Following {} (Ctrl+C to stop)...", path.display()).as_str()
        )
    );

    let (tx, rx) = mpsc::channel();

    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    watcher.watch(path, RecursiveMode::NonRecursive)?;

    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(event)) => {
                if matches!(event.kind, EventKind::Modify(_)) {
                    // read new lines
                    let mut line = String::new();
                    while reader.read_line(&mut line)? > 0 {
                        line_number += 1;
                        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
                        if !trimmed.is_empty() {
                            let parsed = parser::parse_line(trimmed, line_number);
                            if filter.matches(&parsed) {
                                display::print_line(&parsed, show_line_numbers, highlight_pattern);
                            }
                        }
                        line.clear();
                    }
                }
            }
            Ok(Err(e)) => {
                eprintln!("Watch error: {:?}", e);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // check for new content periodically even without events
                let mut line = String::new();
                while reader.read_line(&mut line)? > 0 {
                    line_number += 1;
                    let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
                    if !trimmed.is_empty() {
                        let parsed = parser::parse_line(trimmed, line_number);
                        if filter.matches(&parsed) {
                            display::print_line(&parsed, show_line_numbers, highlight_pattern);
                        }
                    }
                    line.clear();
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok(())
}

fn count_lines(path: &Path) -> Result<usize> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    Ok(reader.lines().count())
}
