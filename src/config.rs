use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// TCP endpoints for GCS connections
    #[serde(default)]
    pub tcp: TcpConfig,

    /// UART endpoints for drone connections
    #[serde(default)]
    pub uart: Vec<UartConfig>,

    /// Dynamic UART discovery settings
    #[serde(default)]
    pub uart_discovery: UartDiscoveryConfig,

    /// Routing rules
    #[serde(default)]
    pub routing: RoutingConfig,

    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Performance stats logging interval in seconds (0 = disabled)
    #[serde(default = "default_stats_interval")]
    pub stats_interval_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TcpConfig {
    /// Port to listen on for incoming GCS connections
    #[serde(default = "default_tcp_port")]
    pub listen_port: u16,

    /// Bind address
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
}

impl Default for TcpConfig {
    fn default() -> Self {
        Self {
            listen_port: default_tcp_port(),
            bind_addr: default_bind_addr(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UartConfig {
    /// Path to the serial device (e.g., /dev/ttyUSB0)
    pub path: String,

    /// Baud rate
    #[serde(default = "default_baud_rate")]
    pub baud_rate: u32,

    /// Optional friendly name for logging
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UartDiscoveryConfig {
    /// Enable dynamic UART discovery
    #[serde(default)]
    pub enabled: bool,

    /// Device pattern to scan (e.g., "/dev/ttyACM*", "/dev/ttyUSB*")
    #[serde(default = "default_device_pattern")]
    pub device_pattern: String,

    /// Baud rate for discovered devices
    #[serde(default = "default_baud_rate")]
    pub baud_rate: u32,

    /// Timeout in seconds to detect MAVLink traffic on a port
    #[serde(default = "default_detection_timeout")]
    pub detection_timeout_secs: u64,

    /// Interval in seconds to rescan for new devices
    #[serde(default = "default_rescan_interval")]
    pub rescan_interval_secs: u64,
}

impl Default for UartDiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            device_pattern: default_device_pattern(),
            baud_rate: default_baud_rate(),
            detection_timeout_secs: default_detection_timeout(),
            rescan_interval_secs: default_rescan_interval(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoutingConfig {
    /// Allow UART-to-UART routing (drone-to-drone)
    #[serde(default)]
    pub allow_uart_to_uart: bool,

    /// Allow TCP-to-TCP routing (GCS-to-GCS)
    #[serde(default = "default_true")]
    pub allow_tcp_to_tcp: bool,

    /// Allow UART-to-TCP routing (drone-to-GCS)
    #[serde(default = "default_true")]
    pub allow_uart_to_tcp: bool,

    /// Allow TCP-to-UART routing (GCS-to-drone)
    #[serde(default = "default_true")]
    pub allow_tcp_to_uart: bool,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            allow_uart_to_uart: false,
            allow_tcp_to_tcp: true,
            allow_uart_to_tcp: true,
            allow_tcp_to_uart: true,
        }
    }
}

fn default_tcp_port() -> u16 {
    5760
}

fn default_bind_addr() -> String {
    "0.0.0.0".to_string()
}

fn default_baud_rate() -> u32 {
    57600
}

fn default_true() -> bool {
    true
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_stats_interval() -> u64 {
    30 // Log stats every 30 seconds by default
}

fn default_device_pattern() -> String {
    "/dev/ttyACM*".to_string()
}

fn default_detection_timeout() -> u64 {
    5 // Wait up to 5 seconds for MAVLink traffic
}

fn default_rescan_interval() -> u64 {
    30 // Rescan for new devices every 30 seconds
}

impl Config {
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn example() -> Self {
        Self {
            tcp: TcpConfig::default(),
            uart: vec![
                UartConfig {
                    path: "/dev/ttyUSB0".to_string(),
                    baud_rate: 57600,
                    name: Some("Drone 1".to_string()),
                },
                UartConfig {
                    path: "/dev/ttyUSB1".to_string(),
                    baud_rate: 57600,
                    name: Some("Drone 2".to_string()),
                },
            ],
            uart_discovery: UartDiscoveryConfig::default(),
            routing: RoutingConfig::default(),
            log_level: default_log_level(),
            stats_interval_secs: default_stats_interval(),
        }
    }
}
