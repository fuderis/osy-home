pub mod home;

use crate::prelude::*;

use anylm::Tool;
use osy_skill::AgentSkill;

/// System agent skills.
#[derive(AgentSkill, Clone, Copy, Debug, Display, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
#[display(rename_all = "snake_case")]
pub enum SkillKind {
    #[skill(
        module = "home",
        description = "Smart home controlling (relay, motors, servo, etc.)."
    )]
    #[tool(
        name = "update",
        description = "Connects to the smart home controller, checks its firmware version against the latest build, and flashes or updates the device if necessary."
    )]
    #[tool(
        name = "alias",
        description = "Triggers a pre-configured device alias by name on the controller, supporting custom macros, automated cycles, and optional hold durations."
    )]
    #[tool(
        name = "relay",
        description = "Controls smart home relays (1 to 4) by turning them on or off."
    )]
    #[tool(
        name = "servo",
        description = "Controls servo drives (1 to 4) by specifying degrees or target state."
    )]
    #[tool(
        name = "motor",
        description = "Controls DC motors (1 to 2) by setting direction and state."
    )]
    Home,
}
