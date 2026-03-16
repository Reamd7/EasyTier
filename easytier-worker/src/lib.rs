mod config;
mod internal_api;
mod protocol;
mod router;
mod state;

pub mod r#do;

use worker::*;

use crate::router::{classify, health, websocket_response, RouteKind};

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let method = req.method().clone();
    let url = req.url()?;
    let is_websocket_upgrade = req
        .headers()
        .get("Upgrade")?
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    match classify(&method, url.path(), is_websocket_upgrade) {
        RouteKind::Health => health(),
        RouteKind::RelayUpgrade => websocket_response(req, &env).await,
        RouteKind::NotFound => Response::error("Not Found", 404),
    }
}
