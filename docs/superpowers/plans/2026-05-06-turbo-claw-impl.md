# Turbo Claw Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Turbo Vision TUI that wraps Claude Code CLI, parsing its NDJSON streaming output into structured UI panels with a flow-based interaction model.

**Architecture:** Each "flow" is a multi-turn Claude conversation — an output window + modal dialogs (prompt, progress, permission). The Claude CLI is spawned per-prompt as `claude -p <prompt> --output-format stream-json`, with NDJSON parsed in a background thread and dispatched to the UI via `mpsc` channels. The UI thread polls the channel during `idle()` via an overlay widget.

**Tech Stack:** Rust (edition 2024), `turbo-vision` 1.2.0, `serde`/`serde_json`, `pulldown-cmark`, `dirs`, `which`

**Reference code:** `vendor/tolaria/src-tauri/src/claude_cli.rs` (Claude CLI spawning), `vendor/tolaria/src-tauri/src/cli_agent_runtime.rs` (NDJSON parsing)

---

### Task 1: Project Setup and Dependencies

**Files:**
- Modify: `Cargo.toml`
- Create: `src/main.rs` (replace hello world)

- [ ] **Step 1: Add dependencies to Cargo.toml**

```toml
[package]
name = "turbo-claw"
version = "0.1.0"
edition = "2024"

[dependencies]
turbo-vision = "1.2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
pulldown-cmark = "0.12"
dirs = "6"
which = "7"
```

- [ ] **Step 2: Write minimal app skeleton in main.rs**

```rust
use turbo_vision::app::Application;
use turbo_vision::core::command::{CM_QUIT, CommandId};
use turbo_vision::core::event::{KB_CTRL_N, KB_F10};
use turbo_vision::core::geometry::Rect;
use turbo_vision::core::menu_data::{Menu, MenuItem};
use turbo_vision::views::menu_bar::{MenuBar, SubMenu};
use turbo_vision::views::status_line::{StatusItem, StatusLine};

/// Custom command IDs for Turbo Claw
const CM_NEW_FLOW: CommandId = 200;

fn main() -> turbo_vision::core::error::Result<()> {
    let mut app = Application::new()?;

    // Menu bar
    let (width, _height) = app.terminal.size();
    let mut menu_bar = MenuBar::new(Rect::new(0, 0, width, 1));
    let flow_menu = Menu::from_items(vec![
        MenuItem::with_shortcut("~N~ew Flow", CM_NEW_FLOW, KB_CTRL_N, "Ctrl+N", 0),
        MenuItem::separator(),
        MenuItem::with_shortcut("E~x~it", CM_QUIT, KB_F10, "F10", 0),
    ]);
    menu_bar.add_submenu(SubMenu::new("~F~low", flow_menu));
    app.set_menu_bar(menu_bar);

    // Status line
    let (_width, height) = app.terminal.size();
    let status_line = StatusLine::new(
        Rect::new(0, height - 1, width, height),
        vec![
            StatusItem::new("~Ctrl+N~ New Flow", KB_CTRL_N, CM_NEW_FLOW),
            StatusItem::new("~F10~ Quit", KB_F10, CM_QUIT),
        ],
    );
    app.set_status_line(status_line);

    app.run();

    Ok(())
}
```

- [ ] **Step 3: Build and verify it compiles**

Run: `cargo build 2>&1`
Expected: Successful build with no errors (warnings OK for now)

- [ ] **Step 4: Run and verify the TUI appears**

Run: `cargo run`
Expected: Empty desktop with "Flow" menu bar and status line. F10 or Alt+X quits. Ctrl+N does nothing yet.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs
git commit -m "feat: minimal turbo-vision app shell with menu bar and status line"
```

---

### Task 2: Claude Binary Discovery

**Files:**
- Create: `src/claude/mod.rs`
- Create: `src/claude/binary.rs`
- Create: `tests/claude_binary.rs`

- [ ] **Step 1: Create the claude module**

```rust
// src/claude/mod.rs
pub mod binary;
```

Update `src/main.rs` to declare the module:
```rust
mod claude;
```
(Add this line near the top of main.rs, before the `fn main()`)

- [ ] **Step 2: Write the failing test**

```rust
// tests/claude_binary.rs
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
```

Also, to make the test binary find our crate, add to `Cargo.toml`:
```toml
[lib]
name = "turbo_claw"
path = "src/lib.rs"

[[bin]]
name = "turbo-claw"
path = "src/main.rs"
```

Create `src/lib.rs`:
```rust
pub mod claude;
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --test claude_binary 2>&1`
Expected: FAIL — `find_claude_binary` doesn't exist yet

- [ ] **Step 4: Implement binary discovery**

```rust
// src/claude/binary.rs
use std::path::PathBuf;
use std::process::Command;

/// Find the `claude` CLI binary on disk.
///
/// Tries three strategies in order:
/// 1. `which claude` (PATH lookup)
/// 2. Login shell resolution (`$SHELL -lc "command -v claude"`)
/// 3. Hardcoded candidate paths
pub fn find_claude_binary() -> Result<PathBuf, String> {
    // Strategy 1: which/where
    if let Some(path) = try_which() {
        return Ok(path);
    }

    // Strategy 2: login shell resolution
    if let Some(path) = try_shell_resolve() {
        return Ok(path);
    }

    // Strategy 3: hardcoded paths
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

    // NVM versions: ~/.nvm/versions/node/*/bin/claude
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
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --test claude_binary 2>&1`
Expected: PASS (finds claude or gives descriptive error)

- [ ] **Step 6: Commit**

```bash
git add src/claude/ src/lib.rs tests/claude_binary.rs Cargo.toml
git commit -m "feat: claude binary discovery with PATH, shell, and hardcoded fallbacks"
```

---

### Task 3: NDJSON Protocol Types and Parsing

**Files:**
- Create: `src/claude/protocol.rs`
- Create: `tests/claude_protocol.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/claude_protocol.rs
use turbo_claw::claude::protocol::{dispatch_event, UiEvent, StreamState};

#[test]
fn parse_system_init() {
    let json: serde_json::Value = serde_json::from_str(r#"{
        "type": "system",
        "subtype": "init",
        "session_id": "abc123",
        "model": "claude-opus-4-6",
        "tools": []
    }"#).unwrap();

    let mut state = StreamState::new();
    let events = dispatch_event(&json, &mut state);

    assert_eq!(events.len(), 1);
    match &events[0] {
        UiEvent::Init { session_id, model } => {
            assert_eq!(session_id, "abc123");
            assert_eq!(model, "claude-opus-4-6");
        }
        other => panic!("Expected Init, got {other:?}"),
    }
    assert_eq!(state.session_id.as_deref(), Some("abc123"));
}

#[test]
fn parse_text_delta() {
    let json: serde_json::Value = serde_json::from_str(r#"{
        "type": "stream_event",
        "event": {
            "type": "content_block_delta",
            "index": 0,
            "delta": {
                "type": "text_delta",
                "text": "Hello "
            }
        }
    }"#).unwrap();

    let mut state = StreamState::new();
    let events = dispatch_event(&json, &mut state);

    assert_eq!(events.len(), 1);
    match &events[0] {
        UiEvent::TextDelta { text } => assert_eq!(text, "Hello "),
        other => panic!("Expected TextDelta, got {other:?}"),
    }
}

#[test]
fn parse_thinking_delta() {
    let json: serde_json::Value = serde_json::from_str(r#"{
        "type": "stream_event",
        "event": {
            "type": "content_block_delta",
            "index": 0,
            "delta": {
                "type": "thinking_delta",
                "thinking": "Let me think..."
            }
        }
    }"#).unwrap();

    let mut state = StreamState::new();
    let events = dispatch_event(&json, &mut state);

    assert_eq!(events.len(), 1);
    match &events[0] {
        UiEvent::ThinkingDelta { text } => assert_eq!(text, "Let me think..."),
        other => panic!("Expected ThinkingDelta, got {other:?}"),
    }
}

#[test]
fn parse_tool_start() {
    let json: serde_json::Value = serde_json::from_str(r#"{
        "type": "stream_event",
        "event": {
            "type": "content_block_start",
            "index": 1,
            "content_block": {
                "type": "tool_use",
                "id": "tool_123",
                "name": "Read",
                "input": {}
            }
        }
    }"#).unwrap();

    let mut state = StreamState::new();
    let events = dispatch_event(&json, &mut state);

    assert_eq!(events.len(), 1);
    match &events[0] {
        UiEvent::ToolStart { tool_name, tool_id, .. } => {
            assert_eq!(tool_name, "Read");
            assert_eq!(tool_id, "tool_123");
        }
        other => panic!("Expected ToolStart, got {other:?}"),
    }
    assert_eq!(state.current_tool_id.as_deref(), Some("tool_123"));
}

#[test]
fn parse_tool_result() {
    let json: serde_json::Value = serde_json::from_str(r#"{
        "type": "tool_result",
        "tool_use_id": "tool_123",
        "content": "File contents here",
        "is_error": false
    }"#).unwrap();

    let mut state = StreamState::new();
    let events = dispatch_event(&json, &mut state);

    assert_eq!(events.len(), 1);
    match &events[0] {
        UiEvent::ToolDone { tool_id, output, is_error } => {
            assert_eq!(tool_id, "tool_123");
            assert_eq!(output.as_deref(), Some("File contents here"));
            assert!(!is_error);
        }
        other => panic!("Expected ToolDone, got {other:?}"),
    }
}

#[test]
fn parse_result() {
    let json: serde_json::Value = serde_json::from_str(r#"{
        "type": "result",
        "subtype": "success",
        "session_id": "abc123",
        "duration_ms": 3200,
        "cost_usd": 0.05,
        "usage": { "input_tokens": 1200, "output_tokens": 800 },
        "result": "The config module..."
    }"#).unwrap();

    let mut state = StreamState::new();
    let events = dispatch_event(&json, &mut state);

    assert_eq!(events.len(), 1);
    match &events[0] {
        UiEvent::Result { session_id, duration_ms, cost_usd, .. } => {
            assert_eq!(session_id, "abc123");
            assert_eq!(*duration_ms, 3200);
            assert!((*cost_usd - 0.05).abs() < f64::EPSILON);
        }
        other => panic!("Expected Result, got {other:?}"),
    }
}

#[test]
fn unknown_type_returns_empty() {
    let json: serde_json::Value = serde_json::from_str(r#"{
        "type": "some_future_type",
        "data": 42
    }"#).unwrap();

    let mut state = StreamState::new();
    let events = dispatch_event(&json, &mut state);
    assert!(events.is_empty(), "Unknown types should be silently skipped");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test claude_protocol 2>&1`
Expected: FAIL — module doesn't exist

- [ ] **Step 3: Implement protocol types and dispatch**

Add `protocol` to `src/claude/mod.rs`:
```rust
pub mod binary;
pub mod protocol;
```

```rust
// src/claude/protocol.rs
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum UiEvent {
    Init { session_id: String, model: String },
    TextDelta { text: String },
    ThinkingDelta { text: String },
    ToolStart { tool_name: String, tool_id: String, input: Option<String> },
    ToolProgress { tool_id: String, content: String },
    ToolDone { tool_id: String, output: Option<String>, is_error: bool },
    Result {
        session_id: String,
        duration_ms: u64,
        cost_usd: f64,
        input_tokens: u64,
        output_tokens: u64,
    },
    Error { message: String },
    StderrLine(String),
    ProcessExited(i32),
}

pub struct StreamState {
    pub session_id: Option<String>,
    pub tool_inputs: HashMap<String, String>,
    pub current_tool_id: Option<String>,
    pub emitted_text: bool,
}

impl StreamState {
    pub fn new() -> Self {
        Self {
            session_id: None,
            tool_inputs: HashMap::new(),
            current_tool_id: None,
            emitted_text: false,
        }
    }
}

/// Parse a single NDJSON line (already deserialized to Value) into UiEvents.
/// Returns an empty vec for unknown/unhandled types (forward-compatible).
pub fn dispatch_event(json: &serde_json::Value, state: &mut StreamState) -> Vec<UiEvent> {
    let msg_type = match json["type"].as_str() {
        Some(t) => t,
        None => return vec![],
    };

    match msg_type {
        "system" => dispatch_system(json, state),
        "stream_event" => dispatch_stream_event(json, state),
        "tool_progress" => dispatch_tool_progress(json, state),
        "tool_result" => dispatch_tool_result(json, state),
        "result" => dispatch_result(json, state),
        _ => vec![],
    }
}

fn dispatch_system(json: &serde_json::Value, state: &mut StreamState) -> Vec<UiEvent> {
    let subtype = json["subtype"].as_str().unwrap_or("");
    if subtype != "init" {
        return vec![];
    }
    let session_id = json["session_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let model = json["model"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    state.session_id = Some(session_id.clone());
    vec![UiEvent::Init { session_id, model }]
}

fn dispatch_stream_event(json: &serde_json::Value, state: &mut StreamState) -> Vec<UiEvent> {
    let event = &json["event"];
    let event_type = event["type"].as_str().unwrap_or("");

    match event_type {
        "content_block_delta" => {
            let delta = &event["delta"];
            let delta_type = delta["type"].as_str().unwrap_or("");
            match delta_type {
                "text_delta" => {
                    let text = delta["text"].as_str().unwrap_or("").to_string();
                    state.emitted_text = true;
                    vec![UiEvent::TextDelta { text }]
                }
                "thinking_delta" => {
                    let text = delta["thinking"].as_str().unwrap_or("").to_string();
                    vec![UiEvent::ThinkingDelta { text }]
                }
                "input_json_delta" => {
                    // Accumulate tool input JSON fragments
                    let partial = delta["partial_json"].as_str().unwrap_or("");
                    if let Some(ref tool_id) = state.current_tool_id {
                        state
                            .tool_inputs
                            .entry(tool_id.clone())
                            .or_default()
                            .push_str(partial);
                    }
                    vec![]
                }
                _ => vec![],
            }
        }
        "content_block_start" => {
            let block = &event["content_block"];
            let block_type = block["type"].as_str().unwrap_or("");
            if block_type == "tool_use" {
                let tool_id = block["id"].as_str().unwrap_or("").to_string();
                let tool_name = block["name"].as_str().unwrap_or("").to_string();
                let input = if block["input"].is_object() && !block["input"].as_object().unwrap().is_empty() {
                    Some(block["input"].to_string())
                } else {
                    None
                };
                state.current_tool_id = Some(tool_id.clone());
                vec![UiEvent::ToolStart { tool_name, tool_id, input }]
            } else {
                vec![]
            }
        }
        "content_block_stop" => {
            state.current_tool_id = None;
            vec![]
        }
        _ => vec![],
    }
}

fn dispatch_tool_progress(json: &serde_json::Value, state: &mut StreamState) -> Vec<UiEvent> {
    let tool_id = json["tool_use_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let content = json["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // If we haven't seen a ToolStart for this tool yet, emit one
    if state.current_tool_id.as_deref() != Some(&tool_id) {
        let tool_name = json["tool_name"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        state.current_tool_id = Some(tool_id.clone());
        return vec![
            UiEvent::ToolStart { tool_name, tool_id: tool_id.clone(), input: None },
            UiEvent::ToolProgress { tool_id, content },
        ];
    }

    vec![UiEvent::ToolProgress { tool_id, content }]
}

fn dispatch_tool_result(json: &serde_json::Value, _state: &mut StreamState) -> Vec<UiEvent> {
    let tool_id = json["tool_use_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let output = json["content"].as_str().map(String::from);
    let is_error = json["is_error"].as_bool().unwrap_or(false);
    vec![UiEvent::ToolDone { tool_id, output, is_error }]
}

fn dispatch_result(json: &serde_json::Value, state: &mut StreamState) -> Vec<UiEvent> {
    let session_id = json["session_id"]
        .as_str()
        .map(String::from)
        .or_else(|| state.session_id.clone())
        .unwrap_or_default();
    let duration_ms = json["duration_ms"].as_u64().unwrap_or(0);
    let cost_usd = json["cost_usd"].as_f64().unwrap_or(0.0);
    let usage = &json["usage"];
    let input_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
    let output_tokens = usage["output_tokens"].as_u64().unwrap_or(0);

    state.session_id = Some(session_id.clone());

    vec![UiEvent::Result {
        session_id,
        duration_ms,
        cost_usd,
        input_tokens,
        output_tokens,
    }]
}
```

Also update `src/lib.rs`:
```rust
pub mod claude;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test claude_protocol 2>&1`
Expected: All 7 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/claude/protocol.rs src/claude/mod.rs tests/claude_protocol.rs
git commit -m "feat: NDJSON protocol types and dispatch for Claude streaming output"
```

---

### Task 4: Claude Session (Process Spawning and I/O)

**Files:**
- Create: `src/claude/session.rs`
- Create: `src/bridge.rs`
- Modify: `src/claude/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create bridge module with channel types**

```rust
// src/bridge.rs
use crate::claude::protocol::UiEvent;
use std::sync::mpsc;

/// Create a new bridge channel pair for communicating between
/// the Claude reader thread and the UI thread.
pub fn new_bridge() -> (mpsc::Sender<UiEvent>, mpsc::Receiver<UiEvent>) {
    mpsc::channel()
}
```

- [ ] **Step 2: Implement the Session struct**

Add `session` to `src/claude/mod.rs`:
```rust
pub mod binary;
pub mod protocol;
pub mod session;
```

```rust
// src/claude/session.rs
use crate::bridge;
use crate::claude::binary::find_claude_binary;
use crate::claude::protocol::{dispatch_event, StreamState, UiEvent};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;

pub struct Session {
    child: Child,
    pub receiver: mpsc::Receiver<UiEvent>,
    _stdout_thread: thread::JoinHandle<()>,
    _stderr_thread: thread::JoinHandle<()>,
}

impl Session {
    /// Spawn a new Claude CLI session.
    ///
    /// - `prompt`: the user's prompt text
    /// - `resume_session_id`: if Some, resumes an existing session
    /// - `allowed_tools`: extra tools pre-approved via Always Allow
    pub fn spawn(
        prompt: &str,
        resume_session_id: Option<&str>,
        allowed_tools: &HashSet<String>,
    ) -> Result<Self, String> {
        let claude_path = find_claude_binary()?;

        let mut args: Vec<String> = vec![
            "-p".to_string(),
            prompt.to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            "--include-partial-messages".to_string(),
            "--permission-mode".to_string(),
            "acceptEdits".to_string(),
            "--tools".to_string(),
            "Read,Edit,MultiEdit,Write,Glob,Grep,LS".to_string(),
        ];

        if let Some(session_id) = resume_session_id {
            args.push("--resume".to_string());
            args.push(session_id.to_string());
        }

        if !allowed_tools.is_empty() {
            let tools_csv: String = allowed_tools
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(",");
            args.push("--allowedTools".to_string());
            args.push(tools_csv);
        }

        let mut child = Command::new(&claude_path)
            .args(&args)
            .env_remove("CLAUDECODE") // prevent nested session guard
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn claude: {e}"))?;

        let stdout = child.stdout.take().ok_or("No stdout handle")?;
        let stderr = child.stderr.take().ok_or("No stderr handle")?;

        let (tx, receiver) = bridge::new_bridge();

        // Stdout reader thread: parse NDJSON
        let tx_stdout = tx.clone();
        let stdout_thread = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            let mut state = StreamState::new();
            for line in reader.lines() {
                let line = match line {
                    Ok(l) => l,
                    Err(_) => break,
                };
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let json: serde_json::Value = match serde_json::from_str(trimmed) {
                    Ok(v) => v,
                    Err(_) => continue, // skip non-JSON lines
                };
                let events = dispatch_event(&json, &mut state);
                for event in events {
                    if tx_stdout.send(event).is_err() {
                        return; // receiver dropped
                    }
                }
            }
        });

        // Stderr reader thread
        let tx_stderr = tx;
        let stderr_thread = thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(l) if !l.trim().is_empty() => {
                        if tx_stderr.send(UiEvent::StderrLine(l)).is_err() {
                            return;
                        }
                    }
                    _ => {}
                }
            }
        });

        Ok(Self {
            child,
            receiver,
            _stdout_thread: stdout_thread,
            _stderr_thread: stderr_thread,
        })
    }

    /// Write a string to the child's stdin (for permission responses).
    pub fn write_stdin(&mut self, data: &str) -> Result<(), String> {
        if let Some(ref mut stdin) = self.child.stdin {
            stdin
                .write_all(data.as_bytes())
                .map_err(|e| format!("Failed to write to stdin: {e}"))?;
            stdin
                .flush()
                .map_err(|e| format!("Failed to flush stdin: {e}"))?;
            Ok(())
        } else {
            Err("No stdin handle".to_string())
        }
    }

    /// Kill the child process.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }

    /// Check if the child process has exited (non-blocking).
    pub fn try_wait(&mut self) -> Option<i32> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.code().unwrap_or(-1)),
            _ => None,
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.kill();
    }
}
```

Update `src/lib.rs`:
```rust
pub mod claude;
pub mod bridge;
```

- [ ] **Step 3: Build to verify it compiles**

Run: `cargo build 2>&1`
Expected: Successful build

- [ ] **Step 4: Commit**

```bash
git add src/claude/session.rs src/claude/mod.rs src/bridge.rs src/lib.rs
git commit -m "feat: Claude session spawning with stdout/stderr reader threads"
```

---

### Task 5: Output View (Scrollable Rendered Output)

**Files:**
- Create: `src/ui/mod.rs`
- Create: `src/ui/output_view.rs`

- [ ] **Step 1: Create the ui module**

```rust
// src/ui/mod.rs
pub mod output_view;
```

Update `src/lib.rs`:
```rust
pub mod claude;
pub mod bridge;
pub mod ui;
```

- [ ] **Step 2: Implement OutputView**

The output view wraps a `Window` + `TerminalWidget` (same pattern as `LogWindow`), appending colored lines for each content type.

```rust
// src/ui/output_view.rs
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use turbo_vision::core::command::CommandId;
use turbo_vision::core::event::{Event, EventType, KB_ENTER};
use turbo_vision::core::geometry::Rect;
use turbo_vision::core::palette::{Attr, TvColor};
use turbo_vision::core::state::StateFlags;
use turbo_vision::terminal::Terminal;
use turbo_vision::views::terminal_widget::TerminalWidget;
use turbo_vision::views::view::View;
use turbo_vision::views::window::Window;

use crate::claude::protocol::UiEvent;

/// Command emitted when user presses Enter in the output view.
pub const CM_OPEN_PROMPT: CommandId = 201;

/// Shared wrapper so TerminalWidget can be a View child of the Window.
struct SharedWidget(Rc<RefCell<TerminalWidget>>);

impl View for SharedWidget {
    fn bounds(&self) -> Rect { self.0.borrow().bounds() }
    fn set_bounds(&mut self, bounds: Rect) { self.0.borrow_mut().set_bounds(bounds); }
    fn draw(&mut self, terminal: &mut Terminal) { self.0.borrow_mut().draw(terminal); }
    fn handle_event(&mut self, event: &mut Event) { self.0.borrow_mut().handle_event(event); }
    fn can_focus(&self) -> bool { true }
    fn state(&self) -> StateFlags { self.0.borrow().state() }
    fn set_state(&mut self, state: StateFlags) { self.0.borrow_mut().set_state(state); }
    fn get_palette(&self) -> Option<turbo_vision::core::palette::Palette> {
        self.0.borrow().get_palette()
    }
}

/// Colors for different content types
const ATTR_TEXT: Attr = Attr::new(TvColor::White, TvColor::Blue);
const ATTR_THINKING: Attr = Attr::new(TvColor::DarkGray, TvColor::Blue);
const ATTR_TOOL_FRAME: Attr = Attr::new(TvColor::LightCyan, TvColor::Blue);
const ATTR_TOOL_OK: Attr = Attr::new(TvColor::LightGreen, TvColor::Blue);
const ATTR_TOOL_ERR: Attr = Attr::new(TvColor::LightRed, TvColor::Blue);
const ATTR_SEPARATOR: Attr = Attr::new(TvColor::DarkGray, TvColor::Blue);
const ATTR_BOLD: Attr = Attr::new(TvColor::BrightWhite, TvColor::Blue);
const ATTR_CODE: Attr = Attr::new(TvColor::LightGreen, TvColor::Blue);

pub struct OutputView {
    window: Window,
    widget: Rc<RefCell<TerminalWidget>>,
    /// Accumulates streaming text between flushes
    text_buffer: String,
    /// Accumulates streaming thinking text
    thinking_buffer: String,
    /// Whether we are currently in a thinking block
    in_thinking: bool,
}

impl OutputView {
    pub fn new(bounds: Rect, title: &str) -> Self {
        let mut window = Window::new(bounds, title);

        let interior = window.interior_bounds();
        let widget = Rc::new(RefCell::new(
            TerminalWidget::new(interior).with_scrollbar(),
        ));

        window.add(Box::new(SharedWidget(Rc::clone(&widget))));

        Self {
            window,
            widget,
            text_buffer: String::new(),
            thinking_buffer: String::new(),
            in_thinking: false,
        }
    }

    /// Process a UiEvent and render it into the output.
    pub fn handle_ui_event(&mut self, event: &UiEvent) {
        match event {
            UiEvent::Init { model, .. } => {
                self.window.set_title(&format!("Flow — {model}"));
            }
            UiEvent::TextDelta { text } => {
                self.flush_thinking();
                self.text_buffer.push_str(text);
                // Flush complete lines
                while let Some(pos) = self.text_buffer.find('\n') {
                    let line = self.text_buffer[..pos].to_string();
                    self.append_markdown_line(&line);
                    self.text_buffer = self.text_buffer[pos + 1..].to_string();
                }
            }
            UiEvent::ThinkingDelta { text } => {
                self.flush_text();
                self.in_thinking = true;
                self.thinking_buffer.push_str(text);
                // Flush complete lines
                while let Some(pos) = self.thinking_buffer.find('\n') {
                    let line = self.thinking_buffer[..pos].to_string();
                    let display = format!("[thinking] {line}");
                    self.widget.borrow_mut().append_line_colored(display, ATTR_THINKING);
                    self.thinking_buffer = self.thinking_buffer[pos + 1..].to_string();
                }
            }
            UiEvent::ToolStart { tool_name, tool_id: _, input } => {
                self.flush_text();
                self.flush_thinking();
                let header = format!("┌─ {tool_name} ─────────────────────────");
                self.widget.borrow_mut().append_line_colored(header, ATTR_TOOL_FRAME);
                if let Some(input_str) = input {
                    // Truncate long JSON input
                    let display = if input_str.len() > 120 {
                        format!("│ {}...", &input_str[..117])
                    } else {
                        format!("│ {input_str}")
                    };
                    self.widget.borrow_mut().append_line_colored(display, ATTR_TOOL_FRAME);
                }
            }
            UiEvent::ToolProgress { content, .. } => {
                // Show live tool output lines
                for line in content.lines() {
                    let display = format!("│ {line}");
                    self.widget.borrow_mut().append_line_colored(display, ATTR_TOOL_FRAME);
                }
            }
            UiEvent::ToolDone { is_error, output, .. } => {
                let (attr, marker) = if *is_error {
                    (ATTR_TOOL_ERR, "✗ error")
                } else {
                    (ATTR_TOOL_OK, "✓ done")
                };
                if let Some(out) = output {
                    // Show first line of output
                    let first_line = out.lines().next().unwrap_or("");
                    let display = if first_line.len() > 100 {
                        format!("│ {}...", &first_line[..97])
                    } else {
                        format!("│ {first_line}")
                    };
                    self.widget.borrow_mut().append_line_colored(display, attr);
                }
                let footer = format!("└─ {marker} ─────────────────────────");
                self.widget.borrow_mut().append_line_colored(footer, attr);
            }
            UiEvent::Result { duration_ms, cost_usd, input_tokens, output_tokens, .. } => {
                self.flush_text();
                self.flush_thinking();
                let secs = *duration_ms as f64 / 1000.0;
                let total_tokens = input_tokens + output_tokens;
                let sep = format!(
                    "── Done ({secs:.1}s, ${cost_usd:.4}, {total_tokens} tokens) ──"
                );
                self.widget.borrow_mut().append_line_colored(sep, ATTR_SEPARATOR);
            }
            UiEvent::Error { message } => {
                self.widget.borrow_mut().append_line_colored(
                    format!("ERROR: {message}"),
                    ATTR_TOOL_ERR,
                );
            }
            UiEvent::StderrLine(line) => {
                self.widget.borrow_mut().append_line_colored(
                    format!("stderr: {line}"),
                    ATTR_THINKING,
                );
            }
            UiEvent::ProcessExited(code) => {
                self.widget.borrow_mut().append_line_colored(
                    format!("Process exited with code {code}"),
                    ATTR_SEPARATOR,
                );
            }
        }
    }

    fn flush_text(&mut self) {
        if !self.text_buffer.is_empty() {
            let remaining = std::mem::take(&mut self.text_buffer);
            self.append_markdown_line(&remaining);
        }
    }

    fn flush_thinking(&mut self) {
        if !self.thinking_buffer.is_empty() {
            let remaining = std::mem::take(&mut self.thinking_buffer);
            let display = format!("[thinking] {remaining}");
            self.widget.borrow_mut().append_line_colored(display, ATTR_THINKING);
        }
        self.in_thinking = false;
    }

    /// Render a line of markdown as attributed text.
    /// Phase 1: basic bold/code detection. Full pulldown-cmark in Task 9.
    fn append_markdown_line(&mut self, line: &str) {
        // Simple heuristics for now:
        if line.starts_with("```") {
            self.widget.borrow_mut().append_line_colored(line.to_string(), ATTR_CODE);
        } else if line.starts_with("# ") || line.starts_with("## ") || line.starts_with("### ") {
            self.widget.borrow_mut().append_line_colored(line.to_string(), ATTR_BOLD);
        } else if line.contains("**") || line.contains('`') {
            // Mixed formatting — just use normal text for now
            self.widget.borrow_mut().append_line_colored(line.to_string(), ATTR_TEXT);
        } else {
            self.widget.borrow_mut().append_line_colored(line.to_string(), ATTR_TEXT);
        }
    }
}

impl View for OutputView {
    fn bounds(&self) -> Rect { self.window.bounds() }
    fn set_bounds(&mut self, bounds: Rect) { self.window.set_bounds(bounds); }
    fn draw(&mut self, terminal: &mut Terminal) { self.window.draw(terminal); }

    fn handle_event(&mut self, event: &mut Event) {
        // Intercept Enter key to emit CM_OPEN_PROMPT
        if event.what == EventType::Keyboard && event.key_code == KB_ENTER {
            *event = Event::command(CM_OPEN_PROMPT);
            return;
        }
        self.window.handle_event(event);
    }

    fn can_focus(&self) -> bool { true }
    fn state(&self) -> StateFlags { self.window.state() }
    fn set_state(&mut self, state: StateFlags) { self.window.set_state(state); }
    fn options(&self) -> u16 { self.window.options() }
    fn set_options(&mut self, options: u16) { self.window.set_options(options); }
    fn get_palette(&self) -> Option<turbo_vision::core::palette::Palette> {
        self.window.get_palette()
    }
    fn get_end_state(&self) -> CommandId { self.window.get_end_state() }
    fn set_end_state(&mut self, cmd: CommandId) { self.window.set_end_state(cmd); }
}
```

- [ ] **Step 3: Build to verify it compiles**

Run: `cargo build 2>&1`
Expected: Successful build

- [ ] **Step 4: Commit**

```bash
git add src/ui/
git commit -m "feat: OutputView with colored rendering for text, thinking, and tool events"
```

---

### Task 6: Prompt Dialog

**Files:**
- Create: `src/ui/prompt_dialog.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Implement the prompt dialog**

Add to `src/ui/mod.rs`:
```rust
pub mod output_view;
pub mod prompt_dialog;
```

```rust
// src/ui/prompt_dialog.rs
use turbo_vision::core::command::{CM_CANCEL, CM_OK, CommandId};
use turbo_vision::core::event::Event;
use turbo_vision::core::geometry::Rect;
use turbo_vision::core::state::StateFlags;
use turbo_vision::terminal::Terminal;
use turbo_vision::views::button::Button;
use turbo_vision::views::dialog::Dialog;
use turbo_vision::views::memo::Memo;
use turbo_vision::views::view::{View, ViewId};

pub struct PromptDialog {
    dialog: Dialog,
    memo_id: ViewId,
}

impl PromptDialog {
    /// Create a new modal prompt dialog centered on screen.
    pub fn new(screen_width: u16, screen_height: u16) -> Box<Self> {
        let dialog_w: i16 = 60.min(screen_width as i16 - 4);
        let dialog_h: i16 = 14.min(screen_height as i16 - 4);
        let x = ((screen_width as i16) - dialog_w) / 2;
        let y = ((screen_height as i16) - dialog_h) / 2;

        let bounds = Rect::new(x, y, x + dialog_w, y + dialog_h);
        let mut dialog = Dialog::new(bounds, "Enter Prompt");

        // Memo (multi-line text input) — interior area minus buttons row
        let memo_bounds = Rect::new(2, 1, dialog_w - 2, dialog_h - 4);
        let memo = Memo::new(memo_bounds).with_scrollbars(true);
        let memo_id = dialog.add(Box::new(memo));

        // OK button
        let btn_y = dialog_h - 3;
        let ok_x = dialog_w / 2 - 12;
        let ok_btn = Button::new(
            Rect::new(ok_x, btn_y, ok_x + 10, btn_y + 2),
            "~O~K",
            CM_OK,
            true,
        );
        dialog.add(Box::new(ok_btn));

        // Cancel button
        let cancel_x = ok_x + 12;
        let cancel_btn = Button::new(
            Rect::new(cancel_x, btn_y, cancel_x + 12, btn_y + 2),
            "Cancel",
            CM_CANCEL,
            false,
        );
        dialog.add(Box::new(cancel_btn));

        dialog.set_initial_focus();

        // Set modal flag
        use turbo_vision::core::state::SF_MODAL;
        let current_state = dialog.state();
        dialog.set_state(current_state | SF_MODAL);

        Box::new(Self { dialog, memo_id })
    }

    /// Get the text entered in the memo field.
    pub fn get_text(&self) -> String {
        if let Some(view) = self.dialog.child_by_id(self.memo_id) {
            // Downcast to Memo to get text
            // Since we can't downcast trait objects, we use the Memo's data method
            // For now, we need to use a workaround via the View trait
            // The Memo stores text internally; we access it through the public API
            // This requires Memo to expose get_text() — it does (checked in source)
            unsafe {
                let view_ptr = view as *const dyn View as *const Memo;
                (*view_ptr).get_text()
            }
        } else {
            String::new()
        }
    }
}

impl View for PromptDialog {
    fn bounds(&self) -> Rect { self.dialog.bounds() }
    fn set_bounds(&mut self, bounds: Rect) { self.dialog.set_bounds(bounds); }
    fn draw(&mut self, terminal: &mut Terminal) { self.dialog.draw(terminal); }
    fn handle_event(&mut self, event: &mut Event) { self.dialog.handle_event(event); }
    fn can_focus(&self) -> bool { true }
    fn state(&self) -> StateFlags { self.dialog.state() }
    fn set_state(&mut self, state: StateFlags) { self.dialog.set_state(state); }
    fn options(&self) -> u16 { self.dialog.options() }
    fn set_options(&mut self, options: u16) { self.dialog.set_options(options); }
    fn get_palette(&self) -> Option<turbo_vision::core::palette::Palette> {
        self.dialog.get_palette()
    }
    fn get_end_state(&self) -> CommandId { self.dialog.get_end_state() }
    fn set_end_state(&mut self, cmd: CommandId) { self.dialog.set_end_state(cmd); }
}
```

- [ ] **Step 2: Build to verify it compiles**

Run: `cargo build 2>&1`
Expected: Successful build

- [ ] **Step 3: Commit**

```bash
git add src/ui/prompt_dialog.rs src/ui/mod.rs
git commit -m "feat: modal prompt dialog with scrollable memo input"
```

---

### Task 7: Progress Dialog

**Files:**
- Create: `src/ui/progress_dialog.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Implement the progress dialog**

Add to `src/ui/mod.rs`:
```rust
pub mod output_view;
pub mod prompt_dialog;
pub mod progress_dialog;
```

```rust
// src/ui/progress_dialog.rs
use turbo_vision::core::command::{CM_CANCEL, CommandId};
use turbo_vision::core::event::Event;
use turbo_vision::core::geometry::Rect;
use turbo_vision::core::state::StateFlags;
use turbo_vision::terminal::Terminal;
use turbo_vision::views::button::Button;
use turbo_vision::views::dialog::Dialog;
use turbo_vision::views::static_text::StaticText;
use turbo_vision::views::view::{View, ViewId};

pub struct ProgressDialog {
    dialog: Dialog,
    status_id: ViewId,
    cost_id: ViewId,
}

impl ProgressDialog {
    /// Create a new modal progress dialog centered on screen.
    pub fn new(screen_width: u16, screen_height: u16) -> Box<Self> {
        let dialog_w: i16 = 40.min(screen_width as i16 - 4);
        let dialog_h: i16 = 9.min(screen_height as i16 - 4);
        let x = ((screen_width as i16) - dialog_w) / 2;
        let y = ((screen_height as i16) - dialog_h) / 2;

        let bounds = Rect::new(x, y, x + dialog_w, y + dialog_h);
        let mut dialog = Dialog::new(bounds, "Running");

        // Status text
        let status = StaticText::new_centered(
            Rect::new(2, 2, dialog_w - 2, 3),
            "Starting...",
        );
        let status_id = dialog.add(Box::new(status));

        // Cost/tokens text
        let cost = StaticText::new_centered(
            Rect::new(2, 3, dialog_w - 2, 4),
            "",
        );
        let cost_id = dialog.add(Box::new(cost));

        // Cancel button
        let btn_x = dialog_w / 2 - 6;
        let btn_y = dialog_h - 3;
        let cancel_btn = Button::new(
            Rect::new(btn_x, btn_y, btn_x + 12, btn_y + 2),
            "Cancel",
            CM_CANCEL,
            true,
        );
        dialog.add(Box::new(cancel_btn));

        dialog.set_initial_focus();

        use turbo_vision::core::state::SF_MODAL;
        let current_state = dialog.state();
        dialog.set_state(current_state | SF_MODAL);

        Box::new(Self { dialog, status_id, cost_id })
    }

    /// Update the status message displayed in the dialog.
    pub fn set_status(&mut self, status: &str) {
        if let Some(view) = self.dialog.child_by_id_mut(self.status_id) {
            unsafe {
                let ptr = view as *mut dyn View as *mut StaticText;
                *ptr = StaticText::new_centered((*ptr).bounds(), status);
            }
        }
    }

    /// Update the cost/tokens display.
    pub fn set_cost(&mut self, cost_usd: f64, tokens: u64) {
        let text = format!("${cost_usd:.4} | {tokens} tokens");
        if let Some(view) = self.dialog.child_by_id_mut(self.cost_id) {
            unsafe {
                let ptr = view as *mut dyn View as *mut StaticText;
                *ptr = StaticText::new_centered((*ptr).bounds(), &text);
            }
        }
    }
}

impl View for ProgressDialog {
    fn bounds(&self) -> Rect { self.dialog.bounds() }
    fn set_bounds(&mut self, bounds: Rect) { self.dialog.set_bounds(bounds); }
    fn draw(&mut self, terminal: &mut Terminal) { self.dialog.draw(terminal); }
    fn handle_event(&mut self, event: &mut Event) { self.dialog.handle_event(event); }
    fn can_focus(&self) -> bool { true }
    fn state(&self) -> StateFlags { self.dialog.state() }
    fn set_state(&mut self, state: StateFlags) { self.dialog.set_state(state); }
    fn options(&self) -> u16 { self.dialog.options() }
    fn set_options(&mut self, options: u16) { self.dialog.set_options(options); }
    fn get_palette(&self) -> Option<turbo_vision::core::palette::Palette> {
        self.dialog.get_palette()
    }
    fn get_end_state(&self) -> CommandId { self.dialog.get_end_state() }
    fn set_end_state(&mut self, cmd: CommandId) { self.dialog.set_end_state(cmd); }
}
```

- [ ] **Step 2: Build to verify it compiles**

Run: `cargo build 2>&1`
Expected: Successful build

- [ ] **Step 3: Commit**

```bash
git add src/ui/progress_dialog.rs src/ui/mod.rs
git commit -m "feat: modal progress dialog with status, cost display, and cancel"
```

---

### Task 8: Flow State Machine and Main Loop Integration

**Files:**
- Create: `src/ui/flow.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/main.rs`

This is the core task that wires everything together.

- [ ] **Step 1: Create the Flow module**

Add to `src/ui/mod.rs`:
```rust
pub mod output_view;
pub mod prompt_dialog;
pub mod progress_dialog;
pub mod flow;
```

```rust
// src/ui/flow.rs
use std::collections::HashSet;

use crate::claude::protocol::UiEvent;
use crate::claude::session::Session;
use crate::ui::output_view::OutputView;

/// Flow state machine
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlowState {
    /// Just created, waiting for first prompt
    Idle,
    /// Claude is running
    Running,
    /// Claude finished, output window has focus
    Done,
}

/// A Flow is a multi-turn conversation session.
/// It owns the output view, session state, and permission allow-list.
pub struct Flow {
    pub output_view: OutputView,
    pub session_id: Option<String>,
    pub state: FlowState,
    pub allowed_tools: HashSet<String>,
    pub session: Option<Session>,
    /// Accumulated cost across all turns
    pub total_cost: f64,
    pub total_tokens: u64,
}

impl Flow {
    pub fn new(output_view: OutputView) -> Self {
        Self {
            output_view,
            session_id: None,
            state: FlowState::Idle,
            allowed_tools: HashSet::new(),
            session: None,
            total_cost: 0.0,
            total_tokens: 0,
        }
    }

    /// Start a new Claude invocation with the given prompt.
    pub fn start_prompt(&mut self, prompt: &str) -> Result<(), String> {
        let session = Session::spawn(
            prompt,
            self.session_id.as_deref(),
            &self.allowed_tools,
        )?;
        self.session = Some(session);
        self.state = FlowState::Running;
        Ok(())
    }

    /// Drain pending events from the session and route them to the output view.
    /// Returns true if the session completed (Result or ProcessExited received).
    pub fn poll(&mut self) -> bool {
        let session = match self.session.as_mut() {
            Some(s) => s,
            None => return false,
        };

        let mut completed = false;

        while let Ok(event) = session.receiver.try_recv() {
            match &event {
                UiEvent::Init { session_id, .. } => {
                    self.session_id = Some(session_id.clone());
                }
                UiEvent::Result { cost_usd, input_tokens, output_tokens, .. } => {
                    self.total_cost += cost_usd;
                    self.total_tokens += input_tokens + output_tokens;
                    completed = true;
                }
                UiEvent::ProcessExited(_) => {
                    completed = true;
                }
                _ => {}
            }
            self.output_view.handle_ui_event(&event);
        }

        // Also check if the child process exited
        if !completed {
            if let Some(code) = session.try_wait() {
                self.output_view
                    .handle_ui_event(&UiEvent::ProcessExited(code));
                completed = true;
            }
        }

        if completed {
            self.state = FlowState::Done;
            self.session = None;
        }

        completed
    }

    /// Kill the running session.
    pub fn cancel(&mut self) {
        if let Some(ref mut session) = self.session {
            session.kill();
        }
        self.session = None;
        self.state = FlowState::Done;
    }
}
```

- [ ] **Step 2: Rewrite main.rs to integrate the flow lifecycle**

```rust
// src/main.rs
mod claude;
mod bridge;
mod ui;

use turbo_vision::app::Application;
use turbo_vision::core::command::{CM_CANCEL, CM_OK, CM_QUIT, CommandId};
use turbo_vision::core::event::{Event, EventType, KB_CTRL_N, KB_F10};
use turbo_vision::core::geometry::Rect;
use turbo_vision::core::menu_data::{Menu, MenuItem};
use turbo_vision::views::menu_bar::{MenuBar, SubMenu};
use turbo_vision::views::status_line::{StatusItem, StatusLine};
use turbo_vision::views::{IdleView, View};

use crate::ui::flow::{Flow, FlowState};
use crate::ui::output_view::{OutputView, CM_OPEN_PROMPT};
use crate::ui::prompt_dialog::PromptDialog;
use crate::ui::progress_dialog::ProgressDialog;

const CM_NEW_FLOW: CommandId = 200;

/// Overlay widget that polls all active flows during idle().
/// This ensures NDJSON events are drained even during modal dialogs.
struct FlowPoller {
    /// Shared pointer to the active flow (if any).
    /// We use raw pointer because the Flow's OutputView is owned by the desktop.
    /// The poller only needs to call poll() on the session.
    flow: Option<*mut Flow>,
}

// SAFETY: FlowPoller is only used on the main thread.
unsafe impl Send for FlowPoller {}
unsafe impl Sync for FlowPoller {}

impl View for FlowPoller {
    fn bounds(&self) -> Rect { Rect::new(0, 0, 0, 0) }
    fn set_bounds(&mut self, _: Rect) {}
    fn draw(&mut self, _: &mut turbo_vision::terminal::Terminal) {}
    fn handle_event(&mut self, _: &mut Event) {}
    fn update_cursor(&self, _: &mut turbo_vision::terminal::Terminal) {}
    fn get_palette(&self) -> Option<turbo_vision::core::palette::Palette> { None }
}

impl IdleView for FlowPoller {
    fn idle(&mut self) {
        if let Some(flow_ptr) = self.flow {
            // SAFETY: Flow lives as long as the application
            let flow = unsafe { &mut *flow_ptr };
            flow.poll();
        }
    }
}

fn main() -> turbo_vision::core::error::Result<()> {
    let mut app = Application::new()?;
    let (width, height) = app.terminal.size();

    // Menu bar
    let mut menu_bar = MenuBar::new(Rect::new(0, 0, width, 1));
    let flow_menu = Menu::from_items(vec![
        MenuItem::with_shortcut("~N~ew Flow", CM_NEW_FLOW, KB_CTRL_N, "Ctrl+N", 0),
        MenuItem::separator(),
        MenuItem::with_shortcut("E~x~it", CM_QUIT, KB_F10, "F10", 0),
    ]);
    menu_bar.add_submenu(SubMenu::new("~F~low", flow_menu));
    app.set_menu_bar(menu_bar);

    // Status line
    let status_line = StatusLine::new(
        Rect::new(0, height - 1, width, height),
        vec![
            StatusItem::new("~Ctrl+N~ New Flow", KB_CTRL_N, CM_NEW_FLOW),
            StatusItem::new("~F10~ Quit", KB_F10, CM_QUIT),
        ],
    );
    app.set_status_line(status_line);

    // Flow state — we keep at most one active flow for now
    let mut active_flow: Option<Box<Flow>> = None;

    app.running = true;

    // Main event loop (custom, not app.run(), because we need flow management)
    loop {
        if !app.running {
            break;
        }

        // Draw
        app.draw();
        let _ = app.terminal.flush();

        // Poll flow if running
        if let Some(ref mut flow) = active_flow {
            flow.poll();
        }

        // Get event (20ms timeout for responsive polling)
        let event = app.get_event();
        if let Some(mut event) = event {
            // Handle our custom commands before passing to the app
            if event.what == EventType::Command {
                match event.command {
                    CM_NEW_FLOW => {
                        // Create a new flow
                        let desktop_bounds = app.desktop.get_bounds();
                        let output_view = OutputView::new(desktop_bounds, "Flow");
                        let mut flow = Box::new(Flow::new(output_view));

                        // Add output view to desktop
                        app.desktop.add(Box::new(FlowOutputProxy {
                            flow_ptr: &mut *flow as *mut Flow,
                        }));

                        // Open prompt dialog
                        let (sw, sh) = app.terminal.size();
                        let prompt = PromptDialog::new(sw, sh);
                        let result = app.exec_view(prompt);

                        if result == CM_OK {
                            // TODO: get text from dialog (needs dialog data extraction)
                            // For now, use a placeholder
                            let prompt_text = "Hello, Claude!";
                            if let Err(e) = flow.start_prompt(prompt_text) {
                                eprintln!("Failed to start: {e}");
                            } else {
                                // Show progress dialog
                                let progress = ProgressDialog::new(sw, sh);
                                // Run progress dialog - it closes when flow completes or user cancels
                                let progress_result = app.exec_view(progress);
                                if progress_result == CM_CANCEL {
                                    flow.cancel();
                                }
                            }
                        }

                        active_flow = Some(flow);
                        event.clear();
                    }
                    CM_OPEN_PROMPT => {
                        // User pressed Enter in output window — open prompt dialog
                        if active_flow.is_some() {
                            let (sw, sh) = app.terminal.size();
                            let prompt = PromptDialog::new(sw, sh);
                            let result = app.exec_view(prompt);

                            if result == CM_OK {
                                if let Some(ref mut flow) = active_flow {
                                    let prompt_text = "Follow-up prompt"; // TODO: extract from dialog
                                    if let Err(e) = flow.start_prompt(prompt_text) {
                                        eprintln!("Failed to start: {e}");
                                    } else {
                                        let progress = ProgressDialog::new(sw, sh);
                                        let progress_result = app.exec_view(progress);
                                        if progress_result == CM_CANCEL {
                                            flow.cancel();
                                        }
                                    }
                                }
                            }
                        }
                        event.clear();
                    }
                    _ => {}
                }
            }

            if event.what != EventType::Nothing {
                app.handle_event(&mut event);
            }
        } else {
            // No event — idle processing
            if let Some(ref mut flow) = active_flow {
                flow.poll();
            }
        }
    }

    Ok(())
}

/// Proxy view that delegates to the Flow's OutputView.
/// This lets us add the output view to the desktop while the Flow owns it.
struct FlowOutputProxy {
    flow_ptr: *mut Flow,
}

// SAFETY: Single-threaded UI
unsafe impl Send for FlowOutputProxy {}
unsafe impl Sync for FlowOutputProxy {}

impl View for FlowOutputProxy {
    fn bounds(&self) -> Rect {
        unsafe { (*self.flow_ptr).output_view.bounds() }
    }
    fn set_bounds(&mut self, bounds: Rect) {
        unsafe { (*self.flow_ptr).output_view.set_bounds(bounds); }
    }
    fn draw(&mut self, terminal: &mut turbo_vision::terminal::Terminal) {
        unsafe { (*self.flow_ptr).output_view.draw(terminal); }
    }
    fn handle_event(&mut self, event: &mut Event) {
        unsafe { (*self.flow_ptr).output_view.handle_event(event); }
    }
    fn can_focus(&self) -> bool { true }
    fn state(&self) -> turbo_vision::core::state::StateFlags {
        unsafe { (*self.flow_ptr).output_view.state() }
    }
    fn set_state(&mut self, state: turbo_vision::core::state::StateFlags) {
        unsafe { (*self.flow_ptr).output_view.set_state(state); }
    }
    fn options(&self) -> u16 {
        unsafe { (*self.flow_ptr).output_view.options() }
    }
    fn set_options(&mut self, options: u16) {
        unsafe { (*self.flow_ptr).output_view.set_options(options); }
    }
    fn get_palette(&self) -> Option<turbo_vision::core::palette::Palette> {
        unsafe { (*self.flow_ptr).output_view.get_palette() }
    }
    fn get_end_state(&self) -> CommandId {
        unsafe { (*self.flow_ptr).output_view.get_end_state() }
    }
    fn set_end_state(&mut self, cmd: CommandId) {
        unsafe { (*self.flow_ptr).output_view.set_end_state(cmd); }
    }
}
```

**Note:** The `FlowOutputProxy` with raw pointers is a pragmatic choice for Phase 1. The `Flow` owns both the `OutputView` data and the `Session`, while the desktop needs a `Box<dyn View>` for the output. In Phase 2, this can be refactored to use `Rc<RefCell<>>` (like `LogWindow` does).

The `TODO` comments for dialog text extraction will be resolved in Task 10 when we fix the `Memo::get_text()` integration.

- [ ] **Step 3: Build to verify it compiles**

Run: `cargo build 2>&1`
Expected: Successful build (there may be warnings about unused code — that's OK)

- [ ] **Step 4: Commit**

```bash
git add src/ui/flow.rs src/ui/mod.rs src/main.rs
git commit -m "feat: flow state machine and main loop integration with output/prompt/progress"
```

---

### Task 9: Prompt Text Extraction and End-to-End Wiring

**Files:**
- Modify: `src/main.rs`
- Modify: `src/ui/prompt_dialog.rs`

This task removes the placeholder prompt text and properly extracts the user's input from the Memo widget.

- [ ] **Step 1: Check how Memo exposes text**

Run: `cargo doc -p turbo-vision --no-deps --open 2>&1 | head -5`

Look at the `Memo` struct documentation for `get_text()` method. The source at `vendor/tolaria/...` isn't relevant here — we need the turbo-vision Memo API.

Run: `grep -n "pub fn get_text" ~/.cargo/registry/src/index.crates.io-*/turbo-vision-1.2.0/src/views/memo.rs`

- [ ] **Step 2: Update PromptDialog to return text without unsafe**

Instead of trying to downcast, store the `Memo` in an `Rc<RefCell<>>` (same pattern as `LogWindow`'s `TerminalWidget`):

```rust
// src/ui/prompt_dialog.rs
use std::cell::RefCell;
use std::rc::Rc;

use turbo_vision::core::command::{CM_CANCEL, CM_OK, CommandId};
use turbo_vision::core::event::Event;
use turbo_vision::core::geometry::Rect;
use turbo_vision::core::state::StateFlags;
use turbo_vision::terminal::Terminal;
use turbo_vision::views::button::Button;
use turbo_vision::views::dialog::Dialog;
use turbo_vision::views::memo::Memo;
use turbo_vision::views::view::View;

/// Shared wrapper for Memo so we can both add it to the Dialog and read its text.
struct SharedMemo(Rc<RefCell<Memo>>);

impl View for SharedMemo {
    fn bounds(&self) -> Rect { self.0.borrow().bounds() }
    fn set_bounds(&mut self, bounds: Rect) { self.0.borrow_mut().set_bounds(bounds); }
    fn draw(&mut self, terminal: &mut Terminal) { self.0.borrow_mut().draw(terminal); }
    fn handle_event(&mut self, event: &mut Event) { self.0.borrow_mut().handle_event(event); }
    fn can_focus(&self) -> bool { true }
    fn state(&self) -> StateFlags { self.0.borrow().state() }
    fn set_state(&mut self, state: StateFlags) { self.0.borrow_mut().set_state(state); }
    fn get_palette(&self) -> Option<turbo_vision::core::palette::Palette> {
        self.0.borrow().get_palette()
    }
}

pub struct PromptDialog {
    dialog: Dialog,
    memo: Rc<RefCell<Memo>>,
}

impl PromptDialog {
    pub fn new(screen_width: u16, screen_height: u16) -> Box<Self> {
        let dialog_w: i16 = 60.min(screen_width as i16 - 4);
        let dialog_h: i16 = 14.min(screen_height as i16 - 4);
        let x = ((screen_width as i16) - dialog_w) / 2;
        let y = ((screen_height as i16) - dialog_h) / 2;

        let bounds = Rect::new(x, y, x + dialog_w, y + dialog_h);
        let mut dialog = Dialog::new(bounds, "Enter Prompt");

        let memo_bounds = Rect::new(2, 1, dialog_w - 2, dialog_h - 4);
        let memo = Rc::new(RefCell::new(Memo::new(memo_bounds).with_scrollbars(true)));
        dialog.add(Box::new(SharedMemo(Rc::clone(&memo))));

        let btn_y = dialog_h - 3;
        let ok_x = dialog_w / 2 - 12;
        dialog.add(Box::new(Button::new(
            Rect::new(ok_x, btn_y, ok_x + 10, btn_y + 2),
            "~O~K",
            CM_OK,
            true,
        )));
        let cancel_x = ok_x + 12;
        dialog.add(Box::new(Button::new(
            Rect::new(cancel_x, btn_y, cancel_x + 12, btn_y + 2),
            "Cancel",
            CM_CANCEL,
            false,
        )));

        dialog.set_initial_focus();

        use turbo_vision::core::state::SF_MODAL;
        let current_state = dialog.state();
        dialog.set_state(current_state | SF_MODAL);

        Box::new(Self { dialog, memo })
    }

    /// Get the text entered in the memo field.
    pub fn get_text(&self) -> String {
        self.memo.borrow().get_text()
    }
}

impl View for PromptDialog {
    fn bounds(&self) -> Rect { self.dialog.bounds() }
    fn set_bounds(&mut self, bounds: Rect) { self.dialog.set_bounds(bounds); }
    fn draw(&mut self, terminal: &mut Terminal) { self.dialog.draw(terminal); }
    fn handle_event(&mut self, event: &mut Event) { self.dialog.handle_event(event); }
    fn can_focus(&self) -> bool { true }
    fn state(&self) -> StateFlags { self.dialog.state() }
    fn set_state(&mut self, state: StateFlags) { self.dialog.set_state(state); }
    fn options(&self) -> u16 { self.dialog.options() }
    fn set_options(&mut self, options: u16) { self.dialog.set_options(options); }
    fn get_palette(&self) -> Option<turbo_vision::core::palette::Palette> {
        self.dialog.get_palette()
    }
    fn get_end_state(&self) -> CommandId { self.dialog.get_end_state() }
    fn set_end_state(&mut self, cmd: CommandId) { self.dialog.set_end_state(cmd); }
}
```

- [ ] **Step 3: Update main.rs to extract prompt text from dialog**

The key change is capturing the `PromptDialog` reference before `exec_view` consumes it. Since `exec_view` takes ownership, we need to use the `Rc<RefCell<Memo>>` approach. But `exec_view` takes `Box<dyn View>`, so we capture the `Rc<RefCell<Memo>>` before passing the dialog:

Replace the CM_NEW_FLOW and CM_OPEN_PROMPT handlers in main.rs. The prompt flow becomes a helper function:

Add this function before `main()`:

```rust
fn run_prompt(app: &mut Application, flow: &mut Flow) {
    let (sw, sh) = app.terminal.size();
    let prompt = PromptDialog::new(sw, sh);

    // Clone the Rc to read text after dialog closes
    let memo_ref = prompt.memo_ref();
    let result = app.exec_view(prompt);

    if result != CM_OK {
        return;
    }

    let prompt_text = memo_ref.borrow().get_text();
    if prompt_text.trim().is_empty() {
        return;
    }

    if let Err(e) = flow.start_prompt(&prompt_text) {
        // Show error — for now just append to output
        flow.output_view.handle_ui_event(
            &crate::claude::protocol::UiEvent::Error { message: e },
        );
        return;
    }

    // Show progress dialog — poll flow in idle
    let progress = ProgressDialog::new(sw, sh);
    let progress_result = app.exec_view(progress);
    if progress_result == CM_CANCEL {
        flow.cancel();
    }
}
```

Add a `memo_ref()` method to `PromptDialog`:
```rust
/// Get a clone of the shared Memo reference (for reading text after dialog closes).
pub fn memo_ref(&self) -> Rc<RefCell<Memo>> {
    Rc::clone(&self.memo)
}
```

**Note:** `exec_view` takes `Box<dyn View>` which consumes the PromptDialog. We need to clone the `Rc` before passing ownership. This works because `Rc` is cheaply cloneable and the `Memo` data persists after the dialog is removed from the desktop.

- [ ] **Step 4: Build and run to verify end-to-end**

Run: `cargo build 2>&1`
Expected: Successful build

Run: `cargo run`
Expected: Empty desktop → Ctrl+N → prompt dialog appears → type text → OK → progress dialog → Claude runs → output appears in window → Enter reopens prompt

- [ ] **Step 5: Commit**

```bash
git add src/ui/prompt_dialog.rs src/main.rs
git commit -m "feat: end-to-end prompt text extraction and flow wiring"
```

---

### Task 10: Permission Dialog

**Files:**
- Create: `src/ui/permission_dialog.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Implement the permission dialog**

Add to `src/ui/mod.rs`:
```rust
pub mod output_view;
pub mod prompt_dialog;
pub mod progress_dialog;
pub mod flow;
pub mod permission_dialog;
```

```rust
// src/ui/permission_dialog.rs
use turbo_vision::core::command::{CM_CANCEL, CM_OK, CM_YES, CommandId};
use turbo_vision::core::event::Event;
use turbo_vision::core::geometry::Rect;
use turbo_vision::core::state::StateFlags;
use turbo_vision::terminal::Terminal;
use turbo_vision::views::button::Button;
use turbo_vision::views::dialog::Dialog;
use turbo_vision::views::static_text::StaticText;
use turbo_vision::views::view::View;

/// CM_YES = Approve (once), CM_OK = Always Allow, CM_CANCEL = Deny
pub const CM_ALWAYS_ALLOW: CommandId = CM_OK;
pub const CM_APPROVE: CommandId = CM_YES;
pub const CM_DENY: CommandId = CM_CANCEL;

pub struct PermissionDialog {
    dialog: Dialog,
}

impl PermissionDialog {
    pub fn new(
        screen_width: u16,
        screen_height: u16,
        tool_name: &str,
        input_preview: &str,
    ) -> Box<Self> {
        let dialog_w: i16 = 55.min(screen_width as i16 - 4);
        let dialog_h: i16 = 12.min(screen_height as i16 - 4);
        let x = ((screen_width as i16) - dialog_w) / 2;
        let y = ((screen_height as i16) - dialog_h) / 2;

        let title = format!("Permission: {tool_name}");
        let bounds = Rect::new(x, y, x + dialog_w, y + dialog_h);
        let mut dialog = Dialog::new(bounds, &title);

        // Tool name label
        let label_text = format!("Tool: {tool_name}");
        dialog.add(Box::new(StaticText::new(
            Rect::new(2, 1, dialog_w - 2, 2),
            &label_text,
        )));

        // Input preview (truncated)
        let preview = if input_preview.len() > ((dialog_w - 4) as usize * 3) {
            let max = (dialog_w - 4) as usize * 3;
            format!("{}...", &input_preview[..max])
        } else {
            input_preview.to_string()
        };
        dialog.add(Box::new(StaticText::new(
            Rect::new(2, 3, dialog_w - 2, 6),
            &preview,
        )));

        // Three buttons
        let btn_y = dialog_h - 3;
        let approve_btn = Button::new(
            Rect::new(2, btn_y, 14, btn_y + 2),
            "~A~pprove",
            CM_APPROVE,
            true,
        );
        dialog.add(Box::new(approve_btn));

        let always_btn = Button::new(
            Rect::new(16, btn_y, 32, btn_y + 2),
            "A~l~ways Allow",
            CM_ALWAYS_ALLOW,
            false,
        );
        dialog.add(Box::new(always_btn));

        let deny_btn = Button::new(
            Rect::new(34, btn_y, 46, btn_y + 2),
            "~D~eny",
            CM_DENY,
            false,
        );
        dialog.add(Box::new(deny_btn));

        dialog.set_initial_focus();

        use turbo_vision::core::state::SF_MODAL;
        let current_state = dialog.state();
        dialog.set_state(current_state | SF_MODAL);

        Box::new(Self { dialog })
    }
}

impl View for PermissionDialog {
    fn bounds(&self) -> Rect { self.dialog.bounds() }
    fn set_bounds(&mut self, bounds: Rect) { self.dialog.set_bounds(bounds); }
    fn draw(&mut self, terminal: &mut Terminal) { self.dialog.draw(terminal); }
    fn handle_event(&mut self, event: &mut Event) { self.dialog.handle_event(event); }
    fn can_focus(&self) -> bool { true }
    fn state(&self) -> StateFlags { self.dialog.state() }
    fn set_state(&mut self, state: StateFlags) { self.dialog.set_state(state); }
    fn options(&self) -> u16 { self.dialog.options() }
    fn set_options(&mut self, options: u16) { self.dialog.set_options(options); }
    fn get_palette(&self) -> Option<turbo_vision::core::palette::Palette> {
        self.dialog.get_palette()
    }
    fn get_end_state(&self) -> CommandId { self.dialog.get_end_state() }
    fn set_end_state(&mut self, cmd: CommandId) { self.dialog.set_end_state(cmd); }
}
```

- [ ] **Step 2: Build to verify it compiles**

Run: `cargo build 2>&1`
Expected: Successful build

- [ ] **Step 3: Commit**

```bash
git add src/ui/permission_dialog.rs src/ui/mod.rs
git commit -m "feat: permission dialog with Approve/Always Allow/Deny buttons"
```

---

### Task 11: Markdown Rendering with pulldown-cmark

**Files:**
- Create: `src/ui/markdown.rs`
- Create: `tests/markdown.rs`
- Modify: `src/ui/output_view.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/markdown.rs
use turbo_claw::ui::markdown::render_markdown;

#[test]
fn plain_text() {
    let lines = render_markdown("Hello world");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, "Hello world");
    assert!(!lines[0].is_bold);
    assert!(!lines[0].is_code);
}

#[test]
fn heading() {
    let lines = render_markdown("## Architecture");
    assert_eq!(lines.len(), 1);
    assert!(lines[0].is_heading);
}

#[test]
fn code_block() {
    let input = "```rust\nfn main() {}\n```";
    let lines = render_markdown(input);
    // Should have: code fence open, code line, code fence close
    assert!(lines.iter().any(|l| l.is_code && l.text.contains("fn main")));
}

#[test]
fn inline_code() {
    let lines = render_markdown("Use `cargo build` to compile");
    assert_eq!(lines.len(), 1);
    assert!(lines[0].segments.iter().any(|s| s.is_code && s.text == "cargo build"));
}

#[test]
fn bold_text() {
    let lines = render_markdown("This is **important** text");
    assert_eq!(lines.len(), 1);
    assert!(lines[0].segments.iter().any(|s| s.is_bold && s.text == "important"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test markdown 2>&1`
Expected: FAIL — module doesn't exist

- [ ] **Step 3: Implement markdown renderer**

Add to `src/ui/mod.rs`:
```rust
pub mod markdown;
```

```rust
// src/ui/markdown.rs
use pulldown_cmark::{Event, Parser, Tag, TagEnd, CodeBlockKind};

/// A segment of text with formatting attributes.
#[derive(Debug, Clone)]
pub struct TextSegment {
    pub text: String,
    pub is_bold: bool,
    pub is_italic: bool,
    pub is_code: bool,
}

/// A rendered line of markdown.
#[derive(Debug, Clone)]
pub struct RenderedLine {
    pub text: String,
    pub segments: Vec<TextSegment>,
    pub is_heading: bool,
    pub is_code: bool,
}

impl RenderedLine {
    pub fn is_bold(&self) -> bool {
        self.segments.iter().all(|s| s.is_bold)
    }
}

/// Render markdown text into a list of attributed lines.
pub fn render_markdown(input: &str) -> Vec<RenderedLine> {
    let parser = Parser::new(input);
    let mut lines: Vec<RenderedLine> = Vec::new();
    let mut current_segments: Vec<TextSegment> = Vec::new();
    let mut in_heading = false;
    let mut in_code_block = false;
    let mut bold_depth = 0u32;
    let mut italic_depth = 0u32;
    let mut code_depth = 0u32;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                in_heading = true;
            }
            Event::End(TagEnd::Heading(_)) => {
                flush_line(&mut lines, &mut current_segments, in_heading, in_code_block);
                in_heading = false;
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush_line(&mut lines, &mut current_segments, in_heading, false);
                in_code_block = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                flush_line(&mut lines, &mut current_segments, in_heading, in_code_block);
                in_code_block = false;
            }
            Event::Start(Tag::Strong) => {
                bold_depth += 1;
            }
            Event::End(TagEnd::Strong) => {
                bold_depth = bold_depth.saturating_sub(1);
            }
            Event::Start(Tag::Emphasis) => {
                italic_depth += 1;
            }
            Event::End(TagEnd::Emphasis) => {
                italic_depth = italic_depth.saturating_sub(1);
            }
            Event::Code(text) => {
                current_segments.push(TextSegment {
                    text: text.to_string(),
                    is_bold: bold_depth > 0,
                    is_italic: italic_depth > 0,
                    is_code: true,
                });
            }
            Event::Text(text) => {
                let text_str = text.to_string();
                if in_code_block {
                    // Code blocks: split by newlines, each is a code line
                    for (i, line) in text_str.split('\n').enumerate() {
                        if i > 0 {
                            flush_line(&mut lines, &mut current_segments, false, true);
                        }
                        if !line.is_empty() {
                            current_segments.push(TextSegment {
                                text: line.to_string(),
                                is_bold: false,
                                is_italic: false,
                                is_code: true,
                            });
                        }
                    }
                } else {
                    current_segments.push(TextSegment {
                        text: text_str,
                        is_bold: bold_depth > 0,
                        is_italic: italic_depth > 0,
                        is_code: code_depth > 0,
                    });
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                flush_line(&mut lines, &mut current_segments, in_heading, in_code_block);
            }
            Event::End(TagEnd::Paragraph) => {
                flush_line(&mut lines, &mut current_segments, in_heading, in_code_block);
            }
            _ => {}
        }
    }

    // Flush remaining
    if !current_segments.is_empty() {
        flush_line(&mut lines, &mut current_segments, in_heading, in_code_block);
    }

    lines
}

fn flush_line(
    lines: &mut Vec<RenderedLine>,
    segments: &mut Vec<TextSegment>,
    is_heading: bool,
    is_code: bool,
) {
    if segments.is_empty() && !is_code {
        return;
    }
    let text = segments.iter().map(|s| s.text.as_str()).collect::<String>();
    lines.push(RenderedLine {
        text,
        segments: std::mem::take(segments),
        is_heading,
        is_code,
    });
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test markdown 2>&1`
Expected: All tests PASS

- [ ] **Step 5: Integrate markdown rendering into OutputView**

Replace the `append_markdown_line` method in `src/ui/output_view.rs`:

```rust
fn append_markdown_line(&mut self, line: &str) {
    use crate::ui::markdown::render_markdown;

    let rendered = render_markdown(line);
    for rline in rendered {
        let attr = if rline.is_heading {
            ATTR_BOLD
        } else if rline.is_code {
            ATTR_CODE
        } else {
            ATTR_TEXT
        };
        self.widget.borrow_mut().append_line_colored(rline.text, attr);
    }
}
```

- [ ] **Step 6: Build and verify**

Run: `cargo build 2>&1`
Expected: Successful build

- [ ] **Step 7: Commit**

```bash
git add src/ui/markdown.rs src/ui/mod.rs src/ui/output_view.rs tests/markdown.rs
git commit -m "feat: markdown rendering with pulldown-cmark for output view"
```

---

### Task 12: Polish — Progress Dialog Idle Updates and Error Handling

**Files:**
- Modify: `src/main.rs`
- Modify: `src/ui/progress_dialog.rs`

This task makes the progress dialog update in real-time while Claude runs, and adds error handling for binary-not-found.

- [ ] **Step 1: Make ProgressDialog an IdleView that polls the flow**

The progress dialog needs to receive status updates while Claude runs. Since `exec_view` runs a modal loop that calls `idle()` on overlay widgets, we register a poller overlay that drains the flow's receiver and updates the progress dialog.

Instead of the overlay approach, the simpler path: the progress dialog should close itself when the flow completes. Add a `close_on_complete` flag that main.rs sets, and use a custom execute loop instead of `exec_view`:

Replace the progress dialog execution in the `run_prompt` function:

```rust
fn run_with_progress(app: &mut Application, flow: &mut Flow) {
    let (sw, sh) = app.terminal.size();
    let mut progress = ProgressDialog::new(sw, sh);

    // Custom modal loop: draw, poll flow, check events
    use turbo_vision::core::state::SF_MODAL;
    let current_state = progress.state();
    progress.set_state(current_state | SF_MODAL);

    // Add to desktop for rendering
    let view_index = app.desktop.child_count();
    app.desktop.add(progress);

    loop {
        // Draw
        app.draw();
        let _ = app.terminal.flush();

        // Poll flow
        let completed = flow.poll();
        if completed {
            app.desktop.remove_child(view_index);
            return;
        }

        // Check for cancel
        match app.terminal.poll_event(std::time::Duration::from_millis(20)).ok().flatten() {
            Some(mut event) => {
                app.handle_event(&mut event);
                if event.what == EventType::Command && event.command == CM_CANCEL {
                    flow.cancel();
                    app.desktop.remove_child(view_index);
                    return;
                }
                if !app.running {
                    flow.cancel();
                    app.desktop.remove_child(view_index);
                    return;
                }
            }
            None => {}
        }
    }
}
```

- [ ] **Step 2: Add error dialog for binary not found**

In main.rs, wrap the flow creation with an error check:

```rust
CM_NEW_FLOW => {
    // Check if claude binary exists before creating a flow
    match crate::claude::binary::find_claude_binary() {
        Ok(_) => { /* proceed with flow creation */ }
        Err(msg) => {
            use turbo_vision::views::msgbox;
            let (sw, sh) = app.terminal.size();
            msgbox::message_box(
                &mut app,
                "Error",
                &msg,
                msgbox::MF_OK_BUTTON,
            );
            event.clear();
            continue;
        }
    }
    // ... rest of flow creation
}
```

- [ ] **Step 3: Build and test end-to-end**

Run: `cargo build 2>&1`
Expected: Successful build

Run: `cargo run`
Expected: Full flow works — Ctrl+N → prompt → Claude runs → progress updates → output rendered → Enter for follow-up

- [ ] **Step 4: Commit**

```bash
git add src/main.rs src/ui/progress_dialog.rs
git commit -m "feat: real-time progress updates and binary-not-found error handling"
```

---

### Task 13: Update CLAUDE.md and Final Cleanup

**Files:**
- Modify: `CLAUDE.md`
- Modify: `src/main.rs` (clippy fixes)

- [ ] **Step 1: Run clippy and fix warnings**

Run: `cargo clippy 2>&1`
Fix any warnings. Common ones: unused imports, unnecessary clones, missing docs.

- [ ] **Step 2: Run formatter**

Run: `cargo fmt`

- [ ] **Step 3: Run all tests**

Run: `cargo test 2>&1`
Expected: All tests pass

- [ ] **Step 4: Update CLAUDE.md with the actual architecture**

Update the Architecture section to reflect what was actually built (module paths, key types, flow lifecycle).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: clippy fixes, formatting, and CLAUDE.md update"
```
