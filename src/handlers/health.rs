use crate::prelude::*;

/// API: Handles server ping.
#[log()]
pub async fn handle_ping() -> Response {
    Response::ok().text("pong")
}

/// API: Refreshes agent config.
#[log()]
pub async fn handle_refresh() -> Response {
    if let Err(e) = Config::update().await {
        return Response::error().text(e.to_string());
    }
    Response::ok()
}
