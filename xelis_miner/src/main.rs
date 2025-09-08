pub mod config;

use crate::config::DEFAULT_DAEMON_ADDRESS;

// --- Standard Library ---
use std::{
    fs::File,
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        RwLock,
    },
    thread,
    time::Duration,
};

// --- Async / Tokio ---
use tokio::{
    select,
    sync::{broadcast, mpsc, Mutex},
    task::JoinHandle,
    time::Instant,
};
#[cfg(feature = "api_stats")]
use tokio::{io::AsyncWriteExt, net::TcpListener};

// --- Futures ---
use futures_util::{SinkExt, StreamExt};

// --- WebSocket (tokio-tungstenite) ---
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error as TungsteniteError, Message},
};

// --- Serialization ---
use serde::{Deserialize, Serialize};

use xelis_common::{
    api::daemon::{
        GetMinerWorkResult,
        SubmitMinerWorkParams,
    },
    async_handler,
    block::{
        Algorithm,
        MinerWork,
        Worker
    },
    config::VERSION,
    crypto::{
        Address,
        Hash,
    },
    difficulty::{
        check_difficulty_against_target,
        compute_difficulty_target,
        difficulty_from_hash,
        Difficulty
    },
    prompt::{
        command::CommandManager,
        Color,
        LogLevel,
        ModuleConfig,
        Prompt,
        ShareablePrompt
    },
    serializer::Serializer,
    time::get_current_time_in_millis,
    tokio::spawn_task,
    utils::{
        format_difficulty,
        format_hashrate,
        sanitize_daemon_address
    }
};
use clap::Parser;
use log::{trace, debug, info, warn, error};
use anyhow::{Result, Error, Context};
use lazy_static::lazy_static;

/// Returns the default daemon address
fn default_daemon_address() -> String {
    DEFAULT_DAEMON_ADDRESS.to_owned()
}

/// Returns the default number of iterations for benchmarking or mining tasks
fn default_iterations() -> usize {
    100
}

/// Returns the default log filename
fn default_log_filename() -> String {
    "xelis-miner.log".to_owned()
}

/// Returns the default logs directory path
fn default_logs_path() -> String {
    "logs/".to_owned()
}

/// Returns the default worker name
fn default_worker_name() -> String {
    "default".to_owned()
}


#[derive(Parser, Serialize, Deserialize)]
pub struct LogConfig {
    /// Set log level
    #[clap(long, value_enum, default_value_t = LogLevel::Info)]
    #[serde(default)]
    log_level: LogLevel,
    /// Set file log level
    /// By default, it will be the same as log level
    #[clap(long, value_enum)]
    file_log_level: Option<LogLevel>,
    /// Disable the log file
    #[clap(long)]
    #[serde(default)]
    disable_file_logging: bool,
    /// Disable the log filename date based
    /// If disabled, the log file will be named xelis-miner.log instead of YYYY-MM-DD.xelis-miner.log
    #[clap(long)]
    #[serde(default)]
    disable_file_log_date_based: bool,
    /// Enable the log file auto compression
    /// If enabled, the log file will be compressed every day
    /// This will only work if the log file is enabled
    #[clap(long)]
    #[serde(default)]
    auto_compress_logs: bool,
    /// Disable the usage of colors in log
    #[clap(long)]
    #[serde(default)]
    disable_log_color: bool,
    /// Disable terminal interactive mode
    /// You will not be able to write CLI commands in it or to have an updated prompt
    #[clap(long)]
    #[serde(default)]
    disable_interactive_mode: bool,
    /// Log filename
    /// 
    /// By default filename is xelis-miner.log.
    /// File will be stored in logs directory, this is only the filename, not the full path.
    /// Log file is rotated every day and has the format YYYY-MM-DD.xelis-miner.log.
    #[clap(default_value_t = String::from("xelis-miner.log"))]
    #[serde(default = "default_log_filename")]
    filename_log: String,
    /// Logs directory
    /// 
    /// By default it will be logs/ of the current directory.
    /// It must end with a / to be a valid folder.
    #[clap(long, default_value_t = String::from("logs/"))]
    #[serde(default = "default_logs_path")]
    logs_path: String,
    /// Module configuration for logs
    #[clap(long)]
    #[serde(default)]
    logs_modules: Vec<ModuleConfig>,
}

#[derive(Parser, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    /// Enable the benchmark mode with the specified algorithm
    #[clap(long)]
    benchmark: Option<Algorithm>,

    /// Iterations to run the benchmark
    #[clap(long, default_value_t = 100)]
    #[serde(default = "default_iterations")]
    iterations: usize,
}


#[derive(Parser, Serialize, Deserialize)]
#[clap(version = VERSION, about = "XELIS is an innovative cryptocurrency built from scratch with BlockDAG, Homomorphic Encryption, Zero-Knowledge Proofs, and Smart Contracts.")]
#[command(styles = xelis_common::get_cli_styles())]
pub struct Config {
    /// Log configuration
    #[clap(flatten)]
    log: LogConfig,
    /// Benchmark configuration
    #[clap(flatten)]
    benchmark: BenchmarkConfig,
    /// Wallet address to mine and receive block rewards on
    #[clap(short, long)]
    miner_address: Option<Address>,
    /// Daemon address to connect to for mining
    #[clap(long, default_value_t = String::from(DEFAULT_DAEMON_ADDRESS))]
    #[serde(default = "default_daemon_address")]
    daemon_address: String,
    /// Bind address for stats API
    #[cfg(feature = "api_stats")]
    #[clap(long)]
    api_bind_address: Option<String>,
    /// Numbers of threads to use (at least 1, max: 65535)
    /// By default, this will try to detect the number of threads available on your CPU.
    #[clap(short, long)]
    num_threads: Option<u16>,
    /// Worker name to be displayed on daemon side
    #[clap(short, long, default_value_t = String::from("default"))]
    #[serde(default = "default_worker_name")]
    worker: String,
    /// JSON File to load the configuration from
    #[clap(long)]
    #[serde(skip)]
    #[serde(default)]
    config_file: Option<String>,
    /// Generate the template at the `config_file` path
    #[clap(long)]
    #[serde(skip)]
    #[serde(default)]
    generate_config_template: bool
}

use serde::{Serialize, Deserialize};

/// Notification sent to worker threads
#[derive(Clone, Debug)]
pub enum ThreadNotification<'a> {
    /// New mining job available for the worker thread
    NewJob {
        algorithm: Algorithm,
        work: MinerWork<'a>,
        difficulty: Difficulty,
        height: u64,
    },
    /// WebSocket connection closed
    WebSocketClosed,
    /// All threads should stop
    Exit,
}

/// Messages exchanged over WebSocket
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocketMessage {
    /// A new job was received from the pool
    NewJob(GetMinerWorkResult),
    /// A mined block was accepted
    BlockAccepted,
    /// A mined block was rejected (reason provided)
    BlockRejected(String),
}


// Optional: conversion if relevant
impl<'a> From<ThreadNotification<'a>> for Option<SocketMessage> {
    fn from(notification: ThreadNotification<'a>) -> Self {
        match notification {
            ThreadNotification::NewJob { work, .. } => {
                Some(SocketMessage::NewJob(GetMinerWorkResult::from(work)))
            }
            ThreadNotification::WebSocketClosed => None,
            ThreadNotification::Exit => None,
        }
    }
}


static WEBSOCKET_CONNECTED: AtomicBool = AtomicBool::new(false);
static CURRENT_TOPO_HEIGHT: AtomicU64 = AtomicU64::new(0);
static BLOCKS_FOUND: AtomicUsize = AtomicUsize::new(0);
static BLOCKS_REJECTED: AtomicUsize = AtomicUsize::new(0);
static HASHRATE_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[cfg(feature = "api_stats")]
static HASHRATE: AtomicU64 = AtomicU64::new(0);

static JOB_ELAPSED: RwLock<Option<Instant>> = RwLock::new(None);

lazy_static! {
    static ref HASHRATE_LAST_TIME: Mutex<Instant> = Mutex::new(Instant::now());
}

/// After how many iterations we update the timestamp of the block
/// to avoid too much CPU usage
const UPDATE_EVERY_NONCE: u64 = 10;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let mut config: Config = Config::parse();

    // Handle config file logic
    if let Some(path) = config.config_file.as_ref() {
        if config.generate_config_template {
            if Path::new(path).exists() {
                eprintln!("Config file already exists at {}", path);
                return Ok(());
            }

            let mut file = File::create(path).context("Error while creating config file")?;
            let json =
                serde_json::to_string_pretty(&config).context("Error while serializing config file")?;
            file.write_all(json.as_bytes())
                .context("Error while writing config file")?;
            println!("Config file template generated at {}", path);
            return Ok(());
        }

        let file = File::open(path).context("Error while opening config file")?;
        config = serde_json::from_reader(file).context("Error while reading config file")?;
    } else if config.generate_config_template {
        eprintln!(
            "Provided config file path is required to generate \
             the template with --config-file"
        );
        return Ok(());
    }

    // Init logger / prompt
    let log = config.log;
    let prompt = Prompt::new(
        log.log_level,
        &log.logs_path,
        &log.filename_log,
        log.disable_file_logging,
        log.disable_file_log_date_based,
        log.disable_log_color,
        log.auto_compress_logs,
        !log.disable_interactive_mode,
        log.logs_modules,
        log.file_log_level.unwrap_or(log.log_level),
    )?;

    // ⬇️ rest of your async miner runtime would continue here…
    Ok(())
}


    // Prevent the user to block the program by selecting text in CLI
    #[cfg(target_os = "windows")]
    prompt.adjust_win_console()?;

// Detect number of available CPU threads
let detected_threads = thread::available_parallelism()
    .map(|n| n.get() as u16)
    .unwrap_or_else(|e| {
        warn!(
            "Unable to detect available CPU threads: {}. Defaulting to 1 thread.",
            e
        );
        1
    });

// Determine threads to use: user-defined or auto-detected
let threads = config.num_threads.unwrap_or(detected_threads);

info!(
    "Miner will run with {} threads (detected available threads: {})",
    threads, detected_threads
);

// Run benchmark if requested
if let Some(algorithm) = config.benchmark.benchmark {
    info!(
        "Benchmark mode enabled. Testing up to {} threads with algorithm: {:?}",
        threads, algorithm
    );

    benchmark(
        threads as usize,
        config.benchmark.iterations,
        algorithm,
    );

    info!("Benchmark completed successfully.");
    return Ok(());
}

let address = config
    .miner_address
    .ok_or_else(|| Error::msg("No miner address specified"))?;
info!("Miner address: {}", address);

if threads != detected_threads {
    warn!(
        "Attention: Number of mining threads used is {}, but the recommended is: {}",
        threads, detected_threads
    );
}

// Broadcast channel for sending jobs or exit notifications to all threads
let (sender, _) = broadcast::channel::<ThreadNotification>(threads as usize);

// MPSC channel for collecting mined work from all threads
let (block_sender, block_receiver) = mpsc::channel::<MinerWork>(threads as usize);

// Spawn mining worker threads
for id in 0..threads {
    debug!("Starting mining thread #{}", id);
    if let Err(e) = start_thread(id, sender.subscribe(), block_sender.clone()) {
        error!("Failed to spawn mining thread #{}: {}", id, e);
    }
}

// Start communication task
let task = spawn_task(
    "communication",
    communication_task(
        config.daemon_address,
        sender.clone(),
        block_receiver,
        address,
        config.worker,
    ),
);

// Optional stats broadcaster task (behind `api_stats` feature)
let stats_task: Option<JoinHandle<Result<()>>> = {
    #[cfg(feature = "api_stats")]
    {
        match config.api_bind_address {
            Some(addr) => Some(spawn_task("broadcast", broadcast_stats_task(addr))),
            None => None,
        }
    }
    #[cfg(not(feature = "api_stats"))]
    {
        None
    }
};

// Run interactive prompt loop
if let Err(e) = run_prompt(prompt).await {
    error!("Error while running prompt: {}", e);
}

// Graceful shutdown
if sender.send(ThreadNotification::Exit).is_err() {
    debug!("Error while sending exit message to threads");
}

// Stop the communication task
task.abort();

// Stop the stats broadcast task (if running)
if let Some(stats_handle) = stats_task {
    stats_handle.abort();
}

#[cfg(feature = "api_stats")]
async fn broadcast_stats_task(broadcast_address: String) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting broadcast task");
    let listener = TcpListener::bind(broadcast_address).await?;

    loop {
        let (mut socket, _) = listener.accept().await?;
        
        tokio::spawn(async move {
            // Assume these are accessible or cloned accordingly
            let blocks_found = BLOCKS_FOUND.load(Ordering::SeqCst);
            let blocks_rejected = BLOCKS_REJECTED.load(Ordering::SeqCst);
            let hashrate = HASHRATE.load(Ordering::SeqCst);

            let data = serde_json::json!({
                "accepted": blocks_found,
                "rejected": blocks_rejected,
                "hashrate": hashrate,
                "hashrate_formatted": format_hashrate(hashrate as f64),
            });

            let status_line = "HTTP/1.1 200 OK\r\n";
            let content_type = "Content-Type: application/json\r\n";
            let contents = data.to_string();
            let length = contents.len();
            let response = format!("{status_line}{content_type}Content-Length: {length}\r\n\r\n{contents}");

            if let Err(e) = AsyncWriteExt::write_all(&mut socket, response.as_bytes()).await {
                log::error!("Failed to write response: {}", e);
            }

            if let Err(e) = socket.shutdown().await {
                log::error!("Failed to shutdown socket: {}", e);
            }
        });
    }
}



// Benchmark the miner with the specified number of threads and iterations
// It will output the total time, total iterations, time per PoW and hashrate for each number of threads
use std::thread;
use std::time::Instant;
use log::info;

fn benchmark(threads: usize, iterations: usize, algorithm: Algorithm) {
    info!(
        "{:<10} | {:<12} | {:<18} | {:<15} | {:<13}",
        "Threads", "Total Time", "Total Iterations", "Time/PoW (ms)", "Hashrate"
    );

    for bench in 1..=threads {
        let total_iters = bench * iterations;
        let start = Instant::now();

        let mut handles = Vec::with_capacity(bench);
        for _ in 0..bench {
            let job = MinerWork::new(Hash::zero(), get_current_time_in_millis());
            let mut worker = Worker::new();
            worker.set_work(job, algorithm).unwrap();

            let handle = thread::spawn(move || {
                for _ in 0..iterations {
                    let _ = worker.get_pow_hash().unwrap();
                    worker.increase_nonce().unwrap();
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        let duration = start.elapsed();
        let total_time_ms = duration.as_secs_f64() * 1000.0;
        let time_per_pow = total_time_ms / total_iters as f64;
        let hashrate = format_hashrate(1000.0 / time_per_pow);

        info!(
            "{:<10} | {:<12.3} | {:<18} | {:<15.3} | {:<13}",
            bench, total_time_ms, total_iters, time_per_pow, hashrate
        );
    }
}


// this Tokio task will runs indefinitely until the user stop himself the miner.
// It maintains a WebSocket connection with the daemon and notify all threads when it receive a new job.
// Its also the task who have the job to send directly the new block found by one of the threads.
// This allow mining threads to only focus on mining and receiving jobs through memory channels.
async fn communication_task(daemon_address: String, job_sender: broadcast::Sender<ThreadNotification<'_>>, mut block_receiver: mpsc::Receiver<MinerWork<'_>>, address: Address, worker: String) {
    info!("Starting communication task");
    let daemon_address = sanitize_daemon_address(&daemon_address);
    'main: loop {
        info!("Trying to connect to {}", daemon_address);
        let client = match connect_async(format!("{}/getwork/{}/{}", daemon_address, address.to_string(), worker)).await {
            Ok((client, response)) => {
                let status = response.status();
                if status.is_server_error() || status.is_client_error() {
                    error!("Error while connecting to {}, got an unexpected response: {}", daemon_address, status.as_str());
                    warn!("Trying to connect to WebSocket again in 10 seconds...");
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue 'main;
                }
                client
            },
            Err(e) => {
                if let TungsteniteError::Http(e) = e {
                    let body: String = e.into_body()
                        .map_or(
                            "Unknown error".to_owned(),
                            |v| String::from_utf8_lossy(&v).to_string()
                        );
                    error!("Error while connecting to {}, got an unexpected response: {}", daemon_address, body);
                } else {
                    error!("Error while connecting to {}: {}", daemon_address, e);
                }

                warn!("Trying to connect to WebSocket again in 10 seconds...");
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue 'main;
            }
        };
        WEBSOCKET_CONNECTED.store(true, Ordering::SeqCst);
        info!("Connected successfully to {}", daemon_address);
        let (mut write, mut read) = client.split();
        loop {
            select! {
                Some(message) = read.next() => { // read all messages from daemon
                    debug!("Received message from daemon: {:?}", message);
                    match handle_websocket_message(message, &job_sender).await {
                        Ok(exit) => {
                            if exit {
                                debug!("Exiting communication task");
                                break;
                            }
                        },
                        Err(e) => {
                            error!("Error while handling message from WebSocket: {}", e);
                            break;
                        }
                    }
                },
                Some(work) = block_receiver.recv() => { // send all valid blocks found to the daemon
                    info!("submitting new block found...");
                    let submit = serde_json::json!(SubmitMinerWorkParams { miner_work: work.to_hex() }).to_string();
                    if let Err(e) = write.send(Message::Text(submit)).await {
                        error!("Error while sending the block found to the daemon: {}", e);
                        break;
                    }
                    debug!("Block found has been sent to daemon");
                }
            }
        }

        WEBSOCKET_CONNECTED.store(false, Ordering::SeqCst);
        if job_sender.send(ThreadNotification::WebSocketClosed).is_err() {
            error!("Error while sending WebSocketClosed message to threads");
        }

        warn!("Trying to connect to WebSocket again in 10 seconds...");
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

async fn handle_websocket_message(
    message: Result<Message, TungsteniteError>,
    job_sender: &broadcast::Sender<ThreadNotification<'_>>,
) -> Result<bool, Error> {
    match message? {
        Message::Text(text) => {
            debug!("Received new WebSocket message: {}", text);

            match serde_json::from_slice::<SocketMessage>(text.as_bytes()) {
                Ok(SocketMessage::NewJob(job)) => {
                    info!(
                        "New job: difficulty={} height={}",
                        format_difficulty(job.difficulty),
                        job.height
                    );

                    let block = MinerWork::from_hex(&job.miner_work)
                        .context("Failed to decode miner work from daemon")?;

                    CURRENT_TOPO_HEIGHT.store(job.topoheight, Ordering::SeqCst);
                    JOB_ELAPSED
                        .write()
                        .unwrap()
                        .replace(Instant::now());

                    if let Err(e) = job_sender.send(ThreadNotification::NewJob(
                        job.algorithm,
                        block,
                        job.difficulty,
                        job.height,
                    )) {
                        error!("Failed to send new job to mining threads: {}", e);
                    }
                }
                Ok(SocketMessage::BlockAccepted) => {
                    BLOCKS_FOUND.fetch_add(1, Ordering::SeqCst);
                    info!("Block accepted by network.");
                }
                Ok(SocketMessage::BlockRejected(err)) => {
                    BLOCKS_REJECTED.fetch_add(1, Ordering::SeqCst);
                    error!("Block rejected by network: {}", err);
                }
                Err(e) => {
                    error!("Failed to deserialize WebSocket message: {}", e);
                }
            }
        }
        Message::Close(reason) => {
            let reason = reason
                .map(|r| r.to_string())
                .unwrap_or_else(|| "No reason provided".to_string());
            warn!("Daemon closed WebSocket connection: {}", reason);
            return Ok(true); // signal connection should stop
        }
        Message::Ping(_) => {
            trace!("Received ping from daemon.");
        }
        Message::Pong(_) => {
            trace!("Received pong from daemon."); // optional: keep-alive logging
        }
        other => {
            warn!("Unexpected WebSocket message: {:?}", other);
            return Ok(true); // stop loop on unknown messages
        }
    }

    Ok(false)
}


fn start_thread(id: u16, mut job_receiver: broadcast::Receiver<ThreadNotification<'static>>, block_sender: mpsc::Sender<MinerWork<'static>>) -> Result<(), Error> {
    let builder = thread::Builder::new().name(format!("Mining Thread #{}", id));
    builder.spawn(move || {
        let mut worker = Worker::new();
        let mut hash: Hash;

        info!("Mining Thread #{}: started", id);
        'main: loop {
            let message = match job_receiver.blocking_recv() {
                Ok(message) => message,
                Err(e) => {
                    error!("Error on thread #{} while waiting on new job: {}", id, e);
                    // Channel is maybe lagging, try to empty it
                    while job_receiver.len() > 1 {
                        let _ = job_receiver.blocking_recv();
                    }
                    thread::sleep(Duration::from_millis(100));
                    continue;
                }
            };

            match message {
                ThreadNotification::WebSocketClosed => {
                    // wait until we receive a new job, check every 100ms
                    while job_receiver.is_empty() {
                        thread::sleep(Duration::from_millis(100));
                    }
                }
                ThreadNotification::Exit => {
                    info!("Exiting Mining Thread #{}...", id);
                    break 'main;
                },
                ThreadNotification::NewJob(algorithm, mut new_job, expected_difficulty, height) => {
                    debug!("Mining Thread #{} received a new job", id);
                    // set thread id in extra nonce for more work spread between threads
                    // u16 support up to 65535 threads
                    new_job.set_thread_id_u16(id);
                    let initial_timestamp = new_job.get_timestamp();
                    worker.set_work(new_job, algorithm).unwrap();

                    let difficulty_target = match compute_difficulty_target(&expected_difficulty) {
                        Ok(value) => value,
                        Err(e) => {
                            error!("Mining Thread #{}: error on difficulty target computation: {}", id, e);
                            continue 'main;
                        }
                    };

                    // Solve block
                    hash = worker.get_pow_hash().unwrap();
                    let mut tries = 0;
                    while !check_difficulty_against_target(&hash, &difficulty_target) {
                        worker.increase_nonce().unwrap();
                        // check if we have a new job pending
                        // Only update every N iterations to avoid too much CPU usage
                        if tries % UPDATE_EVERY_NONCE == 0 {
                            if !job_receiver.is_empty() {
                                continue 'main;
                            }
                            if let Ok(instant) = JOB_ELAPSED.read() {
                                if let Some(instant) = instant.as_ref() {
                                    worker.set_timestamp(initial_timestamp + instant.elapsed().as_millis() as u64).unwrap();
                                }
                            }
                            HASHRATE_COUNTER.fetch_add(UPDATE_EVERY_NONCE as usize, Ordering::SeqCst);
                        }

                        hash = worker.get_pow_hash().unwrap();
                        tries += 1;
                    }

                    // compute the reference hash for easier finding of the block
                    let block_hash = worker.get_block_hash().unwrap();
                    info!("Thread #{}: block {} found at height {} with difficulty {}", id, block_hash, height, format_difficulty(difficulty_from_hash(&hash)));

                    let job = worker.take_work().unwrap();
                    if let Err(_) = block_sender.blocking_send(job) {
                        error!("Mining Thread #{}: error while sending block found with hash {}", id, block_hash);
                        continue 'main;
                    }
                    debug!("Job sent to communication task");
                }
            };
        }
        info!("Mining Thread #{}: stopped", id);
    })?;
    Ok(())
}

async fn run_prompt(prompt: ShareablePrompt) -> Result<()> {
    let command_manager = CommandManager::new(prompt.clone());
    command_manager.register_default_commands()?;

    let closure = |_: &_, _: _| async {
        let topoheight_str = format!(
            "{}: {}",
            prompt.colorize_string(Color::Yellow, "TopoHeight"),
            prompt.colorize_string(Color::Green, &format!("{}", CURRENT_TOPO_HEIGHT.load(Ordering::SeqCst))),
        );
        let blocks_found = format!(
            "{}: {}",
            prompt.colorize_string(Color::Yellow, "Accepted"),
            prompt.colorize_string(Color::Green, &format!("{}", BLOCKS_FOUND.load(Ordering::SeqCst))),
        );
        let blocks_rejected = format!(
            "{}: {}",
            prompt.colorize_string(Color::Yellow, "Rejected"),
            prompt.colorize_string(Color::Green, &format!("{}", BLOCKS_REJECTED.load(Ordering::SeqCst))),
        );
        let status = if WEBSOCKET_CONNECTED.load(Ordering::SeqCst) {
            prompt.colorize_string(Color::Green, "Online")
        } else {
            prompt.colorize_string(Color::Red, "Offline")
        };
        let hashrate = {
            let mut last_time = HASHRATE_LAST_TIME.lock().await;
            let counter = HASHRATE_COUNTER.swap(0, Ordering::SeqCst);

            let hashrate = 1000f64 / (last_time.elapsed().as_millis() as f64 / counter as f64);
            *last_time = Instant::now();

            #[cfg(feature = "api_stats")]
            HASHRATE.store(hashrate as u64, Ordering::SeqCst);

            prompt.colorize_string(Color::Green, &format!("{}", format_hashrate(hashrate)))
        };

        Ok(
            format!(
                "{} | {} | {} | {} | {} | {} {} ",
                prompt.colorize_string(Color::Blue, "XELIS Miner"),
                topoheight_str,
                blocks_found,
                blocks_rejected,
                hashrate,
                status,
                prompt.colorize_string(Color::BrightBlack, ">>")
            )
        )
    };

    prompt.start(Duration::from_millis(1000), Box::new(async_handler!(closure)), Some(&command_manager)).await?;
    Ok(())
}
