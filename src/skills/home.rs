use crate::{controller::*, prelude::*};

use atoman::{fs, time};
use tokio_serial::SerialPortType;

#[derive(Debug, Deserialize, Schema)]
pub struct UpdateAction {
    /// Serial device port (optional).
    pub port: Option<String>,
}

#[derive(Debug, Deserialize, Schema)]
pub struct AliasAction {
    /// Alias name to execute.
    pub action: String,
    /// Optional hold duration in milliseconds (for motor holding simulation).
    pub hold: Option<u64>,
}

#[derive(Debug, Deserialize, Schema)]
pub struct RelayAction {
    /// Relay index.
    #[schema(min = 1, max = 4)]
    pub index: usize,
    /// Relay state (enable/disable).
    pub enable: bool,
}

#[derive(Debug, Deserialize, Schema)]
pub struct ServoAction {
    /// Servo index.
    #[schema(min = 1, max = 4)]
    pub index: usize,
    /// Servo rotation degress.
    #[schema(min = 0, max = 180)]
    pub degs: i32,
}

#[derive(Debug, Display, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[display(rename_all = "lowercase")]
pub enum RotationMode {
    Left,
    Right,
}

#[derive(Debug, Deserialize, Schema)]
pub struct MotorAction {
    /// Motor index.
    #[schema(min = 1, max = 2)]
    pub index: usize,
    /// Motor state.
    pub enable: bool,
    /// Motor rotation mode.
    #[schema(serde_json)]
    #[schema(variants = ["left", "right"])]
    pub side: RotationMode,
    /// Motor power percentage.
    #[schema(min = 0.0, max = 1.0)]
    pub power: f32,
}

#[log()]
pub async fn handle_update(tx: Sender<Bytes>, query: ToolQuery<UpdateAction>) -> Result<()> {
    let ToolQuery { payload, .. } = query;

    tx.send(Event::Thinking(
        "Checking controller connection and firmware version...".into(),
    ))?;

    // try init existing controller
    let mut need_flash = false;
    let init_success = Controller::init(payload.port.clone()).await?;

    if init_success {
        let mut ctrl_lock = Controller::lock().await;
        let ctrl = ctrl_lock
            .as_mut()
            .ok_or(Error::Custom("Failed to lock controller device.".into()))?;

        match ctrl.read_metadata().await {
            Ok(meta) => {
                info!(
                    "Connected controller version: `{}`, expected: `{VERSION}`",
                    meta.version
                );

                if meta.version != VERSION {
                    need_flash = true;
                } else {
                    if let Ok(resp) = ctrl.ping().await {
                        let port_name = ctrl.port_name.clone();
                        drop(ctrl_lock);
                        let msg = format!(
                            "Controller is already up to date and connected on `{port_name}`. Ping: {resp}"
                        );
                        info!("{msg}");
                        tx.send(Event::Answer(msg))?;
                        return Ok(());
                    }
                }
            }
            Err(_) => {
                warn!("Failed to get metadata from controller, forcing re-flash...");
                need_flash = true;
            }
        }
    } else {
        info!("Controller not detected via handshake. Preparing for clean flash...");
        need_flash = true;
    }

    // get custom port or trying to find automatically
    let port = if let Some(p) = payload.port.clone() {
        p
    } else {
        let ports = atoman::spawn_blocking(|| tokio_serial::available_ports())
            .await
            .map_err(|e| Error::Custom(format!("Failed to join blocking task: {e}")))?
            .map_err(|e| Error::Custom(format!("Failed to list serial ports: {e}")))?;

        let mut candidate = None;
        for p in ports {
            if let SerialPortType::UsbPort(_) = p.port_type {
                candidate = Some(p.port_name);
                break;
            }
        }

        if let Some(p) = candidate {
            p
        } else {
            let confirmation = Dialog::confirm(
                &tx,
                "No USB serial ports found automatically. Disconnect other devices and click Yes when ready to flash.",
                Some(Confirmation::Yes),
                Duration::from_secs(120),
            ).await?;

            if let Some(conf) = confirmation
                && conf.confirmed()
            {
                // read available devices list
                let retry_ports = atoman::spawn_blocking(|| tokio_serial::available_ports())
                    .await
                    .map_err(|e| Error::Custom(format!("Failed to join blocking task: {e}")))?
                    .map_err(|e| Error::Custom(format!("Failed to list serial ports: {e}")))?;

                // find arduino device
                retry_ports
                    .into_iter()
                    .find(|p| matches!(p.port_type, SerialPortType::UsbPort(_)))
                    .map(|p| p.port_name)
                    .ok_or_else(|| Error::Custom("No USB device available for flashing.".into()))?
            } else {
                return Err(
                    Error::Custom("Flash aborted by user: no device available.".into()).into(),
                );
            }
        }
    };

    // flash device if needed
    if need_flash {
        tx.send(Event::Thinking(
            "Preparing workspace and rendering config.h...".into(),
        ))?;

        let work_dir = path!("$temp/osy-home/sketch");
        fs::create_dir_all(&work_dir).await?;

        fs::write(work_dir.join("sketch.ino"), generate_sketch_code()).await?;
        fs::write(work_dir.join("config.h"), generate_config_code()).await?;

        tx.send(Event::Thinking(
            format!("Compiling and flashing on port {port}...").into(),
        ))?;

        // free USB port before flashing
        Controller::free().await;
        time::sleep(Duration::from_millis(500)).await;

        // gen flash command
        let cmd = format!(
            "arduino-cli compile --fqbn arduino:avr:micro \"{}\" && arduino-cli upload -p {} --fqbn arduino:avr:micro \"{}\"",
            work_dir.display(),
            port,
            work_dir.display()
        );

        // send command to user CLI
        let result = Dialog::script(&tx, cmd, true, Duration::from_secs(180))
            .await?
            .ok_or(Error::OperationTimeout)?;

        if !result.success {
            return Err(Error::Custom(format!("Flash failed: {}", result.stderr)).into());
        }

        // re-init arduino device at finish
        Controller::init(Some(port.clone())).await?;

        let msg = format!("Pro Micro successfully flashed, initialized and connected on `{port}`.");
        info!("{msg}");
        tx.send(Event::Answer(msg))?;
    }

    Ok(())
}

#[log()]
pub async fn handle_alias(tx: Sender<Bytes>, query: ToolQuery<AliasAction>) -> Result<()> {
    let ToolQuery { payload, .. } = query;

    // try to lock controller state
    let mut ctrl_lock = Controller::lock().await;
    let ctrl = ctrl_lock
        .as_mut()
        .ok_or_else(|| Error::Custom("Controller not connected.".into()))?;

    // send alias command to controller
    ctrl.set_alias(&payload.action, payload.hold).await?;

    let msg = if let Some(h) = payload.hold {
        format!(
            "Alias `{}` successfully triggered with hold `{}` ms.",
            payload.action, h
        )
    } else {
        format!("Alias `{}` successfully triggered.", payload.action)
    };

    info!("{msg}");
    tx.send(Event::Answer(msg))?;

    Ok(())
}

#[log()]
pub async fn handle_relay(tx: Sender<Bytes>, query: ToolQuery<RelayAction>) -> Result<()> {
    let ToolQuery { payload, .. } = query;

    // try to lock controller state
    let mut ctrl_lock = Controller::lock().await;
    let ctrl = ctrl_lock.as_mut().ok_or_else(|| {
        Error::Custom("Controller not connected. Run update/connect first.".into())
    })?;

    // send relay command
    ctrl.set_relay(payload.index, payload.enable).await?;

    let msg = format!(
        "Relay `{}` is `{}`.",
        payload.index,
        if payload.enable {
            "activated"
        } else {
            "deactivated"
        }
    );
    info!("{msg}");
    tx.send(Event::Answer(msg))?;

    Ok(())
}

#[log()]
pub async fn handle_servo(tx: Sender<Bytes>, query: ToolQuery<ServoAction>) -> Result<()> {
    let ToolQuery { payload, .. } = query;

    // try to lock controller state
    let mut ctrl_lock = Controller::lock().await;
    let ctrl = ctrl_lock
        .as_mut()
        .ok_or_else(|| Error::Custom("Controller not connected.".into()))?;

    // send servo command
    ctrl.set_servo(payload.index, payload.degs).await?;

    let msg = format!(
        "Servo `{}` rotated by `{}` degs.",
        payload.index, payload.degs
    );
    info!("{msg}");
    tx.send(Event::Answer(msg))?;

    Ok(())
}

#[log()]
pub async fn handle_motor(tx: Sender<Bytes>, query: ToolQuery<MotorAction>) -> Result<()> {
    let ToolQuery { payload, .. } = query;

    // try to lock controller state
    let mut ctrl_lock = Controller::lock().await;
    let ctrl = ctrl_lock
        .as_mut()
        .ok_or_else(|| Error::Custom("Controller not connected.".into()))?;

    // send motor command
    ctrl.set_motor(payload.index, payload.enable, payload.side, payload.power)
        .await?;

    let msg = format!(
        "Motor `{}` is `{}` (to {}, {}% PWM).",
        payload.index,
        if payload.enable {
            "enabled"
        } else {
            "disabled"
        },
        payload.side,
        (payload.power * 100.0) as i32
    );
    info!("{msg}");
    tx.send(Event::Answer(msg))?;

    Ok(())
}
