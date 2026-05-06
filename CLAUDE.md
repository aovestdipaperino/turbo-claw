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

**Thread model:** Main UI thread (Turbo Vision event loop) + background reader threads for stdout/stderr from the Claude CLI child process, connected via `mpsc` channels.

**Data flow:** Claude CLI stdout (NDJSON) → reader thread deserializes → `mpsc::send(UiEvent)` → UI thread's `idle()` polls receiver → dispatches to views.

**Planned module structure:**
- `claude/` — Binary discovery, child process spawning, NDJSON serde types (`SdkMessage` enum with `#[serde(tag = "type")]`)
- `ui/` — Turbo Vision views: chat, tool panel, input line, status bar, permission dialog, markdown renderer
- `bridge.rs` — mpsc channel between reader threads and UI
- `config.rs` — CLI args, MCP config

**Key design decisions:**
- Uses `idle()` polling on the TV event loop (non-blocking `try_recv`) rather than async or fd multiplexing
- Stdin writes to the child process happen synchronously from the UI thread (no writer thread)
- Claude Code is spawned with `--no-session --print-streaming --permission-mode <mode>`
- The TUI framework decision (turbo-vision-4-rust vs ratatui fallback) is still open

## Rust Edition

This project uses Rust edition **2024** (see Cargo.toml).
