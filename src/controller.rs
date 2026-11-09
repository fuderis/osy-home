use crate::{prelude::*, skills::home::RotationMode};

use atoman::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{mpsc, oneshot},
    time,
};
use tokio_serial::{SerialPortBuilderExt, SerialPortType, SerialStream};

const SKETCH_INO: &str = include_str!("../sketch/sketch.ino");
const SKETCH_CONFIG: &str = include_str!("../sketch/config.h");

/// Unique identificator (for device recognition).
pub const UUID: &str = "osy-home-controller";
/// Firmware version (sketch).
pub const VERSION: &str = "1.4.6";

/// Arduino controller global state.
static CONTROLLER: State<Option<Controller>> = State::new(|| None);

/// Arduino controller metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerMetadata {
    pub device: String,
    pub version: String,
}

/// Commands for the background controller worker.
enum ControllerCommand {
    SendJson {
        doc: JsonValue,
        reply: Option<oneshot::Sender<Result<String>>>,
    },
    Disconnect,
}

/// Arduino controller.
#[derive(Clone)]
pub struct Controller {
    pub port_name: String,
    cmd_tx: mpsc::Sender<ControllerCommand>,
}

impl Controller {
    /// Locks & returns controller state guard.
    pub async fn lock() -> StateGuard<Option<Self>> {
        CONTROLLER.lock().await
    }

    /// Disconnects from the device, stops background worker, and clears state.
    pub async fn free() {
        if let Some(ctrl) = CONTROLLER.lock().await.take() {
            let _ = ctrl.cmd_tx.send(ControllerCommand::Disconnect).await;
        }
    }

    /// Initializes the arduino controller connection asynchronously.
    pub async fn init(custom_port: Option<String>) -> Result<bool> {
        let port_name = match custom_port {
            Some(ref p) => {
                info!("Using manually specified port: {p}");
                p.clone()
            }
            None => {
                info!("Auto-detecting Arduino Pro Micro board...");
                match Self::auto_detect_port().await {
                    Ok(p) => p,
                    Err(_) => return Ok(false),
                }
            }
        };

        let mut stream = match tokio_serial::new(&port_name, 115200)
            .timeout(Duration::from_millis(1000))
            .open_native_async()
        {
            Ok(s) => s,
            Err(e) => {
                if custom_port.is_some() {
                    return Err(Error::Custom(format!(
                        "Failed to open serial port {port_name}: {e}"
                    ))
                    .into());
                }
                return Ok(false);
            }
        };

        if let Err(e) = Self::verify_handshake(&mut stream).await {
            if custom_port.is_some() {
                return Err(e);
            }
            return Ok(false);
        }

        let (cmd_tx, cmd_rx) = mpsc::channel(32);
        atoman::spawn(async move {
            Self::worker_loop(stream, cmd_rx).await;
        });

        let ctrl = Self {
            port_name: port_name.clone(),
            cmd_tx,
        };
        CONTROLLER.set(Some(ctrl)).await;

        info!("Successfully connected and verified Controller on port `{port_name}`.");
        Ok(true)
    }

    /// Background worker loop managing asynchronous read/write without blocking.
    async fn worker_loop(stream: SerialStream, mut cmd_rx: mpsc::Receiver<ControllerCommand>) {
        let (reader_half, mut writer_half) = atoman::io::split(stream);
        let mut reader = BufReader::new(reader_half);
        let mut line = String::new();
        let mut pending_reply: Option<oneshot::Sender<Result<String>>> = None;

        loop {
            atoman::select! {
                res = reader.read_line(&mut line) => {
                    match res {
                        Ok(0) => {
                            warn!("Controller serial stream closed (EOF)");
                            break;
                        }
                        Ok(_) => {
                            let trimmed = line.trim().to_string();
                            if !trimmed.is_empty() {
                                if let Some(reply) = pending_reply.take() {
                                    let _ = reply.send(Ok(trimmed));
                                } else {
                                    Self::handle_incoming_event(&trimmed);
                                }
                            }
                            line.clear();
                        }
                        Err(e) => {
                            warn!("Error reading from controller serial stream: {e}");
                            break;
                        }
                    }
                }
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(ControllerCommand::SendJson { doc, reply }) => {
                            if let Some(r) = reply {
                                pending_reply = Some(r);
                            }
                            let data = format!("{doc}\n");
                            if let Err(e) = writer_half.write_all(data.as_bytes()).await {
                                warn!("Failed to write to controller serial port: {e}");
                                if let Some(r) = pending_reply.take() {
                                    let _ = r.send(Err(Error::Custom(format!("Write error: {e}")).into()));
                                }
                            }
                        }
                        Some(ControllerCommand::Disconnect) | None => {
                            info!("Controller background worker stopping due to disconnect signal.");
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Parse and log all incoming events from the board using prelude macros.
    fn handle_incoming_event(line: &str) {
        match serde_json::from_str::<JsonValue>(line) {
            Ok(json) => {
                let event_type = json
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                match event_type {
                    "code" => {
                        let code = json.get("code").and_then(|v| v.as_str()).unwrap_or("");
                        let millis = json.get("millis").and_then(|v| v.as_u64()).unwrap_or(0);
                        info!(code = %code, millis = %millis, "IR code received from board");
                    }
                    "alias" => {
                        let name = json.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        info!(name = %name, "Alias triggered on board");
                    }
                    "relay" => {
                        let index = json.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                        let enabled = json
                            .get("enabled")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        info!(index = %index, enabled = %enabled, "Relay state updated on board");
                    }
                    "servo" => {
                        let index = json.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                        let degs = json.get("degs").and_then(|v| v.as_i64()).unwrap_or(0);
                        info!(index = %index, degs = %degs, "Servo position updated on board");
                    }
                    "motor" => {
                        let index = json.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                        let direct = json.get("direct").and_then(|v| v.as_str()).unwrap_or("");
                        let enabled = json
                            .get("enabled")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let power = json.get("power").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        info!(index = %index, direct = %direct, enabled = %enabled, power = %power, "Motor state updated on board");
                    }
                    "pong" => {
                        info!("Received pong from board");
                    }
                    _ => {
                        info!(event = %line, "Received general event from board");
                    }
                }
            }
            Err(_) => {
                info!(raw = %line, "Received raw message from board");
            }
        }
    }

    async fn auto_detect_port() -> Result<String> {
        let ports = atoman::task::spawn_blocking(|| tokio_serial::available_ports())
            .await
            .map_err(|e| Error::Custom(format!("Failed to join blocking task: {e}")))?
            .map_err(|e| Error::Custom(format!("Failed to list serial ports: {e}")))?;

        let mut candidates = Vec::new();
        for p in ports {
            if let SerialPortType::UsbPort(_) = p.port_type {
                candidates.push(p.port_name);
            }
        }

        if candidates.is_empty() {
            return Err(
                Error::Custom("No USB serial ports found for auto-detection.".into()).into(),
            );
        }

        for port_name in &candidates {
            if let Ok(mut port) = tokio_serial::new(port_name, 115200)
                .timeout(Duration::from_millis(500))
                .open_native_async()
            {
                if port.write_all(b"{\"type\":\"metadata\"}\n").await.is_ok() {
                    let mut reader = BufReader::new(port);
                    let mut line = String::new();
                    if let Ok(Ok(_)) =
                        time::timeout(Duration::from_millis(500), reader.read_line(&mut line)).await
                    {
                        if line.contains(UUID) {
                            return Ok(port_name.clone());
                        }
                    }
                }
            }
        }

        Ok(candidates[0].clone())
    }

    async fn verify_handshake(stream: &mut SerialStream) -> Result<()> {
        stream.write_all(b"{\"type\":\"metadata\"}\n").await?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();

        time::timeout(Duration::from_millis(1000), reader.read_line(&mut line))
            .await
            .map_err(|_| {
                Error::Custom("Device handshake timeout: no response to metadata command".into())
            })?
            .map_err(|e| Error::Custom(format!("Read error during handshake: {e}")))?;

        if !line.contains(UUID) {
            return Err(Error::Custom(format!("Invalid device response: {line}")).into());
        }

        Ok(())
    }

    async fn send_command(
        &self,
        doc: JsonValue,
        reply: Option<oneshot::Sender<Result<String>>>,
    ) -> Result<()> {
        self.cmd_tx
            .send(ControllerCommand::SendJson { doc, reply })
            .await
            .map_err(|_| Error::Custom("Controller worker task is not running".into()).into())
    }

    pub async fn read_metadata(&self) -> Result<ControllerMetadata> {
        let (tx, rx) = oneshot::channel();
        let doc = json!({"type": "metadata"});
        self.send_command(doc, Some(tx)).await?;

        let resp_line = time::timeout(Duration::from_secs(2), rx)
            .await
            .map_err(|_| Error::Custom("Device metadata timeout: no response".into()))?
            .map_err(|_| Error::Custom("Metadata request canceled".into()))??;

        let meta: ControllerMetadata = serde_json::from_str(&resp_line).map_err(|e| {
            Error::Custom(format!(
                "Failed to parse metadata JSON: {e}, raw: {resp_line}"
            ))
        })?;

        Ok(meta)
    }

    pub async fn ping(&self) -> Result<String> {
        let (tx, rx) = oneshot::channel();
        let doc = json!({"type": "ping"});
        self.send_command(doc, Some(tx)).await?;

        let resp_line = time::timeout(Duration::from_secs(2), rx)
            .await
            .map_err(|_| Error::Custom("Device ping timeout: no response".into()))?
            .map_err(|_| Error::Custom("Ping request canceled".into()))??;

        Ok(resp_line)
    }

    /// Triggers a configured alias on the controller asynchronously (with optional hold duration in ms).
    pub async fn set_alias(&self, name: &str, hold: Option<u64>) -> Result<()> {
        let mut doc = json!({"type": "alias", "name": name});
        if let Some(h) = hold {
            doc["hold"] = json!(h);
        }
        self.send_command(doc, None).await
    }

    /// Sets relay state directly.
    pub async fn set_relay(&self, index: usize, enable: bool) -> Result<()> {
        let doc = json!({"type": "relay", "index": index, "enable": enable});
        self.send_command(doc, None).await
    }

    /// Sets servo state directly.
    pub async fn set_servo(&self, index: usize, degs: i32) -> Result<()> {
        let doc = json!({"type": "servo", "index": index, "degs": degs});
        self.send_command(doc, None).await
    }

    /// Sets motor state directly.
    pub async fn set_motor(
        &self,
        index: usize,
        enable: bool,
        side: RotationMode,
        power: f32,
    ) -> Result<()> {
        let doc = json!({"type": "motor", "index": index, "enable": enable, "side": side, "power": power});
        self.send_command(doc, None).await
    }
}

/// Helper method to generate sketch code.
pub fn generate_sketch_code() -> String {
    SKETCH_INO
        .replace("%UUID%", &format!("\"{UUID}\""))
        .replace("%VERSION%", &format!("\"{VERSION}\""))
}

/// Helper method to generate sketch config code.
pub fn generate_config_code() -> String {
    let config = Config::get();
    let binds = &config.binds;
    let max_count = binds.bind_count.max(1);

    let aliases_code: Vec<String> = binds
        .aliases
        .iter()
        .map(|alias| alias.to_cpp_initializer(max_count))
        .collect();

    SKETCH_CONFIG
        .replace("%BIND_COUNT%", &max_count.to_string())
        .replace("%ALIAS_COUNT%", &binds.aliases.len().to_string())
        .replace("%ALIASES%", &aliases_code.join(",\n"))
}
