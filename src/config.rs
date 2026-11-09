use crate::prelude::*;

const MAX_BIND_COUNT: usize = 5;

/// Device action alias configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AliasConfig {
    Relay {
        name: String,
        #[serde(default)]
        index: u8,
        #[serde(default)]
        codes: Vec<String>,
    },
    Servo {
        name: String,
        #[serde(default)]
        index: u8,
        #[serde(default)]
        codes: Vec<String>,
        #[serde(default = "default_min_deg")]
        min_deg: i32,
        #[serde(default = "default_max_deg")]
        max_deg: i32,
        #[serde(default)]
        mid_deg: Option<i32>,
    },
    Motor {
        name: String,
        #[serde(default)]
        index: u8,
        #[serde(default)]
        codes: Vec<String>,
        #[serde(default = "default_side")]
        side: String,
        #[serde(default = "default_power")]
        power: f32,
        #[serde(default)]
        duration: Option<u64>,
        #[serde(default)]
        hold_enabled: bool,
    },
}

fn default_min_deg() -> i32 {
    0
}
fn default_max_deg() -> i32 {
    180
}
fn default_side() -> String {
    "right".into()
}
fn default_power() -> f32 {
    1.0
}

impl AliasConfig {
    /// Generates C++ struct initializer for the sketch config.
    pub fn to_cpp_initializer(&self, max_count: usize) -> String {
        let (
            name,
            kind,
            index,
            codes,
            min_deg,
            max_deg,
            mid_deg,
            side,
            power,
            duration,
            hold_enabled,
        ) = match self {
            AliasConfig::Relay { name, index, codes } => (
                name.clone(),
                "relay",
                *index,
                codes.clone(),
                0,
                180,
                -1,
                "".into(),
                1.0,
                0,
                false,
            ),
            AliasConfig::Servo {
                name,
                index,
                codes,
                min_deg,
                max_deg,
                mid_deg,
            } => (
                name.clone(),
                "servo",
                *index,
                codes.clone(),
                *min_deg,
                *max_deg,
                mid_deg.unwrap_or(-1),
                "".into(),
                1.0,
                0,
                false,
            ),
            AliasConfig::Motor {
                name,
                index,
                codes,
                side,
                power,
                duration,
                hold_enabled,
            } => (
                name.clone(),
                "motor",
                *index,
                codes.clone(),
                0,
                180,
                -1,
                side.clone(),
                *power,
                duration.unwrap_or(0),
                *hold_enabled,
            ),
        };

        let mut items: Vec<String> = codes
            .iter()
            .take(max_count)
            .map(|s| {
                if s.starts_with("0x") || s.starts_with("0X") {
                    s.clone()
                } else {
                    format!("0x{:X}", s.parse::<u64>().unwrap_or(0))
                }
            })
            .collect();

        while items.len() < max_count {
            items.push("0".into());
        }

        let codes_str = format!("{{{}}}", items.join(", "));
        let side_str = if side.is_empty() {
            "\"\""
        } else {
            &format!("\"{}\"", side)
        };

        format!(
            "    {{\
                \"{name}\", \
                \"{kind}\", \
                {index}, \
                {codes_str}, \
                {min_deg}, \
                {max_deg}, \
                {mid_deg}, \
                {side_str}, \
                {power:.1}f, \
                {duration}, \
                {hold_enabled} \
            }}"
        )
    }
}

/// Sketch options for controller.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ControllerBinds {
    #[serde(default = "ControllerBinds::default_bind_count")]
    pub bind_count: usize,

    #[serde(default)]
    pub aliases: Vec<AliasConfig>,
}

impl ControllerBinds {
    fn default_bind_count() -> usize {
        MAX_BIND_COUNT
    }
}

impl Default for ControllerBinds {
    fn default() -> Self {
        Self {
            bind_count: 5,
            aliases: vec![
                AliasConfig::Relay {
                    name: "backlight".into(),
                    index: 0,
                    codes: vec!["0x8AB3679B".into()],
                },
                AliasConfig::Servo {
                    name: "light".into(),
                    index: 0,
                    codes: vec!["0x12345678".into()],
                    min_deg: 0,
                    max_deg: 180,
                    mid_deg: Some(90),
                },
            ],
        }
    }
}

/// Agent configuration.
#[atoman::config]
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub binds: ControllerBinds,
}
