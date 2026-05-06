# Turbo Claw

**Cl**aude **A**gent **W**rapper -- a Turbo Vision TUI that wraps [Claude Code](https://docs.anthropic.com/en/docs/claude-code) as a child process, parsing its streaming JSON output into structured, interactive terminal UI panels.

Single-binary Rust. No Node.js runtime.

## Features

- Spawns Claude Code as a subprocess with `--output-format stream-json`
- Streams NDJSON events into a Turbo Vision output window with markdown rendering
- Modal dialogs for prompt input, progress/cost tracking, and tool permission approval
- Non-blocking event loop with `mpsc` channel bridging between reader threads and the UI

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) (edition 2024)
- [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) installed and on your `PATH`

## Build & Run

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo run                # Run
```

## License

[MIT](LICENSE)
