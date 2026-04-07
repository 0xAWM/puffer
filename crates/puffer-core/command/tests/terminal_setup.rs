use super::*;
use std::ffi::{OsStr, OsString};
use std::fs;

struct ScopedEnvVar {
    name: &'static str,
    old_value: Option<OsString>,
}

impl ScopedEnvVar {
    fn set(name: &'static str, value: impl AsRef<OsStr>) -> Self {
        let old_value = std::env::var_os(name);
        std::env::set_var(name, value);
        Self { name, old_value }
    }

    fn unset(name: &'static str) -> Self {
        let old_value = std::env::var_os(name);
        std::env::remove_var(name);
        Self { name, old_value }
    }
}

impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        if let Some(value) = self.old_value.take() {
            std::env::set_var(self.name, value);
        } else {
            std::env::remove_var(self.name);
        }
    }
}

fn scoped_terminal_env(
    term_program: Option<&str>,
    term: Option<&str>,
    tmux: Option<&str>,
) -> Vec<ScopedEnvVar> {
    vec![
        match term_program {
            Some(value) => ScopedEnvVar::set("TERM_PROGRAM", value),
            None => ScopedEnvVar::unset("TERM_PROGRAM"),
        },
        match term {
            Some(value) => ScopedEnvVar::set("TERM", value),
            None => ScopedEnvVar::unset("TERM"),
        },
        match tmux {
            Some(value) => ScopedEnvVar::set("TMUX", value),
            None => ScopedEnvVar::unset("TMUX"),
        },
        ScopedEnvVar::unset("STY"),
        ScopedEnvVar::unset("CURSOR_TRACE_ID"),
        ScopedEnvVar::unset("VSCODE_GIT_ASKPASS_MAIN"),
        ScopedEnvVar::unset("ALACRITTY_LOG"),
    ]
}

#[test]
fn terminal_setup_command_hides_on_native_terminals_and_uses_dynamic_description() {
    let _lock = lock_puffer_home();
    let _env = scoped_terminal_env(Some("WezTerm"), None, None);
    let commands = supported_commands();

    let command = find_command(&commands, "terminal-setup").expect("terminal-setup");
    assert_eq!(
        command.description,
        "Install Shift+Enter key binding for newlines"
    );
    assert!(command.hidden);
}

#[test]
fn terminal_setup_command_uses_apple_terminal_description() {
    let _lock = lock_puffer_home();
    let _env = scoped_terminal_env(Some("Apple_Terminal"), None, None);
    let commands = supported_commands();

    let command = find_command(&commands, "terminal-setup").expect("terminal-setup");
    assert_eq!(
        command.description,
        "Enable Option+Enter key binding for newlines and visual bell"
    );
    assert!(!command.hidden);
}

#[test]
fn terminal_setup_command_emits_native_terminal_guidance() {
    let _lock = lock_puffer_home();
    let _env = scoped_terminal_env(Some("WezTerm"), None, None);
    let tempdir = tempdir().unwrap();
    let paths = ConfigPaths::discover(tempdir.path());
    ensure_workspace_dirs(&paths).unwrap();
    let session_store = SessionStore::from_paths(&paths).unwrap();
    let session = session_store
        .create_session(tempdir.path().to_path_buf())
        .unwrap();
    let mut state = AppState::new(
        PufferConfig::default(),
        tempdir.path().to_path_buf(),
        session,
    );

    dispatch_command(
        &mut state,
        &supported_commands(),
        &LoadedResources::default(),
        &mut ProviderRegistry::new(),
        &mut AuthStore::default(),
        &session_store,
        "/terminal-setup",
    )
    .unwrap();

    assert!(state.transcript.last().is_some_and(|message| {
        message
            .text
            .contains("Shift+Enter is natively supported in WezTerm")
            && message
                .text
                .contains("No configuration needed. Just use Shift+Enter to add newlines.")
    }));
}

#[test]
fn terminal_setup_command_mentions_supported_reroute_for_tmux() {
    let _lock = lock_puffer_home();
    let _env = scoped_terminal_env(None, Some("screen-256color"), Some("1"));
    let tempdir = tempdir().unwrap();
    let paths = ConfigPaths::discover(tempdir.path());
    ensure_workspace_dirs(&paths).unwrap();
    let session_store = SessionStore::from_paths(&paths).unwrap();
    let session = session_store
        .create_session(tempdir.path().to_path_buf())
        .unwrap();
    let mut state = AppState::new(
        PufferConfig::default(),
        tempdir.path().to_path_buf(),
        session,
    );

    dispatch_command(
        &mut state,
        &supported_commands(),
        &LoadedResources::default(),
        &mut ProviderRegistry::new(),
        &mut AuthStore::default(),
        &session_store,
        "/terminal-setup",
    )
    .unwrap();

    assert!(state.transcript.last().is_some_and(|message| {
        message
            .text
            .contains("Terminal setup cannot be run from tmux")
            && message
                .text
                .contains("Run /terminal-setup directly in one of these terminals")
    }));
}

#[test]
fn terminal_setup_command_installs_vscode_keybinding() {
    let _lock = lock_puffer_home();
    let tempdir = tempdir().unwrap();
    let home = tempdir.path().join("home");
    let workspace = tempdir.path().join("workspace");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    let home_text = home.to_string_lossy().into_owned();
    let xdg_text = home.join(".config").to_string_lossy().into_owned();
    let _home = ScopedEnvVar::set("HOME", home_text.as_str());
    let _xdg = ScopedEnvVar::set("XDG_CONFIG_HOME", xdg_text.as_str());
    let _env = scoped_terminal_env(Some("vscode"), None, None);
    let paths = ConfigPaths::discover(&workspace);
    ensure_workspace_dirs(&paths).unwrap();
    let session_store = SessionStore::from_paths(&paths).unwrap();
    let session = session_store.create_session(workspace.clone()).unwrap();
    let mut state = AppState::new(PufferConfig::default(), workspace, session);

    dispatch_command(
        &mut state,
        &supported_commands(),
        &LoadedResources::default(),
        &mut ProviderRegistry::new(),
        &mut AuthStore::default(),
        &session_store,
        "/terminal-setup",
    )
    .unwrap();

    let keybindings = fs::read_to_string(home.join(".config/Code/User/keybindings.json")).unwrap();
    assert!(state.transcript.last().is_some_and(|message| {
        message
            .text
            .contains("Installed VSCode terminal Shift+Enter key binding")
    }));
    assert!(keybindings.contains("\"shift+enter\""));
    assert!(keybindings.contains("workbench.action.terminal.sendSequence"));
}
