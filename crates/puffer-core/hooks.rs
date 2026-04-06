use puffer_resources::LoadedResources;
use std::path::Path;
use std::process::Command;

/// Runs matching resource hooks for one event with the provided environment map.
pub fn run_resource_hooks(
    resources: &LoadedResources,
    cwd: &Path,
    event: &str,
    envs: &[(&str, String)],
) {
    for hook in resources
        .hooks
        .iter()
        .filter(|hook| hook.value.event == event)
    {
        let mut command = Command::new("sh");
        command
            .arg("-lc")
            .arg(&hook.value.command)
            .current_dir(cwd)
            .env("PUFFER_HOOK_ID", &hook.value.id)
            .env("PUFFER_HOOK_EVENT", event);
        for (key, value) in envs {
            command.env(key, value);
        }
        let _ = command.output();
    }
}
