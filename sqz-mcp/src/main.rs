// sqz-mcp binary entry point
// Compiled Rust binary (not Node.js) — Requirement 2.4

use sqz_mcp::{McpServer, McpTransport};
use std::path::PathBuf;

/// Default preset directory: `~/.sqz/presets`. Falls back to `.` (the
/// process CWD) if `$HOME`/`$USERPROFILE` can't be resolved, matching the
/// same fallback sqz_engine uses for the session store.
fn default_preset_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    match home {
        Some(h) => h.join(".sqz").join("presets"),
        None => PathBuf::from("."),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Parse --transport and --port flags.
    let mut transport = McpTransport::Stdio;
    let mut preset_dir = default_preset_dir();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--transport" | "-t" => {
                i += 1;
                if i < args.len() {
                    match args[i].as_str() {
                        "sse" => {
                            // Default SSE port; may be overridden by --port.
                            transport = McpTransport::Sse { port: 3000 };
                        }
                        _ => {
                            // "stdio" or anything unrecognized: default to stdio.
                            transport = McpTransport::Stdio;
                        }
                    }
                }
            }
            "--port" | "-p" => {
                i += 1;
                if i < args.len() {
                    let port: u16 = args[i].parse().unwrap_or(3000);
                    transport = McpTransport::Sse { port };
                }
            }
            "--preset-dir" | "-d" => {
                i += 1;
                if i < args.len() {
                    preset_dir = PathBuf::from(&args[i]);
                }
            }
            "--help" | "-h" => {
                eprintln!(
                    "Usage: sqz-mcp [--transport stdio|sse] [--port PORT] [--preset-dir DIR]"
                );
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }

    // Make sure the preset directory exists so hot-reload watching (and
    // dropping a preset in later) works out of the box. Not fatal if this
    // fails — `McpServer::new` and `watch_presets` both degrade gracefully
    // when the directory is missing or unwatchable.
    let _ = std::fs::create_dir_all(&preset_dir);

    let server = McpServer::new(&preset_dir).unwrap_or_else(|e| {
        eprintln!("[sqz-mcp] failed to initialize server: {e}");
        std::process::exit(1);
    });

    // Start preset hot-reload watcher (keep handle alive for the process lifetime).
    // If the watcher fails (e.g. invalid/unwatchable preset dir), degrade gracefully —
    // the server still works, just without hot-reload.
    let _watcher = match server.watch_presets() {
        Ok(w) => Some(w),
        Err(e) => {
            eprintln!("[sqz-mcp] warning: preset watcher failed to start: {e}");
            eprintln!("[sqz-mcp] continuing without hot-reload support");
            None
        }
    };

    if let Err(e) = server.start(transport) {
        eprintln!("[sqz-mcp] server error: {e}");
        std::process::exit(1);
    }
}
