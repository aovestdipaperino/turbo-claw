use super::binary::find_claude_binary;
use super::protocol::{StreamState, UiEvent, dispatch_event};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;

pub struct Session {
    child: Child,
    pub receiver: mpsc::Receiver<UiEvent>,
    _stdout_thread: thread::JoinHandle<()>,
    _stderr_thread: thread::JoinHandle<()>,
}

impl Session {
    pub fn spawn(
        prompt: &str,
        resume_session_id: Option<&str>,
        allowed_tools: &HashSet<String>,
    ) -> Result<Self, String> {
        let claude_path = find_claude_binary()?;

        let mut args: Vec<String> = vec![
            "-p".to_string(),
            prompt.to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            "--include-partial-messages".to_string(),
            "--permission-mode".to_string(),
            "acceptEdits".to_string(),
            "--tools".to_string(),
            "Read,Edit,MultiEdit,Write,Glob,Grep,LS".to_string(),
        ];

        if let Some(session_id) = resume_session_id {
            args.push("--resume".to_string());
            args.push(session_id.to_string());
        }

        if !allowed_tools.is_empty() {
            let tools_csv: String = allowed_tools.iter().cloned().collect::<Vec<_>>().join(",");
            args.push("--allowedTools".to_string());
            args.push(tools_csv);
        }

        let mut child = Command::new(&claude_path)
            .args(&args)
            .env_remove("CLAUDECODE")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn claude: {e}"))?;

        let stdout = child.stdout.take().ok_or("No stdout handle")?;
        let stderr = child.stderr.take().ok_or("No stderr handle")?;

        let (tx, receiver): (mpsc::Sender<UiEvent>, mpsc::Receiver<UiEvent>) = mpsc::channel();

        let tx_stdout = tx.clone();
        let stdout_thread = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            let mut state = StreamState::new();
            for line in reader.lines() {
                let line = match line {
                    Ok(l) => l,
                    Err(_) => break,
                };
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let json: serde_json::Value = match serde_json::from_str(trimmed) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let events = dispatch_event(&json, &mut state);
                for event in events {
                    if tx_stdout.send(event).is_err() {
                        return;
                    }
                }
            }
        });

        let tx_stderr = tx;
        let stderr_thread = thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(l) if !l.trim().is_empty() => {
                        if tx_stderr.send(UiEvent::StderrLine(l)).is_err() {
                            return;
                        }
                    }
                    _ => {}
                }
            }
        });

        Ok(Self {
            child,
            receiver,
            _stdout_thread: stdout_thread,
            _stderr_thread: stderr_thread,
        })
    }

    pub fn write_stdin(&mut self, data: &str) -> Result<(), String> {
        if let Some(ref mut stdin) = self.child.stdin {
            stdin
                .write_all(data.as_bytes())
                .map_err(|e| format!("Failed to write to stdin: {e}"))?;
            stdin
                .flush()
                .map_err(|e| format!("Failed to flush stdin: {e}"))?;
            Ok(())
        } else {
            Err("No stdin handle".to_string())
        }
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }

    pub fn try_wait(&mut self) -> Option<i32> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.code().unwrap_or(-1)),
            _ => None,
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.kill();
    }
}
