use crate::dto::{PullRequestDto, RepoActionResultDto, RepoStatusDto};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// Returns desktop-friendly Git and GitHub status for one repository.
pub(crate) fn refresh_repo_status(cwd: &Path) -> Result<RepoStatusDto> {
    let cwd_text = cwd.display().to_string();
    if !is_git_repo(cwd) {
        return Ok(RepoStatusDto {
            cwd: cwd_text,
            is_git_repo: false,
            branch: None,
            has_uncommitted_changes: false,
            gh_available: command_exists("gh"),
            gh_authenticated: false,
            create_pr_enabled: false,
            create_pr_reason: Some("Not a git repository".to_string()),
            merge_pr_enabled: false,
            merge_pr_reason: Some("Not a git repository".to_string()),
            active_pull_request: None,
        });
    }

    let branch = run_command(cwd, "git", &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    let has_uncommitted_changes = !run_command(cwd, "git", &["status", "--short"])?
        .trim()
        .is_empty();
    let gh_available = command_exists("gh");
    let gh_authenticated = gh_available && gh_authenticated(cwd);
    let active_pull_request = if gh_authenticated {
        load_active_pull_request(cwd).ok().flatten()
    } else {
        None
    };

    let create_pr_reason = if !gh_available {
        Some("GitHub CLI is not installed".to_string())
    } else if !gh_authenticated {
        Some("GitHub CLI is not authenticated".to_string())
    } else {
        None
    };
    let merge_pr_reason = if !gh_available {
        Some("GitHub CLI is not installed".to_string())
    } else if !gh_authenticated {
        Some("GitHub CLI is not authenticated".to_string())
    } else if active_pull_request.is_none() {
        Some("No active pull request for the current branch".to_string())
    } else {
        None
    };

    Ok(RepoStatusDto {
        cwd: cwd_text,
        is_git_repo: true,
        branch: Some(branch),
        has_uncommitted_changes,
        gh_available,
        gh_authenticated,
        create_pr_enabled: create_pr_reason.is_none(),
        create_pr_reason,
        merge_pr_enabled: merge_pr_reason.is_none(),
        merge_pr_reason,
        active_pull_request,
    })
}

/// Creates a GitHub pull request from the selected repository.
pub(crate) fn create_pull_request(
    cwd: &Path,
    title: Option<String>,
    body: Option<String>,
) -> Result<RepoActionResultDto> {
    let status = refresh_repo_status(cwd)?;
    if !status.create_pr_enabled {
        bail!(
            "{}",
            status
                .create_pr_reason
                .clone()
                .unwrap_or_else(|| "Create PR is unavailable".to_string())
        );
    }

    let mut command = Command::new("gh");
    command.arg("pr").arg("create").current_dir(cwd);
    if let Some(value) = title.as_deref() {
        command.args(["--title", value]);
    } else {
        command.arg("--fill");
    }
    if let Some(value) = body.as_deref() {
        command.args(["--body", value]);
    }

    let output = command.output().context("failed to run gh pr create")?;
    if !output.status.success() {
        bail!("{}", command_error(&output.stderr));
    }

    let repo_status = refresh_repo_status(cwd)?;
    Ok(RepoActionResultDto {
        success: true,
        message: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        pull_request: repo_status.active_pull_request.clone(),
        repo_status,
    })
}

/// Merges a GitHub pull request from the selected repository.
pub(crate) fn merge_pull_request(
    cwd: &Path,
    pull_request_number: Option<u64>,
    merge_method: Option<String>,
) -> Result<RepoActionResultDto> {
    let status = refresh_repo_status(cwd)?;
    if !status.merge_pr_enabled {
        bail!(
            "{}",
            status
                .merge_pr_reason
                .clone()
                .unwrap_or_else(|| "Merge PR is unavailable".to_string())
        );
    }

    let mut command = Command::new("gh");
    command
        .arg("pr")
        .arg("merge")
        .arg(merge_method_flag(merge_method.as_deref())?)
        .arg("--delete-branch")
        .current_dir(cwd);
    if let Some(number) = pull_request_number {
        command.arg(number.to_string());
    }

    let output = command.output().context("failed to run gh pr merge")?;
    if !output.status.success() {
        bail!("{}", command_error(&output.stderr));
    }

    let repo_status = refresh_repo_status(cwd)?;
    Ok(RepoActionResultDto {
        success: true,
        message: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        pull_request: repo_status.active_pull_request.clone(),
        repo_status,
    })
}

fn is_git_repo(cwd: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(cwd)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn gh_authenticated(cwd: &Path) -> bool {
    Command::new("gh")
        .args(["auth", "status"])
        .current_dir(cwd)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn command_exists(binary: &str) -> bool {
    Command::new(binary)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn run_command(cwd: &Path, binary: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(binary)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to run {binary}"))?;
    if !output.status.success() {
        bail!("{}", command_error(&output.stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn load_active_pull_request(cwd: &Path) -> Result<Option<PullRequestDto>> {
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            "--json",
            "number,title,url,state,isDraft,mergeStateStatus,baseRefName,headRefName",
        ])
        .current_dir(cwd)
        .output()
        .context("failed to run gh pr view")?;
    if !output.status.success() {
        return Ok(None);
    }

    let value: Value =
        serde_json::from_slice(&output.stdout).context("failed to parse gh pr view output")?;
    Ok(Some(PullRequestDto {
        number: value
            .get("number")
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow!("gh pr view omitted number"))?,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("gh pr view omitted title"))?
            .to_string(),
        url: value
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("gh pr view omitted url"))?
            .to_string(),
        state: value
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("UNKNOWN")
            .to_string(),
        merge_state_status: value
            .get("mergeStateStatus")
            .and_then(Value::as_str)
            .map(str::to_string),
        is_draft: value
            .get("isDraft")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        base_ref_name: value
            .get("baseRefName")
            .and_then(Value::as_str)
            .map(str::to_string),
        head_ref_name: value
            .get("headRefName")
            .and_then(Value::as_str)
            .map(str::to_string),
    }))
}

fn merge_method_flag(method: Option<&str>) -> Result<&'static str> {
    match method.unwrap_or("merge") {
        "merge" => Ok("--merge"),
        "squash" => Ok("--squash"),
        "rebase" => Ok("--rebase"),
        other => bail!("unsupported merge method `{other}`"),
    }
}

fn command_error(stderr: &[u8]) -> String {
    let message = String::from_utf8_lossy(stderr).trim().to_string();
    if message.is_empty() {
        "command failed".to_string()
    } else {
        message
    }
}

#[cfg(test)]
mod tests {
    use super::merge_method_flag;

    #[test]
    fn resolves_known_merge_methods() {
        assert_eq!(merge_method_flag(None).unwrap(), "--merge");
        assert_eq!(merge_method_flag(Some("squash")).unwrap(), "--squash");
        assert_eq!(merge_method_flag(Some("rebase")).unwrap(), "--rebase");
    }

    #[test]
    fn rejects_unknown_merge_methods() {
        assert!(merge_method_flag(Some("fast-forward")).is_err());
    }
}
