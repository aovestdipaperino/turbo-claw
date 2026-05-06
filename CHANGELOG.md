# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0] - 2026-05-06

### Added

- Turbo Vision app shell with menu bar and status line
- Claude CLI binary discovery (`claude` / `claude.exe`) via `PATH` and common install locations
- NDJSON streaming protocol types and event dispatch
- Claude session spawning with stdout/stderr reader threads and `mpsc` bridge
- Modal prompt dialog with scrollable memo input
- Modal progress dialog with status/cost display and cancel support
- Output view window with colored, word-wrapped rendering on black background
- Flow state machine (Idle / Running / Done) driving the main event loop
- Permission dialog (Approve / Always Allow / Deny) for tool-use requests
- Markdown rendering via pulldown-cmark

### Fixed

- Output view stays visible during modal dialogs
- `Stdio::null()` for claude stdin to avoid piped-input warning
