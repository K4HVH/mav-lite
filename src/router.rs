use crate::config::RoutingConfig;
use crate::connection::tcp::RouterMessage;
use crate::connection::{ConnectionId, ConnectionType, MessageSender};
use crate::mavlink::MavFrame;
use crate::metrics::Metrics;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

pub struct Router {
    config: RoutingConfig,
    connections: HashMap<ConnectionId, Connection>,
    sysid_map: HashMap<u8, ConnectionId>,
    metrics: Metrics,
}

struct Connection {
    tx: MessageSender,
    conn_type: ConnectionType,
    sysid: Option<u8>,
}

impl Router {
    pub fn new(config: RoutingConfig, metrics: Metrics) -> Self {
        Self {
            config,
            connections: HashMap::new(),
            sysid_map: HashMap::new(),
            metrics,
        }
    }

    pub async fn run(mut self, mut rx: mpsc::UnboundedReceiver<RouterMessage>) {
        info!("Router started");

        while let Some(msg) = rx.recv().await {
            match msg {
                RouterMessage::NewConnection { conn_id, tx } => {
                    self.handle_new_connection(conn_id, tx);
                }
                RouterMessage::Disconnect { conn_id } => {
                    self.handle_disconnect(conn_id);
                }
                RouterMessage::Frame { source, frame } => {
                    self.route_frame(source, frame);
                }
            }
        }

        info!("Router stopped");
    }

    fn handle_new_connection(&mut self, conn_id: ConnectionId, tx: MessageSender) {
        info!("Router: new connection {}", conn_id);
        self.connections.insert(
            conn_id,
            Connection {
                tx,
                conn_type: conn_id.conn_type,
                sysid: None,
            },
        );
    }

    fn handle_disconnect(&mut self, conn_id: ConnectionId) {
        info!("Router: connection {} disconnected", conn_id);

        // Remove from connections
        if let Some(conn) = self.connections.remove(&conn_id) {
            // Remove from sysid map if it had a sysid
            if let Some(sysid) = conn.sysid {
                self.sysid_map.remove(&sysid);
                info!("Router: removed sysid {} mapping", sysid);
            }
        }
    }

    fn route_frame(&mut self, source: ConnectionId, frame: MavFrame) {
        let sysid = frame.sys_id();

        // Record received message
        self.metrics.record_received();

        // Update sysid mapping for UART connections
        if source.conn_type == ConnectionType::Uart {
            if let Some(conn) = self.connections.get_mut(&source) {
                if conn.sysid.is_none() {
                    conn.sysid = Some(sysid);
                    self.sysid_map.insert(sysid, source);
                    info!(
                        "Router: discovered sysid {} on connection {}",
                        sysid, source
                    );
                }
            }
        }

        debug!(
            "Routing frame from {} (sysid={}, compid={}, msgid={})",
            source,
            sysid,
            frame.comp_id(),
            frame.msg_id()
        );

        // Route to all eligible connections
        let frame_bytes = bytes::Bytes::copy_from_slice(frame.as_bytes());
        let frame_len = frame_bytes.len();

        for (&dest_id, dest_conn) in &self.connections {
            // Don't send back to source
            if dest_id == source {
                continue;
            }

            // Check routing rules
            if !self.should_route(source.conn_type, dest_conn.conn_type) {
                continue;
            }

            // Send the frame with backpressure detection
            match dest_conn.tx.send(frame_bytes.clone()) {
                Ok(_) => {
                    self.metrics.record_routed(frame_len);
                    debug!("Routed frame from {} to {}", source, dest_id);
                }
                Err(e) => {
                    self.metrics.record_dropped();
                    warn!(
                        "BACKPRESSURE: Failed to send to {} (channel full): {}",
                        dest_id, e
                    );
                }
            }
        }
    }

    fn should_route(&self, src_type: ConnectionType, dst_type: ConnectionType) -> bool {
        match (src_type, dst_type) {
            (ConnectionType::Uart, ConnectionType::Uart) => self.config.allow_uart_to_uart,
            (ConnectionType::Uart, ConnectionType::Tcp) => self.config.allow_uart_to_tcp,
            (ConnectionType::Tcp, ConnectionType::Uart) => self.config.allow_tcp_to_uart,
            (ConnectionType::Tcp, ConnectionType::Tcp) => self.config.allow_tcp_to_tcp,
        }
    }

    #[allow(dead_code)]
    pub fn get_connection_by_sysid(&self, sysid: u8) -> Option<ConnectionId> {
        self.sysid_map.get(&sysid).copied()
    }

    #[allow(dead_code)]
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    #[allow(dead_code)]
    pub fn tcp_connection_count(&self) -> usize {
        self.connections
            .values()
            .filter(|c| c.conn_type == ConnectionType::Tcp)
            .count()
    }

    #[allow(dead_code)]
    pub fn uart_connection_count(&self) -> usize {
        self.connections
            .values()
            .filter(|c| c.conn_type == ConnectionType::Uart)
            .count()
    }
}
