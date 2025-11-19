# mav-lite

A high-performance MAVLink router built in Rust, designed as a replacement for mavlink-router with focus on speed, efficiency, and reliability.

## Features

- **High Performance**: Handles 10+ simultaneous connections with minimal overhead
- **Dual Protocol Support**: Supports both MAVLink v1 and v2 seamlessly
- **Completely Transparent**: Messages pass through unmodified - perfect for custom/extended message sets
- **Zero-Copy Parsing**: Custom parser optimized for routing performance (no CRC validation for maximum compatibility)
- **Smart Connection Management**:
  - TCP support for GCS connections (e.g., QGroundControl)
  - UART support for drone connections
  - **Dynamic UART discovery** - automatically finds and connects to MAVLink ports
  - Automatic reconnection for UART devices
  - Dynamic sysid discovery for UART connections
- **Flexible Routing**: Configure routing rules to control message flow between connections
- **Robust**: Handles partial connections, disconnections, and reconnections gracefully

## Architecture

```
┌─────────────┐
│   GCS #1    │◄──┐
│ (QGC/TCP)   │   │
└─────────────┘   │
                  │
┌─────────────┐   │     ┌──────────────┐
│   GCS #2    │◄──┼────►│              │
│  (TCP)      │   │     │   mav-lite   │
└─────────────┘   │     │   Router     │
                  │     │              │
┌─────────────┐   │     └──────────────┘
│  Drone #1   │◄──┤              ▲
│  (UART)     │   │              │
└─────────────┘   │              │
                  │         Dynamic
┌─────────────┐   │        sysid
│  Drone #2   │◄──┤       discovery
│  (UART)     │   │              │
└─────────────┘   │              │
                  │              ▼
┌─────────────┐   │
│  Drone #N   │◄──┘
│  (UART)     │
└─────────────┘
```

## Quick Start

### Building

```bash
cargo build --release
```

The binary will be available at `target/release/mav-lite`.

### Configuration

Create a configuration file (e.g., `config.toml`):

```toml
[tcp]
listen_port = 5761
bind_addr = "0.0.0.0"

# Dynamic UART discovery - automatically finds MAVLink ports
[uart_discovery]
enabled = true
device_pattern = "/dev/ttyACM*"
baud_rate = 57600
detection_timeout_secs = 5
rescan_interval_secs = 30

# Routing rules
[routing]
allow_uart_to_uart = false
allow_tcp_to_tcp = true
allow_uart_to_tcp = true
allow_tcp_to_uart = true
```

**Or** use static UART configuration (disable `uart_discovery` first):

```toml
[uart_discovery]
enabled = false

[[uart]]
path = "/dev/ttyUSB0"
baud_rate = 57600
name = "Drone 1"
```

### Running

```bash
./target/release/mav-lite config.toml
```

Or for development with debug logging:

```bash
RUST_LOG=debug cargo run -- config.toml
```

## Configuration Reference

### TCP Configuration

- `listen_port`: Port to listen on for incoming GCS connections (default: 5760)
- `bind_addr`: Bind address (default: "0.0.0.0" for all interfaces)

### Dynamic UART Discovery

- `enabled`: Enable dynamic discovery
- `device_pattern`: Glob pattern (e.g., "/dev/ttyACM*")
- `baud_rate`: Baud rate for discovered devices
- `detection_timeout_secs`: Time to test each port for MAVLink traffic
- `rescan_interval_secs`: How often to scan for new devices

### Static UART Configuration

- `path`: Device path (e.g., "/dev/ttyUSB0")
- `baud_rate`: Baud rate
- `name`: Optional friendly name

### Routing Configuration

Control message flow between connection types:

- `allow_uart_to_uart`: Allow drone-to-drone communication (default: false)
- `allow_tcp_to_tcp`: Allow GCS-to-GCS communication (default: true)
- `allow_uart_to_tcp`: Allow drone-to-GCS communication (default: true)
- `allow_tcp_to_uart`: Allow GCS-to-drone communication (default: true)

## Performance Characteristics

- **Zero-Copy Parsing**: MAVLink frames are parsed without unnecessary allocations
- **Async I/O**: Built on Tokio for efficient concurrent connection handling
- **Lock-Free Channels**: Uses MPSC channels for fast inter-task communication
- **Compile-Time CRC Table**: CRC validation uses a pre-computed lookup table

## Use Cases

### Multi-Drone Setup

Connect multiple drones via UART to a single GCS instance, preventing drones from communicating with each other:

```toml
[routing]
allow_uart_to_uart = false
allow_uart_to_tcp = true
allow_tcp_to_uart = true
```

### Multiple GCS Instances

Allow multiple GCS applications to monitor the same drone(s):

```toml
[routing]
allow_tcp_to_tcp = true
allow_tcp_to_uart = true
allow_uart_to_tcp = true
```

## UART Modes

**Dynamic Discovery**: Automatically scans `/dev/ttyACM*` (or pattern) and tests each port for MAVLink traffic. Only connects to ports with valid MAVLink data.

**Static Config**: Manually specify device paths in config.

## Logging

Control log level with the `RUST_LOG` environment variable:

```bash
# All debug logs
RUST_LOG=debug mav-lite config.toml

# Only info and above
RUST_LOG=info mav-lite config.toml

# Specific module logging
RUST_LOG=mav_lite::router=debug,info mav-lite config.toml
```

## Comparison to mavlink-router

| Feature | mav-lite | mavlink-router |
|---------|----------|----------------|
| Language | Rust | C++ |
| MAVLink Version | v1 and v2 | v1 and v2 |
| Transparency | Full (no CRC check) | Standard |
| Performance | Optimized for speed | Good |
| Memory Safety | Guaranteed by Rust | Manual |
| Config Format | TOML | Command-line args |
| Auto-Reconnect | Yes (UART) | Limited |
| Dynamic sysid | Yes | Yes |
| Custom Messages | Full support | Limited |

## License

See LICENSE file for details.

## Contributing

Contributions welcome! Please ensure code passes `cargo clippy` and `cargo test` before submitting PRs.
