pub mod tcp;
pub mod uart;
pub mod uart_discovery;

use std::fmt;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectionType {
    Tcp,
    Uart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId {
    pub conn_type: ConnectionType,
    pub id: usize,
}

impl ConnectionId {
    pub fn new_tcp(id: usize) -> Self {
        Self {
            conn_type: ConnectionType::Tcp,
            id,
        }
    }

    pub fn new_uart(id: usize) -> Self {
        Self {
            conn_type: ConnectionType::Uart,
            id,
        }
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.conn_type {
            ConnectionType::Tcp => write!(f, "TCP-{}", self.id),
            ConnectionType::Uart => write!(f, "UART-{}", self.id),
        }
    }
}

pub type MessageSender = mpsc::UnboundedSender<bytes::Bytes>;
pub type MessageReceiver = mpsc::UnboundedReceiver<bytes::Bytes>;
