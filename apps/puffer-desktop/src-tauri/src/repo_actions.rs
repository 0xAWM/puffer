use crate::dtos::{ActionResultDto, PullRequestDto, RepoStatusDto};
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Loads repository status for the given working directory.
pub(crate) fn load_repo_status(cwd: impl Into<PathBuf>) -> Result<RepoStatusDto> {
    let cwd = cwd.into();
    let gh_available = command_available("gh");
    if run_command(&cwd, "git", &["rev-parse", "--is-inside-work-tree"]).is_err() {
        let reason = "Current session is not in a git repository.".to_string();
        return Ok(RepoStatusDto {
            cwd: cwd.display().to_string(),
            is_git_repo: false,
            branch: None,
            has_uncommitted_changes: false,
            git_status: String::new(),
            gh_available,
            gh_authenticated: false,
            create_pr_enabled: false,
            create_pr_reason: Some(reason.clone()),
            merge_pr_enabled: false,
            merge_pr_reason: Some(reason.clone()),
            active_pull_request: None,
            error: Some(reason),
        });
    }

    let branch = run_command(&cwd, "git", &["branch", "--show-current"])
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let git_status = run_command(&cwd, "git", &["status", "--short"]).unwrap_or_default();
    let has_uncommitted_changes = !git_status.trim().is_empty();
    let gh_authenticated = gh_available
        && Command::new("gh")
            .args(["auth", "status"])
            .current_dir(&cwd)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
    let active_pull_request = if gh_authenticated {
        current_pull_request(&cwd).ok().flatten()
    } else {
        None
    };
    let create_pr_reason = create_pr_reason(
        gh_available,
        gh_authenticated,
        branch.as_deref(),
        active_pull_request.as_ref(),
    );
    let merge_pr_reason = merge_pr_reason(
        gh_available,
        gh_authenticated,
        branch.as_deref(),
        active_pull_request.as_ref(),
    );

    Ok(RepoStatusDto {
        cwd: cwd.display().to_string(),
        is_git_repo: true,
        branch,
        has_uncommitted_changes,
        git_status,
        gh_available,
        gh_authenticated,
        create_pr_enabled: create_pr_reason.is_none(),
        create_pr_reason,
        merge_pr_enabled: merge_pr_reason.is_none(),
        merge_pr_reason,
        active_pull_request,
        error: None,
    })
}

/// Creates a pull request for the current repository branch.
pub(crate) fn create_pull_request(
    cwd: impl Into<PathBuf>,
    title: Option<String>,
    body: Option<String>,
) -> Result<ActionResultDto> {
    let cwd = cwd.into();
    ensure_gh_ready(&cwd)?;
    let args = build_create_pr_args(title.as_deref(), body.as_deref());
    let output = Command::new("gh")
        .args(args.iter().map(String::as_str))
        .current_dir(&cwd)
        .output()
        .context("failed to launch `gh pr create`")?;
    let repo_status = load_repo_status(&cwd)?;
    if !output.status.success() {
        return Ok(ActionResultDto {
            success: false,
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            pull_request: repo_status.active_pull_request.clone(),
            repo_status: Some(repo_status),
        });
    }
    let repo_status = load_repo_status(&cwd)?;
    Ok(ActionResultDto {
        success: true,
        message: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        pull_request: repo_status.active_pull_request.clone(),
        repo_status: Some(repo_status),
    })
}

/// Merges the active or selected pull request for the current repository.
pub(crate) fn merge_pull_request(
    cwd: impl Into<PathBuf>,
    pull_request_number: Option<u64>,
    merge_method: Option<&str>,
) -> Result<ActionResultDto> {
    let cwd = cwd.into();
    ensure_gh_ready(&cwd)?;
    let args = build_merge_pr_args(pull_request_number, merge_method);
    let output = Command::new("gh")
        .args(args.iter().map(String::as_str))
        .current_dir(&cwd)
        .output()
        .context("failed to launch `gh pr merge`")?;
    let repo_status = load_repo_status(&cwd)?;
    if !output.status.success() {
        return Ok(ActionResultDto {
            success: false,
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            pull_request: repo_status.active_pull_request.clone(),
            repo_status: Some(repo_status),
        });
    }
    let repo_status = load_repo_status(&cwd)?;
    Ok(ActionResultDto {
        success: true,
        message: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        pull_request: repo_status.active_pull_request.clone(),
        repo_status: Some(repo_status),
    })
}

fn create_pr_reason(
    gh_available: bool,
    gh_authenticated: bool,
    branch: Option<&str>,
    pull_request: Option<&PullRequestDto>,
) -> Option<String> {
    if !gh_available {
        return Some("GitHub CLI (`gh`) is not installed.".to_string());
    }
    if !gh_authenticated {
        return Some("GitHub CLI is not authenticated.".to_string());
    }
    if branch.is_none() {
        return Some("No active git branch is available.".to_string());
    }
    if let Some(pr) = pull_request {
        return Some(format!("PR #{} is already open for this branch.", pr.number));
    }
    None
}

fn merge_pr_reason(
    gh_available: bool,
    gh_authenticated: bool,
    branch: Option<&str>,
    pull_request: Option<&PullRequestDto>,
) -> Option<String> {
    if !gh_available {
        return Some("GitHub CLI (`gh`) is not installed.".to_string());
    }
    if !gh_authenticated {
        return Some("GitHub CLI is not authenticated.".to_string());
    }
    if branch.is_none() {
        return Some("No active git branch is available.".to_string());
    }
    if pull_request.is_none() {
        return Some("No active pull request for the current branch.".to_string());
    }
    None
}

fn build_create_pr_args(title: Option<&str>, body: Option<&str>) -> Vec<String> {
    let title = title.map(str::trim).filter(|value| !value.is_empty());
    let body = body.map(str::trim).filter(|value| !value.is_empty());
    let mut args = vec!["pr".to_string(), "create".to_string()];
    match (title, body) {
        (Some(title), Some(body)) => {
            args.extend([
                "--title".to_string(),
                title.to_string(),
                "--body".to_string(),
                body.to_string(),
            ]);
        }
        (Some(title), None) => {
            args.extend([
                "--title".to_string(),
                title.to_string(),
                "--fill".to_string(),
            ]);
        }
        (None, Some(body)) => {
            args.extend([
                "--fill".to_string(),
                "--body".to_string(),
                body.to_string(),
            ]);
        }
        (None, None) => args.push("--fill".to_string()),
    }
    args
}

fn build_merge_pr_args(pull_request_number: Option<u64>, merge_method: Option<&str>) -> Vec<String> {
    let mut args = vec!["pr".to_string(), "merge".to_string()];
    if let Some(number) = pull_request_number {
        args.push(number.to_string());
    }
    args.push(match merge_method.unwrap_or("merge") {
        "rebase" => "--rebase".to_string(),
        "squash" => "--squash".to_string(),
        _ => "--merge".to_string(),
    });
    args.push("--delete-branch".to_string());
    args
}

fn ensure_gh_ready(cwd: &Path) -> Result<()> {
    let status = load_repo_status(cwd)?;
    if !status.is_git_repo {
        return Err(anyhow!(
            status
                .error
                .unwrap_or_else(|| "Current session is not in a git repository.".to_string())
        ));
    }
    if !status.gh_available {
        return Err(anyhow!("GitHub CLI (`gh`) is not installed."));
    }
    if !status.gh_authenticated {
        return Err(anyhow!("GitHub CLI is not authenticated."));
    }
    Ok(())
}

fn current_pull_request(cwd: &Path) -> Result<Option<PullRequestDto>> {
    let raw = run_command(
        cwd,
        "gh",
        &[
            "pr",
            "view",
            "--json",
            "number,title,url,state,isDraft,mergeStateStatus,baseRefName,headRefName",
        ],
    );
    let Ok(raw) = raw else {
        return Ok(None);
    };
    let parsed: Value = serde_json::from_str(&raw).context("failed to parse gh pr view json")?;
    Ok(Some(PullRequestDto {
        number: parsed.get("number").and_then(Value::as_u64).unwrap_or_default(),
        title: parsed
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Pull Request")
            .to_string(),
        url: parsed
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        state: parsed
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("UNKNOWN")
            .to_string(),
        merge_state_status: parsed
            .get("mergeStateStatus")
            .and_then(Value::as_str)
            .map(str::to_string),
        is_draft: parsed
            .get("isDraft")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        base_ref_name: parsed
            .get("baseRefName")
            .and_then(Value::as_str)
            .map(str::to_string),
        head_ref_name: parsed
            .get("headRefName")
            .and_then(Value::as_str)
            .map(str::to_string),
    }))
}

fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn run_command(cwd: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to launch `{program}`"))?;
    if !output.status.success() {
        return Err(anyhow!(
            "{}",
            String::from_utf8_lossy(&output.stderr).trim().to_string()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::{build_create_pr_args, build_merge_pr_args, create_pr_reason, merge_pr_reason};
    use crate::dtos::PullRequestDto;

    fn sample_pr() -> PullRequestDto {
        PullRequestDto {
            number: 17,
            title: "Desktop UI".to_string(),
            url: "https://example.com/pr/17".to_string(),
            state: "OPEN".to_string(),
            merge_state_status: Some("CLEAN".to_string()),
            is_draft: false,
            base_ref_name: Some("master".to_string()),
            head_ref_name: Some("feature".to_string()),
        }
    }

    #[test]
    fn create_pr_args_use_title_and_body_when_both_are_present() {
        let args = build_create_pr_args(Some("Title"), Some("Body"));
        assert_eq!(args, vec!["pr", "create", "--title", "Title", "--body", "Body"]);
    }

    #[test]
    fn merge_pr_args_support_requested_strategy_and_number() {
        let args = build_merge_pr_args(Some(99), Some("squash"));
        assert_eq!(args, vec!["pr", "merge", "99", "--squash", "--delete-branch"]);
    }

    #[test]
    fn create_pr_reason_requires_auth_and_no_existing_pr() {
        assert_eq!(
            create_pr_reason(false, false, Some("feature"), None).as_deref(),
            Some("GitHub CLI (`gh`) is not installed.")
        );
        assert_eq!(
            create_pr_reason(true, false, Some("feature"), None).as_deref(),
            Some("GitHub CLI is not authenticated.")
        );
        assert_eq!(
            create_pr_reason(true, true, Some("feature"), Some(&sample_pr())).as_deref(),
            Some("PR #17 is already open for this branch.")
        );
        assert_eq!(create_pr_reason(true, true, Some("feature"), None), None);
    }

    #[test]
    fn merge_pr_reason_requires_an_active_pr() {
        assert_eq!(
            merge_pr_reason(true, true, Some("feature"), None).as_deref(),
            Some("No active pull request for the current branch.")
        );
        assert_eq!(merge_pr_reason(true, true, Some("feature"), Some(&sample_pr())), None);
    }
}
