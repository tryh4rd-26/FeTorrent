use anyhow::Context;
use clap::{Parser, Subcommand};
use colored::Colorize;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::NOTHING, Attribute, Cell, Color, Table};
use fetorrent_core::config::FeConfig;
use fetorrent_core::models::{GlobalStats, TorrentInfo, TorrentStatus};
use reqwest::{multipart, Client};
use serde::Deserialize;
use std::io::{self, BufRead, IsTerminal, Write};
use std::process::{Command, Stdio};
use std::time::Duration;
use tracing::warn;

#[derive(Debug, Deserialize)]
struct SelectDirectoryResponse {
    path: String,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// API URL (default: http://127.0.0.1:6977/api/v1)
    #[arg(
        short,
        long,
        global = true,
        default_value = "http://127.0.0.1:6977/api/v1"
    )]
    url: String,

    /// Output raw JSON instead of pretty tables
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Add a new torrent (magnet or file path)
    Add {
        target: String,
        #[arg(short, long)]
        dir: Option<String>,
    },
    /// List all torrents
    List {
        #[arg(short, long)]
        watch: bool,
    },
    /// Info on a specific torrent
    Info { id: usize },
    /// Pause a torrent
    Pause { id: usize },
    /// Resume a torrent
    Resume { id: usize },
    /// Remove a torrent
    Remove { id: usize },
    /// Show global statistics
    Stats,
    /// Launch the daemon process in the background
    #[command(alias = "start")]
    Run {
        #[arg(short, long)]
        port: Option<u16>,

        /// Run in foreground instead of daemonizing
        #[arg(short, long)]
        foreground: bool,
    },
    /// View daemon logs
    Log {
        /// Follow log output in real-time
        #[arg(short, long)]
        live: bool,

        /// Number of recent lines to show
        #[arg(short, long, default_value = "50")]
        lines: usize,
    },
    /// Terminate the background daemon process
    Kill,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Only init tracing for CLI if needed, usually we want clean output
    if std::env::var("RUST_LOG").is_ok() {
        init_logging();
    }

    let cli = Cli::parse();
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(15))
        .build()
        .context("failed to initialize HTTP client")?;

    // Banner logic: Only print for Run/Start command or explicitly requested
    if should_print_banner(&cli.command, cli.json) {
        print_banner();
    }

    if !matches!(cli.command, Commands::Run { .. } | Commands::Log { .. }) {
        ensure_daemon_available(&cli.url).await?;
    }

    match &cli.command {
        Commands::Add { target, dir } => {
            let chosen_dir = resolve_add_directory(&client, &cli.url, dir.clone()).await?;

            let res = if target.starts_with("magnet:") {
                let mut form = multipart::Form::new().text("magnet", target.clone());
                if let Some(d) = &chosen_dir {
                    form = form.text("dir", d.clone());
                }
                send_request(
                    client
                        .post(format!("{}/torrents/add", cli.url))
                        .multipart(form),
                    "Add torrent (magnet)",
                    &cli.url,
                    true,
                )
                .await?
            } else {
                let file_path = std::path::Path::new(target);
                let bytes = std::fs::read(file_path)?;
                let part = multipart::Part::bytes(bytes)
                    .file_name(file_path.file_name().unwrap().to_string_lossy().to_string());
                let mut form = multipart::Form::new().part("file", part);
                if let Some(d) = &chosen_dir {
                    form = form.text("dir", d.clone());
                }
                send_request(
                    client
                        .post(format!("{}/torrents/add", cli.url))
                        .multipart(form),
                    "Add torrent (file)",
                    &cli.url,
                    true,
                )
                .await?
            };

            if cli.json {
                println!("{}", res.text().await?);
            } else if res.status().is_success() {
                println!("{}", "Successfully added torrent".green().bold());
            } else {
                eprintln!("{}", format!("Failed to add: {}", res.text().await?).red());
            }
        }
        Commands::List { watch } => {
            if *watch && !cli.json {
                loop {
                    print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
                    let res = send_request(
                        client.get(format!("{}/torrents", cli.url)),
                        "List torrents",
                        &cli.url,
                        true,
                    )
                    .await?;
                    let torrents: Vec<TorrentInfo> = res.json().await?;
                    print_torrents(&torrents);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            } else {
                let res = send_request(
                    client.get(format!("{}/torrents", cli.url)),
                    "List torrents",
                    &cli.url,
                    true,
                )
                .await?;
                if cli.json {
                    println!("{}", res.text().await?);
                } else {
                    let torrents: Vec<TorrentInfo> = res.json().await?;
                    print_torrents(&torrents);
                }
            }
        }
        Commands::Info { id } => {
            let res = send_request(
                client.get(format!("{}/torrents/{}", cli.url, id)),
                "Get torrent info",
                &cli.url,
                true,
            )
            .await?;
            println!("{}", res.text().await?);
        }
        Commands::Pause { id } => {
            let res = send_request(
                client.post(format!("{}/torrents/{}/pause", cli.url, id)),
                "Pause torrent",
                &cli.url,
                true,
            )
            .await?;
            cmd_result("Pause", res, cli.json).await?;
        }
        Commands::Resume { id } => {
            let res = send_request(
                client.post(format!("{}/torrents/{}/resume", cli.url, id)),
                "Resume torrent",
                &cli.url,
                true,
            )
            .await?;
            cmd_result("Resume", res, cli.json).await?;
        }
        Commands::Remove { id } => {
            let res = send_request(
                client.delete(format!("{}/torrents/{}", cli.url, id)),
                "Remove torrent",
                &cli.url,
                true,
            )
            .await?;
            cmd_result("Remove", res, cli.json).await?;
        }
        Commands::Stats => {
            let res = send_request(
                client.get(format!("{}/stats", cli.url)),
                "Get stats",
                &cli.url,
                true,
            )
            .await?;
            if cli.json {
                println!("{}", res.text().await?);
            } else {
                let stats: GlobalStats = res.json().await?;
                println!("DL Speed: {}/s", format_bytes(stats.dl_speed));
                println!("UL Speed: {}/s", format_bytes(stats.ul_speed));
                println!("Active: {}", stats.active_torrents);
            }
        }
        Commands::Run { port, foreground } => {
            if *foreground {
                println!("Starting daemon in foreground...");
                let mut cmd = Command::new("fetorrent-daemon");
                if let Some(p) = port {
                    cmd.arg("--port").arg(p.to_string());
                }
                let status = match cmd.status() {
                    Ok(s) => s,
                    Err(_) => {
                        let mut cargo_cmd = Command::new("cargo");
                        cargo_cmd.arg("run").arg("-p").arg("fetorrent-daemon");
                        if let Some(p) = port {
                            cargo_cmd.arg("--").arg("--port").arg(p.to_string());
                        }
                        cargo_cmd.status()?
                    }
                };
                if !status.success() {
                    std::process::exit(status.code().unwrap_or(1));
                }
            } else {
                println!("Starting FeTorrent daemon in background...");
                start_daemon_for_url(&cli.url)?;
                println!("{}", "Daemon initialized.".green().bold());

                let ui_url = cli.url.replace("/api/v1", "");
                println!("Web UI: {}", ui_url.bright_cyan().underline());
                let _ = webbrowser::open(&ui_url);

                println!("Use 'fetorrent log --live' to see daemon output.");
            }
        }
        Commands::Log { live, lines } => {
            let log_path = FeConfig::resolve_log_path();
            if !log_path.exists() {
                anyhow::bail!(
                    "Log file not found at {}. Is the daemon running?",
                    log_path.display()
                );
            }

            if *live {
                let file = std::fs::File::open(&log_path)?;
                let reader = io::BufReader::new(file);

                // Seek to end minus some lines initially
                // For simplicity, we just print the last N lines then follow
                let all_lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
                let start = all_lines.len().saturating_sub(*lines);
                for line in &all_lines[start..] {
                    println!("{}", line);
                }

                // Follow
                let mut last_pos = std::fs::metadata(&log_path)?.len();
                loop {
                    let current_len = std::fs::metadata(&log_path)?.len();
                    if current_len > last_pos {
                        let mut f = std::fs::File::open(&log_path)?;
                        use std::io::Seek;
                        f.seek(io::SeekFrom::Start(last_pos))?;
                        let r = io::BufReader::new(f);
                        for line in r.lines() {
                            println!("{}", line?);
                        }
                        last_pos = current_len;
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            } else {
                let file = std::fs::File::open(&log_path)?;
                let reader = io::BufReader::new(file);
                let all_lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
                let start = all_lines.len().saturating_sub(*lines);
                for line in &all_lines[start..] {
                    println!("{}", line);
                }
            }
        }
        Commands::Kill => {
            let Some((host, port)) = daemon_endpoint_from_api_url(&cli.url) else {
                anyhow::bail!("Unsupported URL format for kill command: {}", cli.url);
            };

            if host != "127.0.0.1" && host != "localhost" {
                anyhow::bail!(
                    "Kill command can only be used on local daemon (host is {})",
                    host
                );
            }

            println!("Stopping FeTorrent daemon on port {}...", port);

            // Cross-platform port kill attempt using lsof on unix-like systems
            let output = Command::new("lsof")
                .arg("-t")
                .arg("-i")
                .arg(format!(":{}", port))
                .output();

            match output {
                Ok(out) if out.status.success() => {
                    let pids = String::from_utf8_lossy(&out.stdout);
                    for pid_str in pids.lines() {
                        if let Ok(pid) = pid_str.trim().parse::<i32>() {
                            let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
                        }
                    }
                    println!("{}", "Daemon stopped successfully.".green().bold());
                }
                _ => {
                    // If lsof fails or no PIDs found, it might already be dead
                    println!("{}", "No running daemon detected on this port.".yellow());
                }
            }
        }
    }

    Ok(())
}

async fn cmd_result(op: &str, res: reqwest::Response, json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", res.text().await?);
    } else if res.status().is_success() {
        println!("{}", format!("{} successful", op).green().bold());
    } else {
        eprintln!("{}", format!("{} failed: {}", op, res.text().await?).red());
    }
    Ok(())
}

async fn send_request(
    req: reqwest::RequestBuilder,
    operation: &str,
    url: &str,
    allow_auto_start: bool,
) -> anyhow::Result<reqwest::Response> {
    let retry_req = if allow_auto_start {
        req.try_clone()
    } else {
        None
    };
    let response = match req.send().await {
        Ok(response) => response,
        Err(err) => {
            if allow_auto_start && can_auto_start_daemon(url) && retry_req.is_some() {
                if let Err(start_err) = start_daemon_for_url(url) {
                    warn!(error = %start_err, "auto-start failed");
                } else if wait_for_daemon(url, Duration::from_secs(30)).await {
                    if let Some(retry_req) = retry_req {
                        return retry_req.send().await.with_context(|| {
                            format!(
                                "{} failed after auto-start while contacting daemon at {}. Start it with 'fetorrent run' or set --url.",
                                operation,
                                url
                            )
                        });
                    }
                }
            }

            return Err(err).with_context(|| {
                format!(
                    "{} failed while contacting daemon at {}. Start it with 'fetorrent run' or set --url.",
                    operation,
                    url
                )
            });
        }
    };
    Ok(response)
}

async fn ensure_daemon_available(api_url: &str) -> anyhow::Result<()> {
    let Some((host, port)) = daemon_endpoint_from_api_url(api_url) else {
        return Ok(());
    };

    if tokio::net::TcpStream::connect((host.as_str(), port))
        .await
        .is_ok()
    {
        return Ok(());
    }

    start_daemon_for_url(api_url).context("failed during daemon preflight startup")?;
    if wait_for_daemon(api_url, Duration::from_secs(30)).await {
        return Ok(());
    }

    anyhow::bail!(
        "daemon did not become ready at {}. Start it with 'fetorrent run' or set --url.",
        api_url
    )
}

fn can_auto_start_daemon(url: &str) -> bool {
    daemon_endpoint_from_api_url(url).is_some()
}

fn daemon_endpoint_from_api_url(api_url: &str) -> Option<(String, u16)> {
    let parsed = reqwest::Url::parse(api_url).ok()?;
    let host = parsed.host_str()?.to_string();
    if host != "127.0.0.1" && host != "localhost" {
        return None;
    }
    Some((host, parsed.port_or_known_default()?))
}

fn start_daemon_for_url(api_url: &str) -> anyhow::Result<()> {
    let (_, port) =
        daemon_endpoint_from_api_url(api_url).context("unsupported URL for auto-start")?;

    let port_arg = port.to_string();
    let try_spawn = |program: &str, args: &[&str]| -> std::io::Result<()> {
        let mut cmd = Command::new(program);
        cmd.args(args).stdout(Stdio::null()).stderr(Stdio::null());
        cmd.spawn().map(|_| ())
    };

    if try_spawn("fetorrent-daemon", &["--port", &port_arg]).is_ok() {
        return Ok(());
    }

    if try_spawn("./target/debug/fetorrent-daemon", &["--port", &port_arg]).is_ok() {
        return Ok(());
    }
    if try_spawn("./target/release/fetorrent-daemon", &["--port", &port_arg]).is_ok() {
        return Ok(());
    }

    let mut cargo_cmd = Command::new("cargo");
    cargo_cmd
        .arg("run")
        .arg("-p")
        .arg("fetorrent-daemon")
        .arg("--")
        .arg("--port")
        .arg(port_arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cargo_cmd
        .spawn()
        .map(|_| ())
        .context("failed to spawn daemon via cargo")
}

async fn wait_for_daemon(api_url: &str, timeout: Duration) -> bool {
    let Some((host, port)) = daemon_endpoint_from_api_url(api_url) else {
        return false;
    };
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if tokio::net::TcpStream::connect((host.as_str(), port))
            .await
            .is_ok()
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    false
}

fn should_print_banner(command: &Commands, json: bool) -> bool {
    !json && matches!(command, Commands::Run { .. })
}

async fn resolve_add_directory(
    client: &Client,
    api_url: &str,
    provided_dir: Option<String>,
) -> anyhow::Result<Option<String>> {
    if let Some(dir) = provided_dir {
        return Ok(Some(dir));
    }

    if !std::io::stdin().is_terminal() {
        return Ok(None);
    }

    println!("Choose download location:");
    println!("  1) Use default from settings");
    println!("  2) Pick folder from file manager");
    println!("  3) Enter custom path");
    print!("Select option [1/2/3] (default 1): ");
    std::io::stdout().flush()?;

    let mut option = String::new();
    std::io::stdin().read_line(&mut option)?;

    match option.trim() {
        "2" => {
            let res = send_request(
                client.get(format!("{}/select-directory", api_url)),
                "Select directory",
                api_url,
                true,
            )
            .await?;

            if !res.status().is_success() {
                let msg = res
                    .text()
                    .await
                    .unwrap_or_else(|_| "Directory selection failed".to_string());
                println!("{}", format!("Falling back to default directory: {}", msg).yellow());
                return Ok(None);
            }

            let payload: SelectDirectoryResponse = res.json().await?;
            Ok(Some(payload.path))
        }
        "3" => {
            print!("Enter download path: ");
            std::io::stdout().flush()?;
            let mut path = String::new();
            std::io::stdin().read_line(&mut path)?;
            let trimmed = path.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        _ => Ok(None),
    }
}

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

fn print_torrents(torrents: &[TorrentInfo]) {
    if torrents.is_empty() {
        println!("{}", "No torrents active.".truecolor(100, 100, 100));
        return;
    }

    let mut table = Table::new();
    table
        .load_preset(NOTHING)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("ID")
                .add_attribute(Attribute::Bold)
                .fg(Color::DarkGrey),
            Cell::new("Name").add_attribute(Attribute::Bold),
            Cell::new("Progress").add_attribute(Attribute::Bold),
            Cell::new("Status").add_attribute(Attribute::Bold),
            Cell::new("Seeds")
                .add_attribute(Attribute::Bold)
                .fg(Color::Green),
            Cell::new("Leechers")
                .add_attribute(Attribute::Bold)
                .fg(Color::Yellow),
            Cell::new("Total Peers")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("Size").add_attribute(Attribute::Bold),
            Cell::new("Speed (DL/UL)").add_attribute(Attribute::Bold),
            Cell::new("ETA")
                .add_attribute(Attribute::Bold)
                .fg(Color::Yellow),
        ]);

    for t in torrents {
        let (prog_color, stat_color) = match t.status {
            TorrentStatus::Downloading | TorrentStatus::DownloadingMetadata => {
                (Color::Cyan, Color::Cyan)
            }
            TorrentStatus::Seeding | TorrentStatus::Finished => (Color::Green, Color::Green),
            TorrentStatus::Queued | TorrentStatus::Checking => (Color::Yellow, Color::Yellow),
            TorrentStatus::Paused => (Color::DarkGrey, Color::DarkGrey),
            TorrentStatus::Error(_) => (Color::Red, Color::Red),
        };

        table.add_row(vec![
            Cell::new(t.id).fg(Color::DarkGrey),
            Cell::new(truncate(&t.name, 25)).add_attribute(Attribute::Bold),
            Cell::new(format!("{:.1}%", t.progress * 100.0)).fg(prog_color),
            Cell::new(t.status.to_string()).fg(stat_color),
            Cell::new(t.num_seeds).fg(Color::Green),
            Cell::new(t.num_leechers).fg(Color::Yellow),
            Cell::new(t.num_peers).fg(Color::Cyan),
            Cell::new(format_bytes(t.total_size)),
            Cell::new(format!(
                "{} ↓ / {} ↑",
                format_bytes(t.dl_speed),
                format_bytes(t.ul_speed)
            )),
            Cell::new(format_eta(t.eta_secs)).fg(Color::Yellow),
        ]);
    }

    println!("{table}");
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let mut truncated: String = s.chars().take(max - 3).collect();
        truncated.push_str("...");
        truncated
    } else {
        s.to_string()
    }
}

fn format_bytes(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut index = 0;
    while size >= 1024.0 && index < units.len() - 1 {
        size /= 1024.0;
        index += 1;
    }
    if index == 0 {
        format!("{size} {u}", u = units[index])
    } else {
        format!("{size:.1} {u}", u = units[index])
    }
}

fn format_eta(secs: Option<u64>) -> String {
    match secs {
        None => "∞".to_string(),
        Some(0) => "Done".to_string(),
        Some(s) if s > 86400 => format!("{}d", s / 86400),
        Some(s) if s > 3600 => format!("{}h {}m", s / 3600, (s % 3600) / 60),
        Some(s) if s > 60 => format!("{}m {}s", s / 60, s % 60),
        Some(s) => format!("{}s", s),
    }
}

fn print_banner() {
    let banner = include_str!("banner.txt");
    println!("{}", banner.bright_cyan().bold());
    println!(
        "{}",
        "   --- FeTorrent: Modern Rust BitTorrent Engine ---"
            .truecolor(100, 100, 100)
            .italic()
    );
    println!();
}
