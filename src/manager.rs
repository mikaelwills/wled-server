use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

pub struct ConnectionManager {
    senders: HashMap<String, mpsc::Sender<Message>>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            senders: HashMap::new(),
        }
    }

    pub fn add_board(&mut self, board_id: String, sender: mpsc::Sender<Message>) {
        self.senders.insert(board_id, sender);
    }
    pub async fn send_command(&self, board_id: &str, msg: Message) {
        if let Some(sender) = self.senders.get(board_id) {
            let _ = sender.send(msg).await;
        }
    }
}
