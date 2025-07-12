# Renews

Renews is a modern, lightweight NNTP (Network News Transfer Protocol) server implemented in Rust. It provides a complete newsgroup server solution with a focus on performance, reliability, and ease of administration.

## Overview

Renews implements the NNTP protocol as defined in RFC 3977, storing articles in a database and supporting configurable newsgroups with flexible retention policies. The server is designed for both standalone operation and distributed newsgroup synchronization with peer servers.

## Features

- **Full NNTP Protocol Support** - RFC 3977 compliant with standard commands (ARTICLE, HEAD, BODY, POST, etc.)
- **Multiple Storage Backends** - SQLite (default) and PostgreSQL support  
- **TLS/SSL Support** - Secure NNTP over TLS with configurable certificates
- **Authentication System** - User authentication with admin and moderator roles
- **Moderated Groups** - Support for moderated newsgroups with approval workflows
- **Peer Synchronization** - Distribute articles across multiple server instances
- **WebSocket Bridge** - Optional WebSocket support for web-based clients
- **Flexible Retention** - Configurable article retention policies per newsgroup
- **Article Size Limits** - Configurable maximum article sizes per group
- **Streaming Mode** - RFC 4644 streaming feeds support (CHECK/TAKETHIS commands)
- **Control Messages** - Support for newgroup/rmgroup/cancel control messages
- **Administrative CLI** - Built-in commands for user and group management
- **Hot Configuration Reload** - Runtime configuration updates via SIGHUP
- **Systemd Socket Activation** - Run as non-root while listening on privileged ports

## Building

### Prerequisites

- Rust toolchain (latest stable recommended)
- SQLite development libraries (for default build)
- PostgreSQL development libraries (if using PostgreSQL backend)

### Basic Build

```bash
cargo build --release
```

This produces the `renews` binary in `target/release/` with default features (SQLite storage).

### Build with Features

```bash
# Build with WebSocket support for web clients
cargo build --release --features websocket

# Build with PostgreSQL backend support  
cargo build --release --features postgres

# Build with all features
cargo build --release --features websocket,postgres
```

### Available Features

- `websocket` - Enables WebSocket bridge for web-based NNTP clients
- `postgres` - Adds PostgreSQL storage backend support alongside SQLite

### Running Tests

```bash
# Run all tests
cargo test

# Run with specific features
cargo test --features websocket,postgres
```

## Quick Start

### Minimal Configuration

Create a basic configuration file (`renews.toml`):

```toml
addr = ":119"
site_name = "news.example.com" 
db_path = "sqlite:///var/lib/renews/news.db"
auth_db_path = "sqlite:///var/lib/renews/auth.db"
```

### Initialize and Run

```bash
# Initialize databases
./renews --init --config renews.toml

# Start the server
./renews --config renews.toml
```

## Configuration

Configuration is loaded from the file specified with `--config`. When the
`RENEWS_CONFIG` environment variable is set it is used as the default,
otherwise `/etc/renews.toml` is assumed. The
following keys are recognised:

- `addr` - listen address for plain NNTP connections. If the host portion is
  omitted the server listens on all interfaces. For systemd socket activation,
  use `systemd://socket_name` format (e.g., `systemd://renews-nntp.socket`).
- `site_name` - hostname advertised by the server. Defaults to the `HOSTNAME`
  environment variable or `localhost` when unset.
- `db_path` - database connection string for storing articles. Defaults to
  `sqlite:///var/lib/renews/news.db`.
- `auth_db_path` - authentication database connection string such as
  `sqlite:///var/lib/renews/auth.db` or `postgres://user:pass@127.0.0.1:5432`.
  When a PostgreSQL URI includes a username and password these are used for
  authentication. Defaults to
  `sqlite:///var/lib/renews/auth.db` when unset.
- `peer_db_path` - connection string for the peer state database. Defaults to
  `sqlite:///var/lib/renews/peers.db`.
- `peer_sync_secs` - default seconds between synchronizing with peers.
- `idle_timeout_secs` - idle timeout in seconds for client connections. Defaults to 600 (10 minutes).
- `peers` - list of peer entries with `sitename`, optional `sync_interval_secs` and `patterns` controlling which groups are exchanged. The `sitename` may include credentials in the form `user:pass@host:port` which are used for `AUTHINFO` when connecting.
- `tls_addr` - optional listen address for NNTP over TLS. Omitting the host
  portion listens on all interfaces. For systemd socket activation,
  use `systemd://socket_name` format (e.g., `systemd://renews-nntps.socket`).
- `tls_cert` - path to the TLS certificate in PEM format.
- `tls_key` - path to the TLS private key in PEM format.
- `ws_addr` - optional listen address for the WebSocket bridge (requires the
  `websocket` feature). Omitting the host portion listens on all interfaces.
- `default_retention_days` - default number of days to keep articles.
- `default_max_article_bytes` - default maximum article size in bytes. A `K`,
  `M` or `G` suffix may be used to specify kilobytes, megabytes or gigabytes.
- `pgp_key_servers` - list of PGP key discovery servers used for looking up public keys
  when verifying signed control messages. Defaults to well-known public key servers
  if not specified.
- `group_settings` - list of per-group rules which can match a `group` exactly or a
  `pattern` using wildmat syntax to override retention and size defaults.

Values inside the configuration may reference environment variables or other files.
The pattern `$ENV{VAR}` is replaced by the value of the `VAR` environment variable
and `$FILE{path}` is replaced with the contents of the file at `path` before the
file is parsed.

An example configuration is provided in the repository:

```toml
addr = ":119"
site_name = "example.com"
db_path = "sqlite:///var/lib/renews/news.db"
auth_db_path = "sqlite:///var/lib/renews/auth.db"
peer_db_path = "sqlite:///var/lib/renews/peers.db"
peer_sync_secs = 3600
idle_timeout_secs = 600
tls_addr = ":563"
tls_cert = "cert.pem"
tls_key = "key.pem"
ws_addr = ":8080"
default_retention_days = 30
default_max_article_bytes = "1M"

pgp_key_servers = [
    "hkps://keys.openpgp.org/pks/lookup?op=get&search=<email>",
    "hkps://pgp.mit.edu/pks/lookup?op=get&search=<email>",
    "hkps://keyserver.ubuntu.com/pks/lookup?op=get&search=<email>"
]

[[group_settings]]
pattern = "short.*"
retention_days = 7

[[group_settings]]
group = "misc.news"
retention_days = 60
max_article_bytes = "2M"

[[peers]]
sitename = "peeruser:peerpass@peer.example.com"
patterns = ["*"]
sync_interval_secs = 3600
```

`tls_addr`, `tls_cert` and `tls_key` must all be set for TLS support to be
enabled. The WebSocket bridge is started when `ws_addr` is set and the crate is
compiled with the `websocket` feature.

## Deployment with systemd

For production deployment, Renews supports both traditional direct binding and systemd socket activation.

### Traditional Deployment

By default the service reads `/etc/renews.toml`. A different path can be
provided with `--config`. A simple systemd unit may look like this:

```ini
[Unit]
Description=Renews NNTP server
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/renews --config /opt/renews/config.toml
ExecReload=/bin/kill -HUP $MAINPID
WorkingDirectory=/opt/renews
Restart=on-failure
User=renews
Group=renews

[Install]
WantedBy=multi-user.target
```

### Systemd Socket Activation (Recommended)

Socket activation allows Renews to listen on privileged ports without running as root:

```ini
# /etc/systemd/system/renews.service
[Unit]
Description=Renews NNTP server
After=network.target
Requires=renews-nntp.socket
Wants=renews-nntps.socket

[Service]
Type=simple
ExecStart=/usr/local/bin/renews --config /opt/renews/config.toml
ExecReload=/bin/kill -HUP $MAINPID
WorkingDirectory=/opt/renews
Restart=on-failure
User=renews
Group=renews

[Install]
WantedBy=multi-user.target
```

With corresponding socket files for NNTP (port 119) and NNTPS (port 563). Configuration uses `systemd://` URLs:

```toml
addr = "systemd://renews-nntp.socket"
tls_addr = "systemd://renews-nntps.socket"
```

Install the files and run `systemctl enable --now renews-nntp.socket renews-nntps.socket renews.service`
to start the server at boot.

For complete setup instructions, see the [Deployment Guide](docs/deployment.md).

Sending `SIGHUP` to the process (for example with `systemctl reload`) reloads
the configuration. Retention, group, and TLS settings are updated at runtime;
the listening ports and database paths remain unchanged.


## Administration

Use the `admin` subcommand to manage newsgroups and users without starting the
server. These commands read the same configuration file as the server itself.

```bash
# configuration is supplied via the environment
export RENEWS_CONFIG=/opt/renews/config.toml

# add a newsgroup
renews admin add-group rust.news --moderated

# remove a user
renews admin remove-user alice

# grant admin privileges
renews admin add-admin alice

# revoke admin privileges
renews admin remove-admin alice

# add moderator permissions
renews admin add-moderator alice 'rust.*'

# remove moderator permissions
renews admin remove-moderator alice 'rust.*'
```

Use `--init` to create the article, authentication and peer state databases
without starting the server:

```bash
renews --init --config /opt/renews/config.toml
```

## Documentation

For detailed information about Renews architecture, configuration, and deployment:

- **[Manual Page](man/renews.1)** - Complete command line and configuration reference
- **[Architecture Guide](docs/architecture.md)** - System design and component overview
- **[Configuration Guide](docs/configuration.md)** - Complete configuration reference  
- **[Deployment Guide](docs/deployment.md)** - Installation and production deployment
- **[Task Interactions](docs/task-interactions.md)** - System flows and task coordination

The manual page can be viewed with: `man ./man/renews.1`
