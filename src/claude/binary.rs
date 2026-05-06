use std::path::PathBuf;
use std::process::Command;

pub fn find_claude_binary() -> Result<PathBuf, String> {
    if let Some(path) = try_which() {
        return Ok(path);
    }
    if let Some(path) = try_shell_resolve() {
        return Ok(path);
    }
    if let Some(path) = try_candidate_paths() {
        return Ok(path);
    }
    Err(
        "Claude CLI not found. Install it with: npm install -g @anthropic-ai/claude-code\n\
         Searched: PATH, login shell, and common install locations."
            .to_string(),
    )
}

fn try_which() -> Option<PathBuf> {
    let cmd = if cfg!(windows) { "where" } else { "which" };
    let output = Command::new(cmd).arg("claude").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let path = PathBuf::from(&path_str);
    if path.exists() { Some(path) } else { None }
}

fn try_shell_resolve() -> Option<PathBuf> {
    let shells = [
        std::env::var("SHELL").ok(),
        Some("/bin/zsh".to_string()),
        Some("/bin/bash".to_string()),
    ];
    for shell in shells.into_iter().flatten() {
        let output = Command::new(&shell)
            .args(["-lc", "command -v claude"])
            .output()
            .ok()?;
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let path = PathBuf::from(&path_str);
            if path.exists() {
                return Some(path);
            }
        }
    }
    None
}

fn try_candidate_paths() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let candidates = [
        home.join(".local/bin/claude"),
        home.join(".claude/local/claude"),
        home.join(".local/share/mise/shims/claude"),
        home.join(".asdf/shims/claude"),
        home.join(".npm-global/bin/claude"),
        home.join(".npm/bin/claude"),
        home.join(".bun/bin/claude"),
        PathBuf::from("/opt/homebrew/bin/claude"),
        PathBuf::from("/usr/local/bin/claude"),
    ];
    for candidate in &candidates {
        if candidate.exists() {
            return Some(candidate.clone());
        }
    }
    let nvm_dir = home.join(".nvm/versions/node");
    if nvm_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&nvm_dir) {
            for entry in entries.flatten() {
                let claude_path = entry.path().join("bin/claude");
                if claude_path.exists() {
                    return Some(claude_path);
                }
            }
        }
    }
    None
}
