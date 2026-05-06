use turbo_claw::claude::protocol::{StreamState, UiEvent, dispatch_event};

#[test]
fn parse_system_init() {
    let json: serde_json::Value = serde_json::from_str(r#"{"type":"system","subtype":"init","session_id":"abc123","model":"claude-opus-4-6","tools":[]}"#).unwrap();
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
    let json: serde_json::Value = serde_json::from_str(r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello "}}}"#).unwrap();
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
    let json: serde_json::Value = serde_json::from_str(r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me think..."}}}"#).unwrap();
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
    let json: serde_json::Value = serde_json::from_str(r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tool_123","name":"Read","input":{}}}}"#).unwrap();
    let mut state = StreamState::new();
    let events = dispatch_event(&json, &mut state);
    assert_eq!(events.len(), 1);
    match &events[0] {
        UiEvent::ToolStart {
            tool_name, tool_id, ..
        } => {
            assert_eq!(tool_name, "Read");
            assert_eq!(tool_id, "tool_123");
        }
        other => panic!("Expected ToolStart, got {other:?}"),
    }
    assert_eq!(state.current_tool_id.as_deref(), Some("tool_123"));
}

#[test]
fn parse_tool_result() {
    let json: serde_json::Value = serde_json::from_str(r#"{"type":"tool_result","tool_use_id":"tool_123","content":"File contents here","is_error":false}"#).unwrap();
    let mut state = StreamState::new();
    let events = dispatch_event(&json, &mut state);
    assert_eq!(events.len(), 1);
    match &events[0] {
        UiEvent::ToolDone {
            tool_id,
            output,
            is_error,
        } => {
            assert_eq!(tool_id, "tool_123");
            assert_eq!(output.as_deref(), Some("File contents here"));
            assert!(!is_error);
        }
        other => panic!("Expected ToolDone, got {other:?}"),
    }
}

#[test]
fn parse_result() {
    let json: serde_json::Value = serde_json::from_str(r#"{"type":"result","subtype":"success","session_id":"abc123","duration_ms":3200,"cost_usd":0.05,"usage":{"input_tokens":1200,"output_tokens":800},"result":"The config module..."}"#).unwrap();
    let mut state = StreamState::new();
    let events = dispatch_event(&json, &mut state);
    assert_eq!(events.len(), 1);
    match &events[0] {
        UiEvent::Result {
            session_id,
            duration_ms,
            cost_usd,
            ..
        } => {
            assert_eq!(session_id, "abc123");
            assert_eq!(*duration_ms, 3200);
            assert!((*cost_usd - 0.05).abs() < f64::EPSILON);
        }
        other => panic!("Expected Result, got {other:?}"),
    }
}

#[test]
fn unknown_type_returns_empty() {
    let json: serde_json::Value =
        serde_json::from_str(r#"{"type":"some_future_type","data":42}"#).unwrap();
    let mut state = StreamState::new();
    let events = dispatch_event(&json, &mut state);
    assert!(events.is_empty());
}
