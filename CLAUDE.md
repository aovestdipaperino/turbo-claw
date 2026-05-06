# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Turbo Claw?

Turbo Claw (**Cl**aude **A**gent **W**rapper) is a Turbo Vision TUI that wraps Claude Code as a child process, parsing its NDJSON streaming output into structured, interactive terminal UI panels. Single-binary Rust, no Node.js runtime.

The full design plan lives in `docs/TURBO-CLAW-PLAN.md`.

## Build & Test Commands

```bash
cargo build              # Build debug
cargo build --release    # Build release
cargo run                # Run debug
cargo test               # Run all tests
cargo test <test_name>   # Run a single test
cargo clippy             # Lint
cargo fmt -- --check     # Check formatting
cargo fmt                # Auto-format
```

## Architecture

**Thread model:** Main UI thread (custom Turbo Vision event loop) + background reader threads for stdout/stderr from the Claude CLI child process, connected via `mpsc` channels.

**Data flow:** Claude CLI stdout (NDJSON) → reader thread dispatches via `protocol::dispatch_event` → `mpsc::send(UiEvent)` → UI thread polls via `flow.poll()` on each loop iteration → dispatches to `OutputView`.

**Module structure:**

```
src/
├── main.rs          — Custom event loop, flow management, menu/status bar
├── lib.rs           — Crate root (pub mod bridge, claude, ui)
├── bridge.rs        — mpsc channel helper
├── claude/
│   ├── mod.rs
│   ├── binary.rs    — Find claude CLI binary on disk
│   ├── session.rs   — Spawn process, reader threads, stdin write
│   └── protocol.rs  — NDJSON dispatch, UiEvent, StreamState
└── ui/
    ├── mod.rs
    ├── flow.rs              — Flow state machine (Idle/Running/Done)
    ├── output_view.rs       — Window + TerminalWidget for streamed output
    ├── prompt_dialog.rs     — Modal Memo input + OK/Cancel
    ├── progress_dialog.rs   — Modal status/cost display + Cancel
    ├── permission_dialog.rs — Approve/Always Allow/Deny modal
    └── markdown.rs          — pulldown-cmark renderer
```

**Key design decisions:**
- Non-blocking `try_recv` polling on each event loop tick — no `idle()` hook, no async, no fd multiplexing
- Stdin writes to the child process happen synchronously from the UI thread (no writer thread)
- Claude Code is spawned with `--output-format stream-json --verbose`
- Progress dialog is driven by a manual modal loop (not `exec_view`) so flow polling continues while the dialog is open

## Rust Edition

This project uses Rust edition **2024** (see Cargo.toml).
