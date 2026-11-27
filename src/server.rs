//! The hub's HTTP side: a small JSON API over the engine, for the `carbon` command. There is no web page: Carbon is
//! looked at in its app.

use std::{process::Command, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::{
    create,
    engine::{HUB_PORT, Hub},
};

type Shared = State<Arc<Hub>>;
type Reply = Result<Json<Value>, (StatusCode, Json<Value>)>;

fn failed(message: String) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message })))
}

fn is_local(host: &str) -> bool {
    [
        format!("localhost:{HUB_PORT}"),
        format!("127.0.0.1:{HUB_PORT}"),
        format!("[::1]:{HUB_PORT}"),
    ]
    .iter()
    .any(|h| h == host)
}

/// Carbon can start and stop processes, so it only answers requests addressed to its own port on this machine
/// that no web page sent: a browser always says where a request came from, and the `carbon` command never does.
/// That shuts out every site open in a browser, and names that merely resolve to this machine.
async fn local_only(request: Request, next: Next) -> Response {
    let headers = request.headers();
    let host_ok = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .is_some_and(is_local);
    let from_page = headers.contains_key(header::ORIGIN) || headers.contains_key("sec-fetch-site");
    if !host_ok || from_page {
        return (
            StatusCode::FORBIDDEN,
            "Carbon only answers the `carbon` command",
        )
            .into_response();
    }
    next.run(request).await
}

async fn nothing() -> Response {
    (
        StatusCode::NOT_FOUND,
        "Carbon has no web page: open the Carbon app, or use the `carbon` command",
    )
        .into_response()
}

async fn state(State(hub): Shared) -> Json<Value> {
    let mut snapshot = serde_json::to_value(hub.snapshot().await).unwrap_or_default();
    snapshot["templates"] = json!(create::TEMPLATES);
    Json(snapshot)
}

async fn logs(State(hub): Shared, Path(key): Path<String>) -> Json<Vec<String>> {
    Json(hub.logs(&key))
}

#[derive(Deserialize)]
struct ById {
    id: String,
}
#[derive(Deserialize)]
struct ByPort {
    port: u16,
}
#[derive(Deserialize)]
struct ByPid {
    pid: i32,
}
#[derive(Deserialize)]
struct ByProject {
    group: String,
    project: String,
}
#[derive(Deserialize)]
struct NewProject {
    group: String,
    name: String,
    template: String,
}

async fn start(State(hub): Shared, Json(body): Json<ById>) -> Reply {
    hub.start(&body.id)
        .await
        .map(|port| Json(json!({ "port": port })))
        .map_err(failed)
}

async fn start_project(State(hub): Shared, Json(body): Json<ByProject>) -> Reply {
    hub.start_project(&body.group, &body.project)
        .await
        .map(|ports| Json(json!({ "ports": ports })))
        .map_err(failed)
}

async fn stop(State(hub): Shared, Json(body): Json<ByPort>) -> Reply {
    hub.stop(body.port)
        .await
        .map(|()| Json(json!({})))
        .map_err(failed)
}

async fn kill(State(hub): Shared, Json(body): Json<ByPid>) -> Reply {
    hub.kill(body.pid)
        .await
        .map(|()| Json(json!({})))
        .map_err(failed)
}

async fn pin(State(hub): Shared, Json(body): Json<ById>) -> Reply {
    hub.pin(&body.id)
        .map(|key| Json(json!({ "job": key })))
        .map_err(failed)
}

/// Opens a local port in the browser, for `carbon open`
async fn open(State(hub): Shared, Json(body): Json<ByPort>) -> Reply {
    if body.port < 1024 {
        return Err(failed(format!("{} isn't a port Carbon opens", body.port)));
    }
    hub.open(&format!("http://localhost:{}", body.port));
    Ok(Json(json!({})))
}

/// A picture of what a port is serving: a PNG, or nothing yet while the first one is being taken
async fn preview(State(hub): Shared, Path(port): Path<u16>) -> Response {
    match hub.preview(port) {
        Some(png) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            png.as_ref().clone(),
        )
            .into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

async fn new_project(State(hub): Shared, Json(body): Json<NewProject>) -> Reply {
    create::create(&hub, &body.group, &body.name, &body.template)
        .map(|key| Json(json!({ "job": key })))
        .map_err(failed)
}

fn router(hub: Arc<Hub>) -> Router {
    Router::new()
        .route("/api/state", get(state))
        .route("/api/logs/{*key}", get(logs))
        .route("/api/start", post(start))
        .route("/api/start-project", post(start_project))
        .route("/api/stop", post(stop))
        .route("/api/kill", post(kill))
        .route("/api/pin", post(pin))
        .route("/api/create", post(new_project))
        .route("/api/open", post(open))
        .route("/api/preview/{port}", get(preview))
        .fallback(nothing)
        .layer(middleware::from_fn(local_only))
        .with_state(hub)
}

/// The hub's port on both loopback addresses, or nothing if either is taken. "localhost" can mean either address, and
/// a browser tries IPv6 first, so Carbon takes both: nothing else can sit on one of them and answer in its place.
async fn bind() -> Option<Vec<TcpListener>> {
    let mut listeners = Vec::new();
    for address in [format!("127.0.0.1:{HUB_PORT}"), format!("[::1]:{HUB_PORT}")] {
        listeners.push(TcpListener::bind(&address).await.ok()?);
    }
    Some(listeners)
}

fn serve(hub: &Arc<Hub>, listeners: Vec<TcpListener>) {
    for listener in listeners {
        let app = router(hub.clone());
        tokio::spawn(async move { axum::serve(listener, app).await });
    }
}

/// Starts answering on the hub's port and returns once it is. If something already has the port, the error says what.
pub async fn listen(hub: Arc<Hub>) -> Result<(), String> {
    let Some(listeners) = bind().await else {
        let holder = Command::new("lsof")
            .args(["-nP", &format!("-iTCP:{HUB_PORT}"), "-sTCP:LISTEN"])
            .output();
        let holder = holder
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        return Err(format!(
            "Carbon needs port {HUB_PORT} and something else has it:\n{holder}"
        ));
    };
    serve(&hub, listeners);
    Ok(())
}

/// Starts answering on the hub's port if it is free now, and says whether it did. For a Carbon that started while
/// another one had the port: when that one goes, this one takes its place.
pub async fn take_over(hub: Arc<Hub>) -> bool {
    match bind().await {
        Some(listeners) => {
            serve(&hub, listeners);
            true
        }
        None => false,
    }
}
