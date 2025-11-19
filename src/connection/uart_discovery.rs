use crate::config::UartDiscoveryConfig;
use crate::connection::uart::UartConnection;
use crate::mavlink::MavFrame;
use bytes::{Buf, BytesMut};
use std::collections::HashSet;
use std::path::PathBuf;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout, Duration};
use tokio_serial::SerialPortBuilderExt;
use tracing::{debug, error, info, warn};

pub struct UartDiscovery {
    config: UartDiscoveryConfig,
    active_devices: HashSet<PathBuf>,
    next_uart_id: usize,
}

impl UartDiscovery {
    pub fn new(config: UartDiscoveryConfig, starting_id: usize) -> Self {
        Self {
            config,
            active_devices: HashSet::new(),
            next_uart_id: starting_id,
        }
    }

    pub async fn run(
        mut self,
        router_tx: mpsc::UnboundedSender<crate::connection::tcp::RouterMessage>,
    ) {
        info!("UART discovery started");
        info!(
            "  Device pattern: {}",
            self.config.device_pattern
        );
        info!("  Baud rate: {}", self.config.baud_rate);
        info!(
            "  Detection timeout: {}s",
            self.config.detection_timeout_secs
        );
        info!(
            "  Rescan interval: {}s",
            self.config.rescan_interval_secs
        );

        loop {
            self.scan_and_connect(&router_tx).await;
            sleep(Duration::from_secs(self.config.rescan_interval_secs)).await;
        }
    }

    async fn scan_and_connect(
        &mut self,
        router_tx: &mpsc::UnboundedSender<crate::connection::tcp::RouterMessage>,
    ) {
        info!("Scanning for UART devices matching {}", self.config.device_pattern);

        let devices = match self.enumerate_devices().await {
            Ok(devices) => devices,
            Err(e) => {
                error!("Failed to enumerate devices: {}", e);
                return;
            }
        };

        info!("Found {} potential device(s)", devices.len());

        for device_path in devices {
            // Skip if already active
            if self.active_devices.contains(&device_path) {
                debug!("Device {:?} already active, skipping", device_path);
                continue;
            }

            // Test if device has MAVLink traffic
            info!("Testing device {:?} for MAVLink traffic...", device_path);
            match self.test_for_mavlink(&device_path).await {
                Ok(true) => {
                    info!(
                        "MAVLink traffic detected on {:?}, connecting...",
                        device_path
                    );

                    let uart_id = self.next_uart_id;
                    self.next_uart_id += 1;

                    let path_str = device_path.to_string_lossy().to_string();
                    let name = format!("Auto-discovered: {}", path_str);

                    let uart_conn = UartConnection::new(
                        uart_id,
                        path_str.clone(),
                        self.config.baud_rate,
                        Some(name),
                    );

                    uart_conn.start(router_tx.clone()).await;
                    self.active_devices.insert(device_path.clone());

                    info!(
                        "Started UART connection {} for device {:?}",
                        uart_id, device_path
                    );
                }
                Ok(false) => {
                    debug!("No MAVLink traffic detected on {:?}", device_path);
                }
                Err(e) => {
                    warn!("Failed to test device {:?}: {}", device_path, e);
                }
            }
        }
    }

    async fn enumerate_devices(&self) -> anyhow::Result<Vec<PathBuf>> {
        let pattern = &self.config.device_pattern;

        // Use glob to find matching devices
        let paths: Vec<PathBuf> = glob::glob(pattern)?
            .filter_map(Result::ok)
            .collect();

        Ok(paths)
    }

    async fn test_for_mavlink(&self, device_path: &PathBuf) -> anyhow::Result<bool> {
        let path_str = device_path.to_string_lossy().to_string();

        // Try to open the device
        let mut port = match tokio_serial::new(&path_str, self.config.baud_rate)
            .open_native_async()
        {
            Ok(port) => port,
            Err(e) => {
                debug!("Failed to open {:?}: {}", device_path, e);
                return Ok(false);
            }
        };

        // Read data with timeout
        let mut read_buf = BytesMut::with_capacity(4096);
        let detection_duration = Duration::from_secs(self.config.detection_timeout_secs);

        let result = timeout(detection_duration, async {
            loop {
                match port.read_buf(&mut read_buf).await {
                    Ok(0) => {
                        // EOF - device disconnected
                        return false;
                    }
                    Ok(_n) => {
                        // Try to parse MAVLink frames
                        while !read_buf.is_empty() {
                            match MavFrame::parse(&read_buf) {
                                Ok((frame, _consumed)) => {
                                    debug!(
                                        "Detected MAVLink frame on {:?}: sysid={} msgid={}",
                                        device_path,
                                        frame.sys_id(),
                                        frame.msg_id()
                                    );
                                    return true;
                                }
                                Err(crate::mavlink::ParseError::Incomplete(_, _)) => {
                                    // Need more data
                                    break;
                                }
                                Err(_) => {
                                    // Invalid data, skip one byte
                                    read_buf.advance(1);
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Read error
                        return false;
                    }
                }
            }
        })
        .await;

        match result {
            Ok(has_mavlink) => Ok(has_mavlink),
            Err(_) => {
                // Timeout - no MAVLink detected
                debug!("Timeout waiting for MAVLink on {:?}", device_path);
                Ok(false)
            }
        }
    }
}
