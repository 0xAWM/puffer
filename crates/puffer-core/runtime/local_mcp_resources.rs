use crate::runtime::claude_tools::mcp_resources::{
    execute_list_mcp_resources_tool, execute_read_mcp_resource_tool, FilesystemMcpBlobStore,
    ListMcpResourcesToolInput, McpClientState, McpReadResourceContent, McpResourceClient,
    McpResourceRecord, ReadMcpResourceToolInput, ReadMcpResourceToolOutput,
};
use anyhow::{Context, Result};
use puffer_resources::{plugin_mcp_servers, LoadedResources};
use std::fs;
use std::path::{Path, PathBuf};

/// Lists the configured live MCP resource server names available in the current workspace.
pub(super) fn live_resource_server_names(resources: &LoadedResources) -> Vec<String> {
    let mut names = configured_live_resource_servers(resources)
        .into_iter()
        .map(|server| server.id)
        .collect::<Vec<_>>();
    names.sort_by_key(|value| value.to_ascii_lowercase());
    names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    names
}

/// Returns true when the given server name maps to a live MCP resource client.
pub(super) fn is_live_resource_server(resources: &LoadedResources, server: &str) -> bool {
    configured_live_resource_servers(resources)
        .into_iter()
        .any(|candidate| candidate.id.eq_ignore_ascii_case(server))
}

/// Executes live MCP `resources/list` calls for supported local runtime servers.
pub(super) fn list_live_mcp_resources(
    resources: &LoadedResources,
    cwd: &Path,
    server: Option<&str>,
) -> Result<String> {
    let mut clients = build_live_clients(resources, cwd);
    execute_list_mcp_resources_tool(
        ListMcpResourcesToolInput {
            server: server.map(str::to_string),
        },
        &mut clients,
    )
}

/// Executes a live MCP `resources/read` call for a supported local runtime server.
pub(super) fn read_live_mcp_resource(
    resources: &LoadedResources,
    cwd: &Path,
    server: &str,
    uri: &str,
) -> Result<ReadMcpResourceToolOutput> {
    let mut clients = build_live_clients(resources, cwd);
    let blob_store = FilesystemMcpBlobStore::new(cwd.join(".puffer").join("mcp-blobs"));
    let output = execute_read_mcp_resource_tool(
        ReadMcpResourceToolInput {
            server: server.to_string(),
            uri: uri.to_string(),
        },
        &mut clients,
        &blob_store,
    )?;
    serde_json::from_str(&output).context("failed to decode live MCP read output")
}

#[derive(Debug, Clone)]
struct LiveResourceServer {
    id: String,
}

#[derive(Debug, Clone)]
struct FilesystemResourceClient {
    name: String,
    root: PathBuf,
}

impl McpResourceClient for FilesystemResourceClient {
    fn name(&self) -> &str {
        &self.name
    }

    fn state(&self) -> McpClientState {
        McpClientState::Connected
    }

    fn supports_resources(&self) -> bool {
        true
    }

    fn ensure_connected(&mut self) -> Result<()> {
        Ok(())
    }

    fn list_resources(&mut self) -> Result<Vec<McpResourceRecord>> {
        let mut relative_paths = Vec::new();
        collect_workspace_files(&self.root, &self.root, &mut relative_paths)?;
        relative_paths.sort();
        relative_paths.truncate(200);
        Ok(relative_paths
            .into_iter()
            .map(|relative| {
                let path = self.root.join(&relative);
                McpResourceRecord {
                    uri: format!("mcp://filesystem/{relative}"),
                    name: relative,
                    mime_type: Some(mime_type_for_path(&path)),
                    description: Some("Live filesystem resource".to_string()),
                }
            })
            .collect())
    }

    fn read_resource(&mut self, uri: &str) -> Result<Vec<McpReadResourceContent>> {
        let relative = uri.strip_prefix("mcp://filesystem/").ok_or_else(|| {
            anyhow::anyhow!("filesystem MCP resource `{uri}` is not a supported URI")
        })?;
        let path = resolve_workspace_file(&self.root, relative)?;
        let bytes = fs::read(&path).with_context(|| {
            format!("failed to read filesystem MCP resource {}", path.display())
        })?;
        let mime_type = Some(mime_type_for_path(&path));
        let content = match String::from_utf8(bytes.clone()) {
            Ok(text) => McpReadResourceContent::Text {
                uri: uri.to_string(),
                mime_type,
                text,
            },
            Err(_) => McpReadResourceContent::Blob {
                uri: uri.to_string(),
                mime_type,
                blob: bytes,
            },
        };
        Ok(vec![content])
    }
}

fn build_live_clients(resources: &LoadedResources, cwd: &Path) -> Vec<Box<dyn McpResourceClient>> {
    configured_live_resource_servers(resources)
        .into_iter()
        .map(|server| {
            Box::new(FilesystemResourceClient {
                name: server.id,
                root: cwd.to_path_buf(),
            }) as Box<dyn McpResourceClient>
        })
        .collect()
}

fn configured_live_resource_servers(resources: &LoadedResources) -> Vec<LiveResourceServer> {
    let mut servers = resources
        .mcp_servers
        .iter()
        .filter(|server| {
            is_live_filesystem_server(server.value.id.as_str(), server.value.target.as_str())
        })
        .map(|server| LiveResourceServer {
            id: server.value.id.clone(),
        })
        .collect::<Vec<_>>();
    servers.extend(
        plugin_mcp_servers(resources)
            .into_iter()
            .filter(|(_, server)| {
                is_live_filesystem_server(server.id.as_str(), server.target.as_str())
            })
            .map(|(_, server)| LiveResourceServer {
                id: server.id.clone(),
            }),
    );
    servers
}

fn is_live_filesystem_server(id: &str, target: &str) -> bool {
    id.trim().eq_ignore_ascii_case("filesystem")
        || matches!(
            target.trim(),
            "builtin:filesystem" | "internal://filesystem" | "puffer-mcp-filesystem"
        )
}

fn collect_workspace_files(root: &Path, current: &Path, output: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_workspace_files(root, &path, output)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let relative = path.strip_prefix(root).unwrap_or(&path);
        output.push(relative.to_string_lossy().replace('\\', "/"));
    }
    Ok(())
}

fn resolve_workspace_file(root: &Path, relative: &str) -> Result<PathBuf> {
    let candidate = root.join(relative);
    let canonical_root = fs::canonicalize(root)?;
    let ancestor = nearest_existing_ancestor(&candidate)
        .ok_or_else(|| anyhow::anyhow!("failed to resolve path {}", candidate.display()))?;
    let canonical_ancestor = fs::canonicalize(&ancestor)?;
    if !canonical_ancestor.starts_with(&canonical_root) {
        anyhow::bail!(
            "path {} resolves through symlink outside workspace {}",
            relative,
            root.display()
        );
    }
    Ok(candidate)
}

fn nearest_existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn mime_type_for_path(path: &Path) -> String {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
    {
        "md" => "text/markdown",
        "json" => "application/json",
        "yaml" | "yml" => "application/yaml",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use puffer_resources::{LoadedItem, McpServerSpec, SourceInfo, SourceKind};

    fn filesystem_resources() -> LoadedResources {
        LoadedResources {
            mcp_servers: vec![LoadedItem {
                value: McpServerSpec {
                    id: "filesystem".to_string(),
                    display_name: "Filesystem".to_string(),
                    transport: "stdio".to_string(),
                    endpoint: String::new(),
                    target: "builtin:filesystem".to_string(),
                    description: "Filesystem server".to_string(),
                },
                source_info: SourceInfo {
                    path: "resources/mcp_servers/filesystem.yaml".into(),
                    kind: SourceKind::Builtin,
                },
            }],
            ..LoadedResources::default()
        }
    }

    #[test]
    fn list_live_mcp_resources_includes_binary_files() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("guide.md"), "# Guide\n").unwrap();
        fs::write(temp.path().join("data.bin"), [0xff, 0x00, 0x01]).unwrap();

        let output =
            list_live_mcp_resources(&filesystem_resources(), temp.path(), Some("filesystem"))
                .unwrap();

        assert!(output.contains("mcp://filesystem/guide.md"));
        assert!(output.contains("mcp://filesystem/data.bin"));
        assert!(output.contains("\"mimeType\": \"application/octet-stream\""));
    }

    #[test]
    fn read_live_mcp_resource_persists_binary_content() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("diagram.png"), [0x89, 0x50, 0x4e, 0x47]).unwrap();

        let output = read_live_mcp_resource(
            &filesystem_resources(),
            temp.path(),
            "filesystem",
            "mcp://filesystem/diagram.png",
        )
        .unwrap();

        assert_eq!(output.contents.len(), 1);
        let content = &output.contents[0];
        assert_eq!(content.mime_type.as_deref(), Some("image/png"));
        let blob_path = PathBuf::from(content.blob_saved_to.clone().unwrap());
        assert!(blob_path.exists());
        assert_eq!(fs::read(blob_path).unwrap(), vec![0x89, 0x50, 0x4e, 0x47]);
    }
}
