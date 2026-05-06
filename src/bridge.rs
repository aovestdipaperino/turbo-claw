use crate::claude::protocol::UiEvent;
use std::sync::mpsc;

pub fn new_bridge() -> (mpsc::Sender<UiEvent>, mpsc::Receiver<UiEvent>) {
    mpsc::channel()
}
