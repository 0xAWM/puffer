use anyhow::{anyhow, Result};
use std::path::Path;

use crate::{run_command_capture, CommandOutput};

/// Describes the dimensions used when parsing terminal output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub rows: u16,
    pub cols: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            rows: 40,
            cols: 120,
        }
    }
}

/// Captures the raw command result together with a VT-normalized screen view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VtCapture {
    pub command: CommandOutput,
    pub screen: String,
}

/// Normalizes snapshot text to make terminal output comparisons stable.
pub fn normalize_snapshot_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parses a VT100 byte stream into normalized screen text.
///
/// This is a best-effort parser for ANSI terminal output and is not a full PTY.
pub fn render_vt_text(bytes: &[u8], size: TerminalSize) -> String {
    let mut screen = vec![vec![' '; usize::from(size.cols)]; usize::from(size.rows)];
    let mut row = 0usize;
    let mut col = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\x1b' => {
                index +=
                    apply_escape_sequence(&bytes[index..], &mut screen, &mut row, &mut col, size);
            }
            b'\r' => {
                col = 0;
                index += 1;
            }
            b'\n' => {
                advance_row(&mut row, size);
                col = 0;
                index += 1;
            }
            b'\t' => {
                let next_stop = ((col / 8) + 1) * 8;
                col = next_stop.min(usize::from(size.cols).saturating_sub(1));
                index += 1;
            }
            0x08 => {
                col = col.saturating_sub(1);
                index += 1;
            }
            byte if !byte.is_ascii_control() => {
                write_byte(&mut screen, &mut row, &mut col, size, byte);
                index += 1;
            }
            _ => {
                index += 1;
            }
        }
    }
    screen_to_text(&screen)
}

/// Runs a command, captures its piped output, and parses stdout as VT100 text.
///
/// This helper is useful for snapshotting ANSI-heavy output in tests without
/// introducing a full pseudo-terminal dependency.
pub fn capture_command_vt(
    program: &str,
    args: &[&str],
    cwd: Option<&Path>,
    size: TerminalSize,
) -> Result<VtCapture> {
    let command = run_command_capture(program, args, cwd)?;
    let screen = render_vt_text(command.stdout.as_bytes(), size);
    Ok(VtCapture { command, screen })
}

/// Asserts that one normalized text blob contains another.
pub fn assert_contains(haystack: &str, needle: &str) -> Result<()> {
    let haystack = normalize_snapshot_text(haystack);
    let needle = normalize_snapshot_text(needle);
    if haystack.contains(&needle) {
        Ok(())
    } else {
        Err(anyhow!("expected normalized output to contain `{needle}`"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_removes_crlf_and_trailing_spaces() {
        let normalized = normalize_snapshot_text("a  \r\nb\t \r\n");
        assert_eq!(normalized, "a\nb");
    }

    #[test]
    fn renders_vt_text_into_a_stable_screen() {
        let screen = render_vt_text(
            b"hello\x1b[2Dyy\r\nsecond line",
            TerminalSize { rows: 6, cols: 20 },
        );
        assert_eq!(screen, "helyy\nsecond line");
    }

    #[test]
    fn captures_command_and_parses_vt_stdout() {
        let capture = capture_command_vt(
            "sh",
            &["-lc", "printf '\\033[2J\\033[Hhello\\nworld'"],
            None,
            TerminalSize { rows: 8, cols: 20 },
        )
        .unwrap();
        assert_eq!(capture.command.status_code, 0);
        assert!(capture.screen.contains("hello"));
        assert!(capture.screen.contains("world"));
    }
}

fn write_byte(
    screen: &mut [Vec<char>],
    row: &mut usize,
    col: &mut usize,
    size: TerminalSize,
    byte: u8,
) {
    if *row >= usize::from(size.rows) {
        return;
    }
    if *col >= usize::from(size.cols) {
        advance_row(row, size);
        *col = 0;
    }
    if *row < usize::from(size.rows) && *col < usize::from(size.cols) {
        screen[*row][*col] = char::from(byte);
        *col += 1;
    }
}

fn advance_row(row: &mut usize, size: TerminalSize) {
    let max_row = usize::from(size.rows).saturating_sub(1);
    *row = (*row + 1).min(max_row);
}

fn apply_escape_sequence(
    bytes: &[u8],
    screen: &mut [Vec<char>],
    row: &mut usize,
    col: &mut usize,
    size: TerminalSize,
) -> usize {
    if bytes.len() < 2 {
        return 1;
    }
    if bytes[1] == b'[' {
        return apply_csi_sequence(bytes, screen, row, col, size);
    }
    1
}

fn apply_csi_sequence(
    bytes: &[u8],
    screen: &mut [Vec<char>],
    row: &mut usize,
    col: &mut usize,
    size: TerminalSize,
) -> usize {
    let mut end = 2usize;
    while end < bytes.len() {
        let byte = bytes[end];
        if (0x40..=0x7e).contains(&byte) {
            break;
        }
        end += 1;
    }
    if end >= bytes.len() {
        return bytes.len();
    }
    let final_byte = bytes[end];
    let params = std::str::from_utf8(&bytes[2..end]).unwrap_or("");
    let numbers = parse_csi_numbers(params);
    match final_byte {
        b'A' => {
            let count = first_or_default(&numbers, 1);
            *row = row.saturating_sub(count);
        }
        b'B' => {
            let count = first_or_default(&numbers, 1);
            *row = (*row + count).min(usize::from(size.rows).saturating_sub(1));
        }
        b'C' => {
            let count = first_or_default(&numbers, 1);
            *col = (*col + count).min(usize::from(size.cols).saturating_sub(1));
        }
        b'D' => {
            let count = first_or_default(&numbers, 1);
            *col = col.saturating_sub(count);
        }
        b'G' => {
            let target = first_or_default(&numbers, 1);
            *col = target
                .saturating_sub(1)
                .min(usize::from(size.cols).saturating_sub(1));
        }
        b'H' | b'f' => {
            let target_row = first_or_default(&numbers, 1);
            let target_col = numbers.get(1).copied().unwrap_or(1);
            *row = target_row
                .saturating_sub(1)
                .min(usize::from(size.rows).saturating_sub(1));
            *col = target_col
                .saturating_sub(1)
                .min(usize::from(size.cols).saturating_sub(1));
        }
        b'J' => clear_screen(screen, row, col, size, first_or_default(&numbers, 0)),
        b'K' => clear_line(screen, *row, *col, size, first_or_default(&numbers, 0)),
        b'm' => {}
        _ => {}
    }
    end + 1
}

fn parse_csi_numbers(params: &str) -> Vec<usize> {
    if params.is_empty() {
        return Vec::new();
    }
    params
        .split(';')
        .map(|value| value.parse::<usize>().unwrap_or(0))
        .collect()
}

fn first_or_default(values: &[usize], default: usize) -> usize {
    values
        .first()
        .copied()
        .filter(|value| *value != 0)
        .unwrap_or(default)
}

fn clear_screen(
    screen: &mut [Vec<char>],
    row: &usize,
    col: &usize,
    size: TerminalSize,
    mode: usize,
) {
    match mode {
        0 => {
            clear_line(screen, *row, *col, size, 0);
            for current_row in (*row + 1)..usize::from(size.rows) {
                clear_line(screen, current_row, 0, size, 2);
            }
        }
        1 => {
            for current_row in 0..*row {
                clear_line(screen, current_row, 0, size, 2);
            }
            clear_line(screen, *row, *col, size, 1);
        }
        2 => {
            for current_row in 0..usize::from(size.rows) {
                clear_line(screen, current_row, 0, size, 2);
            }
        }
        _ => {}
    }
}

fn clear_line(screen: &mut [Vec<char>], row: usize, col: usize, size: TerminalSize, mode: usize) {
    if row >= usize::from(size.rows) {
        return;
    }
    match mode {
        0 => {
            for current_col in col..usize::from(size.cols) {
                screen[row][current_col] = ' ';
            }
        }
        1 => {
            for current_col in 0..=col.min(usize::from(size.cols).saturating_sub(1)) {
                screen[row][current_col] = ' ';
            }
        }
        2 => {
            for current_col in 0..usize::from(size.cols) {
                screen[row][current_col] = ' ';
            }
        }
        _ => {}
    }
}

fn screen_to_text(screen: &[Vec<char>]) -> String {
    let mut lines = screen
        .iter()
        .map(|row| row.iter().collect::<String>().trim_end().to_string())
        .collect::<Vec<_>>();
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    normalize_snapshot_text(&lines.join("\n"))
}
