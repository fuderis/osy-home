use crate::{prelude::*, skills::SkillKind};
use osy_share::{SkillExt, ToolQuery};

/// API: Handles skills list receiving.
#[log()]
pub async fn handle_skills_list() -> Response {
    let skills = SkillKind::skills_list();
    Response::ok().json(&skills)
}

/// API: Handles tools list receiving.
#[log()]
pub async fn handle_tools_list(skill: Paths<SkillKind>) -> Response {
    let tools = skill.tools_list();
    Response::ok().json(&tools)
}

/// API: Handles agent tool call.
#[log(skill = %paths.0, tool = %paths.1)]
pub async fn handle_tool_call(
    Paths(paths): Paths<(SkillKind, String)>,
    query: Json<ToolQuery<JsonValue>>,
) -> Response {
    let (skill, tool) = paths;
    info!("Initialized the `{skill}.{tool}` tool handling");

    Response::ok().stream(async move |tx| {
        if let Err(e) = skill.tool_call(tx.clone(), tool.clone(), query.0).await {
            error!("[handle_tool_call{{skill={skill}, tool={tool}}}] {e}");
            tx.send(Event::Error(e.to_string())).ok();
        }
    })
}
