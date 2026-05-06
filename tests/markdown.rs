use turbo_claw::ui::markdown::render_markdown;

#[test]
fn plain_text() {
    let lines = render_markdown("Hello world");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, "Hello world");
    assert!(!lines[0].is_heading);
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
    assert!(
        lines
            .iter()
            .any(|l| l.is_code && l.text.contains("fn main"))
    );
}

#[test]
fn inline_code() {
    let lines = render_markdown("Use `cargo build` to compile");
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0]
            .segments
            .iter()
            .any(|s| s.is_code && s.text == "cargo build")
    );
}

#[test]
fn bold_text() {
    let lines = render_markdown("This is **important** text");
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0]
            .segments
            .iter()
            .any(|s| s.is_bold && s.text == "important")
    );
}
