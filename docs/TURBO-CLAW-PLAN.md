# Turbo Claw — Design & Implementation Plan

**Turbo Claw** is a Turbo Vision TUI that wraps Claude Code (and potentially other AI CLI agents) as a child process, parsing its NDJSON streaming output into structured, interactive terminal UI panels.

The name: **Claw** = **Cl**aude **A**gent **W**rapper.

---

## Table of Contents

1. [Goals & Non-Goals](#1-goals--non-goals)
2. [Architecture Overview](#2-architecture-overview)
3. [SDK Streaming Protocol](#3-sdk-streaming-protocol)
4. [Module Breakdown](#4-module-breakdown)
5. [Implementation Phases](#5-implementation-phases)
6. [Data Flow](#6-data-flow)
7. [UI Layout](#7-ui-layout)
8. [Event Loop Integration](#8-event-loop-integration)
9. [MCP Integration (Optional)](#9-mcp-integration-optional)
10. [Dependencies](#10-dependencies)
11. [Open Questions](#11-open-questions)

---

## 1. Goals & Non-Goals

### Goals

- Spawn Claude Code CLI as a child process and parse its NDJSON stdout
- Render assistant text, thinking, tool calls, and tool results in distinct Turbo Vision panels
- Show live tool progress (file diffs, bash output) as it streams
- Display session metadata: model, cost, token usage, duration
- Support the permission flow — modal dialog for approve/deny, response written to stdin
- Allow the user to type prompts via an input line, sent to Claude's stdin
- Keep it single-binary Rust, no Node.js runtime required for core functionality

### Non-Goals (for now)

- Full MCP server implementation (Phase 3 stretch goal)
- Multi-session/tab management
- Syntax highlighting in code blocks (defer to a later phase)
- Embedding Claude Code's own TUI (we want structured control, not a terminal-in-terminal)
- Supporting non-Claude agents (Gemini CLI, Codex) — defer until the protocol abstraction is proven

---

## 2. Architecture Overview

```
┌──────────────────────────────────────────────────────────┐
│                    Turbo Claw TUI                        │
│                                                          │
│  ┌─────────────────────┐  ┌───────────────────────────┐  │
│  │   TChatView          │  │   TToolPanel              │  │
│  │   (conversation)     │  │   (active tool progress)  │  │
│  │                      │  │                           │  │
│  │  text blocks         │  │  tool name + input        │  │
│  │  thinking (dimmed)   │  │  live stdout/diff         │  │
│  │  tool_use summaries  │  │  result on completion     │  │
│  └─────────────────────┘  └───────────────────────────┘  │
│  ┌─────────────────────────────────────────────────────┐  │
│  │   TInputLine (user prompt)                          │  │
│  └─────────────────────────────────────────────────────┘  │
│  ┌─────────────────────────────────────────────────────┐  │
│  │   TStatusBar: model | cost | tokens | duration      │  │
│  └─────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
        │ stdin (JSON)              ▲ stdout (NDJSON)
        ▼                          │
   ┌─────────────────────────────────┐
   │     claude CLI child process    │
   │  --no-session --print-streaming │
   └─────────────────────────────────┘
```

---

## 3. SDK Streaming Protocol

Claude Code with `--print-streaming` (or `--output-format stream-json`) emits one JSON object per stdout line. The `type` field discriminates:

### Message types to handle

| Type | Subtype | Priority | What to do |
|------|---------|----------|------------|
| `system` | `init` | P0 | Extract model name, tools, cwd, permission mode → status bar |
| `assistant` | — | P0 | Parse `content` blocks: `text`, `thinking`, `tool_use`, `tool_result` |
| `result` | `success` | P0 | Show duration, cost, usage, stop reason → status bar |
| `result` | `error_*` | P0 | Show error in chat view |
| `tool_progress` | — | P1 | Stream live output into TToolPanel |
| `status` | — | P1 | Update status line ("Reading file…") |
| `prompt_suggestion` | — | P2 | Show suggested follow-ups below input |
| `rate_limit_event` | — | P2 | Show rate limit warning |
| `hook_started/progress/response` | — | P3 | Log to debug panel |
| `task_notification/started/progress` | — | P3 | Subagent events — log or nested view |

### Content block types (inside `assistant.content[]`)

| Block type | Rendering |
|------------|-----------|
| `text` | Markdown → attributed text in chat view |
| `thinking` | Dimmed/collapsible section, italic prefix "Thinking:" |
| `tool_use` | Framed panel: tool name header, JSON input preview |
| `tool_result` | Appended below the tool_use frame, success/error indicator |

### Serde model sketch

```rust
#[derive(Deserialize)]
#[serde(tag = "type")]
enum SdkMessage {
    #[serde(rename = "system")]
    System { subtype: Option<String>, /* ... */ },

    #[serde(rename = "assistant")]
    Assistant { message: AssistantMessage },

    #[serde(rename = "result")]
    Result { subtype: String, duration_ms: Option<u64>, cost_usd: Option<f64>, usage: Option<Usage> },

    #[serde(rename = "tool_progress")]
    ToolProgress { tool_use_id: String, content: String },

    #[serde(rename = "status")]
    Status { message: String },

    #[serde(rename = "prompt_suggestion")]
    PromptSuggestion { suggestions: Vec<String> },

    // ... other variants as needed
}

#[derive(Deserialize)]
struct AssistantMessage {
    content: Vec<ContentBlock>,
    // role, model, stop_reason, usage ...
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "thinking")]
    Thinking { thinking: String },

    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String, input: serde_json::Value },

    #[serde(rename = "tool_result")]
    ToolResult { tool_use_id: String, content: String, is_error: Option<bool> },
}
```

---

## 4. Module Breakdown

```
src/
├── main.rs              — App entry, TApplication setup, arg parsing
├── claude/
│   ├── mod.rs           — Re-exports
│   ├── binary.rs        — Find claude CLI binary on disk
│   ├── session.rs       — Spawn process, manage stdin/stdout/stderr
│   └── protocol.rs      — Serde types for NDJSON SDK messages
├── ui/
│   ├── mod.rs           — Re-exports
│   ├── app.rs           — TApplication subclass, idle() polling
│   ├── chat_view.rs     — TChatView: scrollable conversation renderer
│   ├── tool_panel.rs    — TToolPanel: live tool progress display
│   ├── input_line.rs    — TInputLine: user prompt entry
│   ├── status_bar.rs    — TStatusBar: model/cost/tokens/duration
│   ├── permission_dialog.rs — TPermissionDialog: approve/deny modal
│   └── markdown.rs      — Markdown-to-attributed-text converter
├── bridge.rs            — mpsc channel between reader thread and UI
└── config.rs            — CLI args, MCP config generation
```

---

## 5. Implementation Phases

### Phase 1: Spawn & Stream (Milestone: "it talks")

**Goal:** Spawn Claude Code, read NDJSON, print parsed messages to a scrollable view.

1. **`claude::binary`** — Search known paths for `claude` executable
   - Hardcoded path list (same as Tolaria): `~/.local/bin/claude`, `/opt/homebrew/bin/claude`, etc.
   - Also check `$PATH` via `which claude`
   - Return `PathBuf` or error with helpful message

2. **`claude::protocol`** — Define serde types for all SDK message variants
   - Start with `system`, `assistant`, `result` — enough to show a conversation
   - Use `#[serde(tag = "type")]` for the top-level enum
   - Use `#[serde(deny_unknown_fields)]` only on inner types, allow unknown at top level for forward compat

3. **`claude::session`** — Spawn the child process
   - `Command::new(claude_path)` with args: `--no-session`, `--print-streaming`, `--permission-mode`, `bypass`
   - Capture stdout (piped), stdin (piped), stderr (piped)
   - Background thread: read stdout line by line → `serde_json::from_str::<SdkMessage>` → send over `mpsc::Sender<SdkMessage>`
   - Second background thread: read stderr line by line → send as log messages

4. **`bridge`** — `mpsc::channel<SdkMessage>` connecting reader thread to UI thread

5. **`ui::app`** — Minimal TApplication
   - `idle()` method drains `mpsc::Receiver`, dispatches to views
   - Single fullscreen `TChatView` for now

6. **`ui::chat_view`** — Basic scrollable text view
   - On `SdkMessage::Assistant` → append text blocks as plain text (no markdown yet)
   - On `SdkMessage::Result` → append "--- Done (Xs, $Y) ---" separator
   - On `SdkMessage::System` → append "Connected: {model}" header

7. **`ui::input_line`** — Single-line text entry at bottom
   - On Enter: serialize as JSON user message, write to child stdin, clear input
   - Input format: the raw text prompt (Claude Code accepts plain text on stdin in `--no-session` mode, or may need JSON — verify during implementation)

**Deliverable:** Launch turbo-claw, see "Connected: claude-opus-4-6", type a prompt, see the streamed response appear line by line.

### Phase 2: Structured Rendering (Milestone: "it looks good")

**Goal:** Render different content block types distinctly, show tool calls, add status bar.

1. **`ui::markdown`** — Convert markdown text to attributed terminal text
   - Bold (`**`), italic (`*`), inline code (`` ` ``), code fences (``` ``` ```)
   - Use Turbo Vision color attributes: bold = bright white, italic = cyan, code = green on dark
   - Code fences: boxed region with language label
   - No full syntax highlighting yet — just monochrome code blocks

2. **`ui::chat_view` enhancements**
   - Thinking blocks: render with dimmed color, prefix "💭 " or "[thinking]", collapsible
   - Tool use blocks: framed box with tool name as title, JSON input formatted/truncated
   - Tool result blocks: appended below their tool_use, success=green border, error=red border

3. **`ui::tool_panel`** — Side panel showing active tool progress
   - When `tool_progress` events arrive, stream content into this panel
   - Shows the tool name + live output (file diffs, bash commands, etc.)
   - Clears/resets when a new tool starts

4. **`ui::status_bar`**
   - Left: model name, permission mode
   - Center: current status message (from `status` events)
   - Right: cost, token count, duration (from `result` events)

5. **Split layout** — TChatView (left/main) + TToolPanel (right), TInputLine (bottom), TStatusBar (bottom-most)

**Deliverable:** Rich conversation display with distinct visual treatment for text, thinking, tool calls. Live tool output in side panel.

### Phase 3: Permission Flow & Polish (Milestone: "it's usable")

**Goal:** Handle permissions, improve UX, add configuration.

1. **`ui::permission_dialog`**
   - Detect permission requests from the streaming protocol (or a stdin prompt pattern)
   - Show modal TDialog: tool name, truncated input JSON, [Approve] [Deny] buttons
   - On choice: write JSON response to child stdin
   - Support "always allow" toggle per tool for the session

2. **`config`** — CLI argument parsing
   - `--vault <path>` — working directory for Claude
   - `--model <model>` — pass through to Claude's `--model` flag
   - `--permission-mode <safe|power_user|bypass>` — default `safe`
   - `--system-prompt <text or file>` — injected system prompt
   - `--mcp-config <path>` — pass through to Claude

3. **Session management**
   - Ctrl+C: send interrupt to child process (SIGINT), show "Interrupted" in chat
   - Child exit: detect, show exit status, allow restart with Enter
   - Ctrl+Q: quit turbo-claw (kill child if running)

4. **Error handling**
   - Binary not found: show helpful error with install instructions
   - Child crash: show last stderr lines in chat view
   - Malformed JSON: log warning, skip line, continue
   - Rate limit events: show warning banner

5. **Prompt suggestions**
   - When `prompt_suggestion` events arrive, show them as clickable/selectable items below the input line
   - Arrow keys to select, Enter to use

**Deliverable:** Fully interactive Claude Code wrapper with permission dialogs, graceful error handling, and configuration options.

### Phase 4: MCP & Extensions (Stretch)

**Goal:** Native MCP tool support and advanced features.

1. **Native MCP tools** (if wrapping a vault/project-aware context)
   - Implement `search_notes`, `get_note`, `open_note`, etc. in Rust
   - Register as MCP server via `--mcp-config` pointing to a stdio bridge
   - Or: skip MCP, implement as custom tools via `--allowedTools`

2. **Multi-agent support**
   - Abstract the `Session` trait to support different CLI backends
   - Add Gemini CLI, Codex support with protocol adapters

3. **Terminal widget embedding**
   - For `Bash` tool calls, optionally show a live terminal widget
   - Using turbo-vision-4-rust's TTerminalWidget if available

4. **Session persistence**
   - Save/load conversation history
   - Resume sessions

---

## 6. Data Flow

```
                        ┌─────────────────┐
                        │  Reader Thread   │
                        │                  │
   claude stdout ──────►│  line-by-line    │
   (NDJSON)             │  serde deser     │
                        │  SdkMessage enum │
                        └────────┬─────────┘
                                 │ mpsc::send
                                 ▼
                        ┌─────────────────┐
                        │  UI Thread       │
                        │  (event loop)    │
                        │                  │
                        │  idle() polls    │──► TChatView.append(block)
                        │  Receiver        │──► TToolPanel.update(progress)
                        │                  │──► TStatusBar.set(status)
                        └────────┬─────────┘
                                 │
               user types Enter  │
                                 ▼
                        ┌─────────────────┐
                        │  Writer (stdin)  │
                        │                  │
                        │  prompt text or  │──► claude stdin
                        │  permission JSON │
                        └─────────────────┘


                        ┌─────────────────┐
                        │  Stderr Thread   │
                        │                  │
   claude stderr ──────►│  line-by-line    │──► mpsc::send ──► TLogWindow
                        └─────────────────┘
```

### Thread model

| Thread | Role | Communication |
|--------|------|---------------|
| Main (UI) | Turbo Vision event loop, rendering | Polls `mpsc::Receiver<UiEvent>` in `idle()` |
| Stdout reader | Reads claude stdout, deserializes NDJSON | `mpsc::Sender<UiEvent>` → main |
| Stderr reader | Reads claude stderr | `mpsc::Sender<UiEvent>` → main |
| (No writer thread) | stdin writes happen synchronously from UI thread | Direct `child.stdin.write()` |

### UiEvent enum (bridge messages)

```rust
enum UiEvent {
    SdkMessage(SdkMessage),    // parsed NDJSON from stdout
    StderrLine(String),         // raw stderr line
    ProcessExited(ExitStatus),  // child exited
}
```

---

## 7. UI Layout

### Default layout (Phase 1-2)

```
╔══════════════════════════════════════════════════════════════╗
║ Turbo Claw — claude-opus-4-6                         [F10] ║
╠══════════════════════════════════╦═══════════════════════════╣
║                                  ║                           ║
║  Connected: claude-opus-4-6      ║  Tool: Read               ║
║  Permission mode: safe           ║  File: src/main.rs        ║
║                                  ║                           ║
║  User: How does the config work? ║  1│ fn main() {           ║
║                                  ║  2│     println!("...");  ║
║  [thinking]                      ║  3│ }                     ║
║  Let me look at the config...    ║                           ║
║                                  ║                           ║
║  ┌─ Read src/config.rs ────────┐ ║                           ║
║  │ input: { path: "src/..." }  │ ║                           ║
║  │ ✓ 45 lines read             │ ║                           ║
║  └─────────────────────────────┘ ║                           ║
║                                  ║                           ║
║  The config module handles...    ║                           ║
║                                  ║                           ║
╠══════════════════════════════════╩═══════════════════════════╣
║ > _                                                          ║
╠══════════════════════════════════════════════════════════════╣
║ claude-opus-4-6 │ safe │ Reading file… │ $0.03 │ 1.2k tokens ║
╚══════════════════════════════════════════════════════════════╝
```

### Keyboard shortcuts

| Key | Action |
|-----|--------|
| Enter | Send prompt |
| Tab | Toggle focus: input ↔ chat ↔ tool panel |
| Ctrl+Q / F10 | Quit |
| Ctrl+C | Interrupt current generation |
| PgUp/PgDn | Scroll chat view |
| Ctrl+T | Toggle thinking block visibility |
| F1 | Help dialog |

---

## 8. Event Loop Integration

Turbo Vision uses a synchronous, blocking event loop. We need to integrate async child process I/O.

### Approach: idle() polling (recommended)

```rust
impl TApplication for ClawApp {
    fn idle(&mut self) {
        // Drain all pending messages (non-blocking)
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                UiEvent::SdkMessage(msg) => self.dispatch_message(msg),
                UiEvent::StderrLine(line) => self.log_window.append(&line),
                UiEvent::ProcessExited(status) => self.handle_exit(status),
            }
        }
    }
}
```

**Why this approach:**
- `idle()` is called by TV's event loop whenever no keyboard/mouse events are pending
- `try_recv()` is non-blocking — returns immediately if no messages
- Background threads handle all blocking I/O
- No need for self-pipe tricks or fd multiplexing
- Matches existing patterns in turbo-vision-4-rust (e.g. `TLogWindow`)

**Latency consideration:** `idle()` frequency depends on TV's internal timing. If response latency is noticeable, we can post a custom event from the reader thread to wake the event loop:
```rust
// In reader thread, after sending to mpsc:
app_handle.post_event(Event::Custom(WAKE_UP));
```

---

## 9. MCP Integration (Optional)

Two paths, in order of complexity:

### Path A: No MCP (simplest, Phase 1-3)

- Don't pass `--mcp-config` to Claude
- Claude only uses its built-in tools (Read, Edit, Write, Bash, etc.)
- Turbo Claw is purely a viewer/input wrapper
- Good enough for most use cases

### Path B: Native Rust MCP server (Phase 4)

For vault/project-aware features (like Tolaria's note search):

1. Implement an MCP stdio server in Rust
2. Write a temp MCP config JSON file:
   ```json
   {
     "mcpServers": {
       "turbo-claw": {
         "type": "stdio",
         "command": "/path/to/turbo-claw",
         "args": ["--mcp-mode"]
       }
     }
   }
   ```
3. When invoked with `--mcp-mode`, turbo-claw acts as an MCP server (reads JSON-RPC from stdin, writes responses to stdout)
4. Tool calls from Claude → MCP server → direct Rust function calls → response
5. UI actions (open file, highlight) → direct method calls on the TV app (since it's the same binary)

**Advantage over Tolaria's approach:** No Node.js dependency, no WebSocket bridge needed. The MCP server and the TUI are the same process (when `--mcp-mode` is used, it forks or the parent relays).

---

## 10. Dependencies

### Required (Phase 1)

| Crate | Purpose |
|-------|---------|
| `turbo-vision` | TUI framework (Turbo Vision for Rust) |
| `serde` + `serde_json` | NDJSON deserialization |
| `dirs` | Home directory expansion for binary search |
| `which` | PATH-based binary lookup |

### Phase 2

| Crate | Purpose |
|-------|---------|
| `pulldown-cmark` | Markdown parsing for rich text rendering |
| `textwrap` | Text wrapping for chat view |

### Phase 3+

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing |
| `tempfile` | MCP config file generation |

### Note on turbo-vision-4-rust

The turbo-vision crate situation needs to be resolved before Phase 1 begins:
- Check if `turbo-vision` is published on crates.io
- If not, use a git dependency or path dependency
- Verify it provides: `TApplication`, `TWindow`, `TDialog`, `TInputLine`, `TScrollBar`, `TStatusLine`, basic `TView` drawing primitives
- If the crate is too immature, consider `ratatui` as a fallback (different API style but well-maintained)

---

## 11. Open Questions

1. **Claude Code stdin protocol** — In `--no-session --print-streaming` mode, what exactly does Claude Code expect on stdin? Raw text? JSON-wrapped messages? Need to test empirically or find docs.

2. **Permission flow over stdin/stdout** — How exactly are permission requests surfaced in the NDJSON stream? Is there a dedicated message type, or does Claude Code block waiting for stdin input? Need to trace Tolaria's handling.

3. **turbo-vision crate maturity** — Does it support enough widgets for this? Fallback plan is `ratatui`, which would change the architecture (immediate-mode vs retained-mode rendering).

4. **`--print-streaming` vs `--output-format stream-json`** — Which flag is current? Claude Code's CLI flags evolve. Pin to a known-working version initially.

5. **Binary stdin lifecycle** — Can we send multiple prompts to the same Claude Code process (multi-turn conversation), or does each prompt require a new spawn? The `--no-session` flag suggests single-turn, but the streaming protocol may support multi-turn.

6. **Interrupt handling** — When user hits Ctrl+C, should we SIGINT the child or send a cancel message via stdin? What does Tolaria do?

---

## Appendix: Reference — Tolaria's Claude Code Invocation

From analysis of Tolaria's Rust source (see [[Claude Code minions]] note):

```
claude \
  --system-prompt <vault context + AGENTS.md> \
  --append-system-prompt <active note, note list, git status> \
  --mcp-config <generated JSON> \
  --strict-mcp-config \
  --permission-mode <safe | power_user> \
  --allowedTools Read,Edit,MultiEdit,Write,Glob,Grep,LS[,Bash] \
  --no-session-persistence \
  --no-session
```

Binary search paths:
- `~/.local/bin/claude`
- `~/.claude/local/claude`
- `~/.local/share/mise/shims/claude`
- `~/.asdf/shims/claude`
- `~/.npm-global/bin/claude`
- `/opt/homebrew/bin/claude`
- `/usr/local/bin/claude`
- `~/.nvm/versions/node/*/bin/claude`
