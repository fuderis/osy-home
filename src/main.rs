// Copyright (C) 2026 Bulat Sh. (fuderis) <synapdrake@ya.ru>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

pub mod config;
pub mod error;
pub mod prelude;

pub mod utils;

pub mod handlers;
pub mod skills;

pub mod controller;

use controller::Controller;
use prelude::*;
use skills::SkillKind;

use osy_share::AgentMeta;
use pearce::Server;
use rigging::{CommandContext, Commands, pkg_meta};

#[atoman::main]
async fn main() -> Result<()> {
    osy_share::macos_proc_protect!();

    // init agent metadata
    AgentMeta::init(pkg_meta!(), SkillKind::skills_list()).await;

    // init settings && logger
    Config::init(path!("$config$/config.toml")).await?;
    Logger::init(path!("$state$/logs"), 1000).await?;

    // init arduino controller
    Controller::init(None).await?;

    // handle arguments
    Commands::new()
        .meta(pkg_meta!())
        .hide_cmd("serve", "Runs the agent server", serve)
        .cmd(
            "metadata",
            "Prints agent metadata in JSON format and exits",
            |_| async move {
                println!("{}", AgentMeta::get().to_json_string());
                Ok(())
            },
        )
        .run()
        .await
}

async fn serve(_: CommandContext) -> Result<()> {
    use handlers as hands;

    let sock_path = AgentMeta::get().sock_path.clone();
    info!("Launching on `{}`...", sock_path.display());

    // start server
    Server::new()
        //    HEALTH
        .get("/ping", hands::handle_ping)
        .get("/refresh", hands::handle_refresh)
        //    SKILLS
        .post("/skills/list", hands::handle_skills_list)
        .post("/skills/{skill}/tools", hands::handle_tools_list)
        .post("/skills/{skill}/call/{tool}", hands::handle_tool_call)
        .callback(true)
        .run(sock_path)
        .await
}
