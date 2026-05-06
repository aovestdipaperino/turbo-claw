use turbo_claw::claude::binary::find_claude_binary;

#[test]
fn find_claude_binary_returns_path_or_descriptive_error() {
    match find_claude_binary() {
        Ok(path) => {
            assert!(path.exists(), "Returned path should exist: {path:?}");
            assert!(
                path.to_string_lossy().contains("claude"),
                "Path should contain 'claude': {path:?}"
            );
        }
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("not found") || msg.contains("install"),
                "Error should be descriptive: {msg}"
            );
        }
    }
}
