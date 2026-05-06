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
    if subtype != "init" { return vec![]; }
    let session_id = json["session_id"].as_str().unwrap_or("").to_string();
    let model = json["model"].as_str().unwrap_or("unknown").to_string();
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
                    let partial = delta["partial_json"].as_str().unwrap_or("");
                    if let Some(ref tool_id) = state.current_tool_id {
                        state.tool_inputs.entry(tool_id.clone()).or_default().push_str(partial);
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
    let tool_id = json["tool_use_id"].as_str().unwrap_or("").to_string();
    let content = json["content"].as_str().unwrap_or("").to_string();
    if state.current_tool_id.as_deref() != Some(&tool_id) {
        let tool_name = json["tool_name"].as_str().unwrap_or("unknown").to_string();
        state.current_tool_id = Some(tool_id.clone());
        return vec![
            UiEvent::ToolStart { tool_name, tool_id: tool_id.clone(), input: None },
            UiEvent::ToolProgress { tool_id, content },
        ];
    }
    vec![UiEvent::ToolProgress { tool_id, content }]
}

fn dispatch_tool_result(json: &serde_json::Value, _state: &mut StreamState) -> Vec<UiEvent> {
    let tool_id = json["tool_use_id"].as_str().unwrap_or("").to_string();
    let output = json["content"].as_str().map(String::from);
    let is_error = json["is_error"].as_bool().unwrap_or(false);
    vec![UiEvent::ToolDone { tool_id, output, is_error }]
}

fn dispatch_result(json: &serde_json::Value, state: &mut StreamState) -> Vec<UiEvent> {
    let session_id = json["session_id"].as_str().map(String::from)
        .or_else(|| state.session_id.clone())
        .unwrap_or_default();
    let duration_ms = json["duration_ms"].as_u64().unwrap_or(0);
    let cost_usd = json["cost_usd"].as_f64().unwrap_or(0.0);
    let usage = &json["usage"];
    let input_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
    let output_tokens = usage["output_tokens"].as_u64().unwrap_or(0);
    state.session_id = Some(session_id.clone());
    vec![UiEvent::Result { session_id, duration_ms, cost_usd, input_tokens, output_tokens }]
}
