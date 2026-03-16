use worker::{Env, Method, Request, Response, Result};

use crate::config::{HEALTH_PATH, RELAY_PATH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteKind {
    Health,
    RelayUpgrade,
    NotFound,
}

pub fn classify(method: &Method, path: &str, websocket_upgrade: bool) -> RouteKind {
    if *method == Method::Get && path == HEALTH_PATH {
        return RouteKind::Health;
    }

    if *method == Method::Get && path == RELAY_PATH && websocket_upgrade {
        return RouteKind::RelayUpgrade;
    }

    RouteKind::NotFound
}

pub fn health() -> Result<Response> {
    Response::ok("ok")
}

pub async fn websocket_response(req: Request, env: &Env) -> Result<Response> {
    let relay_namespace = env.durable_object("RELAY_SHARD")?;
    let relay_stub = relay_namespace.get_by_name("default")?;
    relay_stub.fetch_with_request(req).await
}

#[cfg(test)]
mod tests {
    use worker::Method;

    use super::{classify, RouteKind};

    #[test]
    fn classifies_health_route() {
        assert_eq!(classify(&Method::Get, "/healthz", false), RouteKind::Health);
    }

    #[test]
    fn classifies_relay_upgrade_route() {
        assert_eq!(classify(&Method::Get, "/relay", true), RouteKind::RelayUpgrade);
    }

    #[test]
    fn rejects_non_upgrade_relay_request() {
        assert_eq!(classify(&Method::Get, "/relay", false), RouteKind::NotFound);
    }
}
