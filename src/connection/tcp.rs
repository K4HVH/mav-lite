use crate::connection::{ConnectionId, MessageReceiver, MessageSender};
use crate::mavlink::MavFrame;
use bytes::{Buf, BytesMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

pub struct TcpServer {
    listener: TcpListener,
    next_id: usize,
}

impl TcpServer {
    pub async fn bind(addr: &str) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        info!("TCP server listening on {}", addr);
        Ok(Self {
            listener,
            next_id: 0,
        })
    }

    pub async fn accept(
        &mut self,
        router_tx: mpsc::UnboundedSender<RouterMessage>,
    ) -> anyhow::Result<()> {
        let (stream, addr) = self.listener.accept().await?;
        let conn_id = ConnectionId::new_tcp(self.next_id);
        self.next_id += 1;

        info!("New TCP connection {} from {}", conn_id, addr);

        let (tx, rx) = mpsc::unbounded_channel();

        // Notify router of new connection
        router_tx.send(RouterMessage::NewConnection { conn_id, tx })?;

        // Spawn handler task
        tokio::spawn(async move {
            if let Err(e) = handle_tcp_connection(conn_id, stream, rx, router_tx.clone()).await {
                error!("TCP connection {} error: {}", conn_id, e);
            }
            // Notify router of disconnect
            let _ = router_tx.send(RouterMessage::Disconnect { conn_id });
            info!("TCP connection {} closed", conn_id);
        });

        Ok(())
    }
}

async fn handle_tcp_connection(
    conn_id: ConnectionId,
    mut stream: TcpStream,
    mut rx: MessageReceiver,
    router_tx: mpsc::UnboundedSender<RouterMessage>,
) -> anyhow::Result<()> {
    let (mut read_half, mut write_half) = stream.split();
    let mut read_buf = BytesMut::with_capacity(4096);

    loop {
        tokio::select! {
            // Read from TCP socket
            result = read_half.read_buf(&mut read_buf) => {
                match result {
                    Ok(0) => {
                        debug!("TCP connection {} EOF", conn_id);
                        break;
                    }
                    Ok(n) => {
                        debug!("TCP connection {} read {} bytes", conn_id, n);

                        // Parse MAVLink frames
                        while !read_buf.is_empty() {
                            match MavFrame::parse(&read_buf) {
                                Ok((frame, consumed)) => {
                                    debug!(
                                        "TCP {} received MAVLink msg: sysid={} compid={} msgid={}",
                                        conn_id, frame.sys_id(), frame.comp_id(), frame.msg_id()
                                    );

                                    // Send to router
                                    router_tx.send(RouterMessage::Frame {
                                        source: conn_id,
                                        frame,
                                    })?;

                                    read_buf.advance(consumed);
                                }
                                Err(crate::mavlink::ParseError::Incomplete(_, _)) => {
                                    // Need more data
                                    break;
                                }
                                Err(e) => {
                                    warn!("TCP {} parse error: {}, skipping byte", conn_id, e);
                                    read_buf.advance(1);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("TCP connection {} read error: {}", conn_id, e);
                        break;
                    }
                }
            }

            // Write to TCP socket
            Some(data) = rx.recv() => {
                write_half.write_all(&data).await?;
                debug!("TCP connection {} wrote {} bytes", conn_id, data.len());
            }
        }
    }

    Ok(())
}

pub enum RouterMessage {
    NewConnection {
        conn_id: ConnectionId,
        tx: MessageSender,
    },
    Disconnect {
        conn_id: ConnectionId,
    },
    Frame {
        source: ConnectionId,
        frame: MavFrame,
    },
}
