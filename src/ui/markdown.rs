use pulldown_cmark::{Event, Parser, Tag, TagEnd};

#[derive(Debug, Clone)]
pub struct TextSegment {
    pub text: String,
    pub is_bold: bool,
    pub is_italic: bool,
    pub is_code: bool,
}

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

pub fn render_markdown(input: &str) -> Vec<RenderedLine> {
    let parser = Parser::new(input);
    let mut lines: Vec<RenderedLine> = Vec::new();
    let mut current_segments: Vec<TextSegment> = Vec::new();
    let mut in_heading = false;
    let mut in_code_block = false;
    let mut bold_depth = 0u32;
    let mut italic_depth = 0u32;

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
                        is_code: false,
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
