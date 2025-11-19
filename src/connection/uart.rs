use crate::connection::{ConnectionId, MessageReceiver};
use crate::mavlink::MavFrame;
use bytes::{Buf, BytesMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio_serial::SerialPortBuilderExt;
use tracing::{debug, error, info, warn};

pub struct UartConnection {
    conn_id: ConnectionId,
    path: String,
    baud_rate: u32,
    name: Option<String>,
}

impl UartConnection {
    pub fn new(id: usize, path: String, baud_rate: u32, name: Option<String>) -> Self {
        Self {
            conn_id: ConnectionId::new_uart(id),
            path,
            baud_rate,
            name,
        }
    }

    pub async fn start(
        self,
        router_tx: mpsc::UnboundedSender<crate::connection::tcp::RouterMessage>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();

        // Notify router of new connection
        let _ = router_tx.send(crate::connection::tcp::RouterMessage::NewConnection {
            conn_id: self.conn_id,
            tx,
        });

        tokio::spawn(async move {
            self.run_with_reconnect(rx, router_tx).await;
        });
    }

    async fn run_with_reconnect(
        &self,
        mut rx: MessageReceiver,
        router_tx: mpsc::UnboundedSender<crate::connection::tcp::RouterMessage>,
    ) {
        let display_name = self
            .name
            .as_deref()
            .unwrap_or(&self.path);

        loop {
            info!(
                "UART connection {} ({}) attempting to open {}",
                self.conn_id, display_name, self.path
            );

            match tokio_serial::new(&self.path, self.baud_rate).open_native_async() {
                Ok(mut port) => {
                    info!(
                        "UART connection {} ({}) opened successfully",
                        self.conn_id, display_name
                    );

                    if let Err(e) = self
                        .handle_connection(&mut port, &mut rx, router_tx.clone())
                        .await
                    {
                        error!(
                            "UART connection {} ({}) error: {}",
                            self.conn_id, display_name, e
                        );
                    }

                    info!(
                        "UART connection {} ({}) disconnected, will retry in 5s",
                        self.conn_id, display_name
                    );
                }
                Err(e) => {
                    warn!(
                        "UART connection {} ({}) failed to open: {}, retrying in 5s",
                        self.conn_id, display_name, e
                    );
                }
            }

            sleep(Duration::from_secs(5)).await;
        }
    }

    async fn handle_connection(
        &self,
        port: &mut tokio_serial::SerialStream,
        rx: &mut MessageReceiver,
        router_tx: mpsc::UnboundedSender<crate::connection::tcp::RouterMessage>,
    ) -> anyhow::Result<()> {
        let mut read_buf = BytesMut::with_capacity(4096);

        loop {
            tokio::select! {
                // Read from UART
                result = port.read_buf(&mut read_buf) => {
                    match result {
                        Ok(0) => {
                            debug!("UART connection {} EOF", self.conn_id);
                            break;
                        }
                        Ok(n) => {
                            debug!("UART connection {} read {} bytes", self.conn_id, n);

                            // Parse MAVLink frames
                            while !read_buf.is_empty() {
                                match MavFrame::parse(&read_buf) {
                                    Ok((frame, consumed)) => {
                                        debug!(
                                            "UART {} received MAVLink msg: sysid={} compid={} msgid={}",
                                            self.conn_id, frame.sys_id(), frame.comp_id(), frame.msg_id()
                                        );

                                        // Send to router
                                        router_tx.send(crate::connection::tcp::RouterMessage::Frame {
                                            source: self.conn_id,
                                            frame,
                                        })?;

                                        read_buf.advance(consumed);
                                    }
                                    Err(crate::mavlink::ParseError::Incomplete(_, _)) => {
                                        // Need more data
                                        break;
                                    }
                                    Err(e) => {
                                        warn!("UART {} parse error: {}, skipping byte", self.conn_id, e);
                                        read_buf.advance(1);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("UART connection {} read error: {}", self.conn_id, e);
                            break;
                        }
                    }
                }

                // Write to UART
                Some(data) = rx.recv() => {
                    port.write_all(&data).await?;
                    debug!("UART connection {} wrote {} bytes", self.conn_id, data.len());
                }
            }
        }

        Ok(())
    }
}
