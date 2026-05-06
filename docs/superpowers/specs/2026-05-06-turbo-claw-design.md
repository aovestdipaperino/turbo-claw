# Turbo Claw — Design Spec

**Turbo Claw** (Claude Agent Wrapper) is a Turbo Vision TUI that wraps Claude Code CLI as a child process, parsing its NDJSON streaming output into structured, interactive terminal UI panels.

## Flow Lifecycle

A "flow" is a multi-turn conversation session. It owns one output window, one session ID, and a permission allow-list.

### States

```
Empty Desktop
    │ Ctrl+N
    ▼
Output Window created + Prompt Dialog (auto-opens first time)
    │ OK
    ▼
Progress Dialog (streams output into window behind it)
    │ Claude finishes
    ▼
Output Window (resting state, has focus)
    │ Enter
    ▼
Prompt Dialog (--resume <session_id>)
    │ OK
    ▼
Progress Dialog ... (loop)
```

### Interactions

- **New Flow (Ctrl+N):** Creates an output window on the desktop and immediately opens the prompt dialog.
- **Prompt Dialog:** Modal. Scrollable text input area + OK + Cancel. OK sends the prompt. Cancel closes the dialog and returns to resting state (or closes the flow if it was the first prompt).
- **Progress Dialog:** Modal. Shows current status message, cost/tokens, spinner. Cancel kills the child process and returns to resting state.
- **Output Window (resting state):** Scrollable. User reads output. Press Enter to open the prompt dialog for the next turn.
- **Permission Dialog:** Modal. Shown when Claude requests permission for a tool not in the allow-list. Three buttons: Approve (once), Always Allow (add to session allow-list), Deny.

## Claude CLI Integration

### Binary Discovery

Search in order:
1. `which claude` (or `where` on Windows)
2. Shell login resolution: `$SHELL -lc "command -v claude"` (falls back to `/bin/zsh`, `/bin/bash`)
3. Hardcoded candidate paths:
   - `~/.local/bin/claude`
   - `~/.claude/local/claude`
   - `~/.local/share/mise/shims/claude`
   - `~/.asdf/shims/claude`
   - `~/.npm-global/bin/claude`, `~/.npm/bin/claude`, `~/.bun/bin/claude`
   - `/opt/homebrew/bin/claude`
   - `/usr/local/bin/claude`
   - `~/.nvm/versions/node/*/bin/claude`

### Spawning

Each prompt is a separate `claude -p <prompt>` invocation.

**First turn:**
```
claude -p <prompt> \
  --output-format stream-json \
  --verbose \
  --include-partial-messages \
  --permission-mode acceptEdits \
  --tools "Read,Edit,MultiEdit,Write,Glob,Grep,LS"
```

**Subsequent turns** (same + resume):
```
claude -p <prompt> \
  --output-format stream-json \
  --verbose \
  --include-partial-messages \
  --permission-mode acceptEdits \
  --tools "Read,Edit,MultiEdit,Write,Glob,Grep,LS" \
  --resume <session_id>
```

**I/O setup:**
- Stdin: **piped** (open, not null) — used for permission responses
- Stdout: piped → reader thread → NDJSON parse → `mpsc::Sender<UiEvent>`
- Stderr: piped → reader thread → `mpsc::Sender<UiEvent>` as error lines

### NDJSON Protocol

Parsed as raw `serde_json::Value`, dispatched on `json["type"]`:

| Claude JSON `type` | Subtype / condition | UiEvent |
|---|---|---|
| `system` | `subtype == "init"` | `Init { session_id, model }` |
| `stream_event` | `content_block_delta`, `delta.type == "text_delta"` | `TextDelta { text }` |
| `stream_event` | `content_block_delta`, `delta.type == "thinking_delta"` | `ThinkingDelta { text }` |
| `stream_event` | `content_block_delta`, `delta.type == "input_json_delta"` | Accumulated in `StreamState.tool_inputs` |
| `stream_event` | `content_block_start`, block is `tool_use` | `ToolStart { tool_name, tool_id, input }` |
| `stream_event` | `content_block_stop` | Clears `current_tool_id` |
| `tool_progress` | has `tool_name` + `tool_use_id` | `ToolProgress { tool_id, content }` |
| `tool_result` | has `tool_use_id` | `ToolDone { tool_id, output, is_error }` |
| `result` | — | `Result { text, session_id, duration_ms, cost_usd, usage }` |

**StreamState** tracks: `session_id`, `tool_inputs` (HashMap accumulating `input_json_delta` chunks), `current_tool_id`, `emitted_text` (prevents duplicate text in Result).

### Permission Flow

- Default: `--permission-mode acceptEdits` + `--tools` whitelist covers most operations without prompting.
- When a permission request arrives in the NDJSON stream (exact message type TBD — Tolaria avoids this by using `Stdio::null()` for stdin; we need to discover the wire format empirically by running `claude` with stdin open and a restrictive `--tools` list), the UI shows a `PermissionDialog`:
  - Tool name, truncated input JSON preview
  - **Approve** — write approval to stdin, one-time
  - **Always Allow** — write approval to stdin, add tool to session allow-list (passed as `--allowedTools` on subsequent turns)
  - **Deny** — write denial to stdin

## Output Rendering

The output window contains a scrollable view rendering Claude's streamed content:

- **Text:** Markdown rendered via `pulldown-cmark`. Bold, italic, inline code, fenced code blocks (monochrome boxed region with language label).
- **Thinking:** Dimmed/italic, prefixed with `[thinking]`, collapsible.
- **Tool use:** Framed box, tool name as title, truncated JSON input preview.
- **Tool result:** Appended below its tool_use frame. Green border = success, red = error.
- **Turn separators:** `── Done (3.2s, $0.05, 2.4k tokens) ──` after each completion.

Auto-scrolls to bottom during streaming. Respects manual scroll position if user scrolled up.

## Module Structure

```
src/
├── main.rs                 — App entry, Application setup, menu bar, event loop
├── claude/
│   ├── mod.rs              — Re-exports
│   ├── binary.rs           — Find claude CLI binary on disk
│   ├── session.rs          — Spawn process, manage stdin/stdout/stderr, kill
│   └── protocol.rs         — NDJSON dispatch, StreamState, UiEvent enum
├── ui/
│   ├── mod.rs              — Re-exports
│   ├── flow.rs             — Flow struct: owns Window + session_id + state machine
│   ├── output_view.rs      — Scrollable rendered output (text/thinking/tools)
│   ├── prompt_dialog.rs    — Modal dialog with scrollable text input + OK/Cancel
│   ├── progress_dialog.rs  — Modal with status text, cost/tokens, Cancel button
│   ├── permission_dialog.rs— Approve/Always Allow/Deny modal
│   └── markdown.rs         — pulldown-cmark → attributed text conversion
└── bridge.rs               — mpsc channel, UiEvent enum, idle() polling
```

## Key Types

### Flow

```rust
struct Flow {
    window: Window,               // output window on desktop
    session_id: Option<String>,   // None until first system.init
    state: FlowState,             // Idle, WaitingForPrompt, Running, Done
    allowed_tools: HashSet<String>, // session allow-list for permissions
    receiver: mpsc::Receiver<UiEvent>,
}

enum FlowState {
    Idle,              // just created, no prompt yet
    WaitingForPrompt,  // prompt dialog is open
    Running,           // claude process active, progress dialog showing
    Done,              // completed, output window has focus
}
```

### UiEvent

```rust
enum UiEvent {
    Init { session_id: String, model: String },
    TextDelta { text: String },
    ThinkingDelta { text: String },
    ToolStart { tool_name: String, tool_id: String, input: Option<String> },
    ToolProgress { tool_id: String, content: String },
    ToolDone { tool_id: String, output: Option<String>, is_error: bool },
    PermissionRequest { tool_name: String, input: String },
    Result { text: String, session_id: String, duration_ms: u64, cost_usd: f64, usage: Usage },
    Error { message: String },
    StderrLine(String),
    ProcessExited(i32),
}
```

### StreamState

```rust
struct StreamState {
    session_id: Option<String>,
    tool_inputs: HashMap<String, String>,  // tool_use_id → accumulated JSON
    current_tool_id: Option<String>,
    emitted_text: bool,  // prevents duplicate text in Result event
}
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| `turbo-vision` | TUI framework |
| `serde` + `serde_json` | NDJSON parsing |
| `pulldown-cmark` | Markdown → attributed text |
| `dirs` | Home directory expansion |
| `which` | PATH-based binary lookup |

## Thread Model

| Thread | Role | Communication |
|--------|------|---------------|
| Main (UI) | Turbo Vision event loop, rendering | Polls `mpsc::Receiver<UiEvent>` via `idle()` or timer |
| Stdout reader | Reads claude stdout, parses NDJSON | `mpsc::Sender<UiEvent>` → main |
| Stderr reader | Reads claude stderr | `mpsc::Sender<UiEvent>` as `StderrLine` |
| (No writer thread) | stdin writes happen synchronously from UI thread | Direct `child.stdin.write()` |
