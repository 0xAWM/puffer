use std::fs;
use std::path::{Path, PathBuf};

const MAX_RUST_FILE_LINES: usize = 1000;

#[test]
fn exported_functions_have_doc_comments() {
    let mut missing = Vec::new();
    for path in rust_files() {
        let contents = fs::read_to_string(&path).expect("read Rust file");
        let lines = contents.lines().collect::<Vec<_>>();
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if !is_exported_function(trimmed) {
                continue;
            }
            if !has_doc_comment(&lines, index) {
                missing.push(format!("{}:{}", display_path(&path), index + 1));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "missing doc comments for exported functions:\n{}",
        missing.join("\n")
    );
}

#[test]
fn rust_files_stay_under_line_limit() {
    let mut oversized = Vec::new();
    for path in rust_files() {
        let contents = fs::read_to_string(&path).expect("read Rust file");
        let line_count = contents.lines().count();
        if line_count > MAX_RUST_FILE_LINES {
            oversized.push(format!("{} ({line_count})", display_path(&path)));
        }
    }

    assert!(
        oversized.is_empty(),
        "Rust files exceed {} lines:\n{}",
        MAX_RUST_FILE_LINES,
        oversized.join("\n")
    );
}

fn rust_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rust_files(&repo_root().join("crates"), &mut files);
    files.sort();
    files
}

fn collect_rust_files(root: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(root).expect("read directory");
    for entry in entries {
        let entry = entry.expect("directory entry");
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some("target") {
                continue;
            }
            collect_rust_files(&path, files);
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn is_exported_function(trimmed: &str) -> bool {
    trimmed.starts_with("pub fn ") || trimmed.starts_with("pub async fn ")
}

fn has_doc_comment(lines: &[&str], function_index: usize) -> bool {
    let mut index = function_index;
    while index > 0 {
        index -= 1;
        let trimmed = lines[index].trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("#[") {
            continue;
        }
        return trimmed.starts_with("///");
    }
    false
}

fn display_path(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .expect("repo-relative path")
        .display()
        .to_string()
}
