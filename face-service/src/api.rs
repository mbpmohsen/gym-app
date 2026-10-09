//! Local HTTP API (loopback only). All /v1 routes need the shared token
//! (`Authorization: Bearer <token>`, or `?token=` for <img>/EventSource which
//! can't set headers).
//!
//!   GET    /v1/health
//!   GET    /v1/gallery                      enrolled members
//!   DELETE /v1/gallery/{member_id}
//!   POST   /v1/enroll/{member_id}/start     body (optional): {"samples": 8}
//!   GET    /v1/enroll                       current session status
//!   POST   /v1/enroll/{member_id}/commit    save a Ready session
//!   DELETE /v1/enroll                       cancel
//!   GET    /v1/events                       SSE; resume with Last-Event-ID or ?since=<id>
//!   GET    /v1/preview                      MJPEG with face boxes
//!   GET    /v1/snapshots/{id}.jpg
//!   GET    /v1/config   PUT /v1/config      {"threshold", "cooldown_secs", "max_fps"} (partial)
//!   POST   /v1/camera                       {"active": false} releases the camera, true reopens it
//!   GET    /                                test page

use std::convert::Infallible;
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;

use axum::{
    body::{Body, Bytes},
    extract::{Path, Query, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event as SseEvent, KeepAlive, Sse},
        Html, IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use futures_util::{stream, Stream, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::{broadcast::error::RecvError, oneshot};

use crate::config::{Loaded, Recognition};
use crate::enrollment::valid_member_id;
use crate::worker::{render_event, Command, Shared};

#[derive(Clone)]
pub struct AppState {
    pub shared: Arc<Shared>,
    pub cmd: mpsc::Sender<Command>,
    pub config: Arc<Mutex<Loaded>>,
    pub started: Instant,
}

type ApiResult = Result<Response, (StatusCode, Json<Value>)>;

fn err(code: StatusCode, msg: impl ToString) -> (StatusCode, Json<Value>) {
    (code, Json(json!({ "error": msg.to_string() })))
}

fn internal(e: impl std::fmt::Display) -> (StatusCode, Json<Value>) {
    err(StatusCode::INTERNAL_SERVER_ERROR, e)
}

pub fn router(state: AppState) -> Router {
    let v1 = Router::new()
        .route("/health", get(health))
        .route("/gallery", get(gallery))
        .route("/gallery/{member_id}", axum::routing::delete(delete_member))
        .route("/enroll", get(enroll_status).delete(enroll_cancel))
        .route("/enroll/{member_id}/start", post(enroll_start))
        .route("/enroll/{member_id}/commit", post(enroll_commit))
        .route("/events", get(events))
        .route("/preview", get(preview))
        .route("/snapshots/{file}", get(snapshot))
        .route("/config", get(get_config).put(put_config))
        .route("/camera", axum::routing::post(set_camera_active))
        .layer(middleware::from_fn_with_state(state.clone(), auth));
    Router::new()
        .route("/", get(|| async { Html(include_str!("web/index.html")) }))
        .nest("/v1", v1)
        .with_state(state)
}

async fn auth(State(s): State<AppState>, req: Request, next: Next) -> Response {
    let expected = s.config.lock().unwrap().config.token.clone();
    if expected.is_empty() {
        return next.run(req).await;
    }
    let from_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);
    let from_query = req
        .uri()
        .query()
        .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("token=")))
        .map(str::to_string);
    if from_header.or(from_query).as_deref() == Some(expected.as_str()) {
        next.run(req).await
    } else {
        err(StatusCode::UNAUTHORIZED, "missing or wrong token").into_response()
    }
}

async fn health(State(s): State<AppState>) -> ApiResult {
    let st = s.shared.status.lock().unwrap().clone();
    let members = s.shared.store.list_members().map_err(internal)?;
    let samples: i64 = members.iter().map(|m| m.samples).sum();
    Ok(Json(json!({
        "status": if st.camera_connected { "ok" } else { "degraded" },
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_secs": s.started.elapsed().as_secs(),
        "camera": { "name": st.camera, "connected": st.camera_connected, "active": s.shared.camera_active.load(Ordering::Relaxed) },
        "fps": st.fps,
        "avg_ms": st.avg_ms,
        "frames_processed": st.frames_processed,
        "members": members.len(),
        "samples": samples,
        "enroll": st.enroll,
    }))
    .into_response())
}

async fn gallery(State(s): State<AppState>) -> ApiResult {
    Ok(Json(s.shared.store.list_members().map_err(internal)?).into_response())
}

fn reload_gallery(s: &AppState) -> Result<(), (StatusCode, Json<Value>)> {
    let g = s.shared.store.load_gallery().map_err(internal)?;
    s.cmd.send(Command::SetGallery(g)).map_err(internal)
}

async fn delete_member(State(s): State<AppState>, Path(member_id): Path<String>) -> ApiResult {
    if !s.shared.store.delete_member(&member_id).map_err(internal)? {
        return Err(err(StatusCode::NOT_FOUND, "no such member"));
    }
    reload_gallery(&s)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(Deserialize, Default)]
struct StartBody {
    samples: Option<usize>,
}

async fn enroll_start(State(s): State<AppState>, Path(member_id): Path<String>, body: Option<Json<StartBody>>) -> ApiResult {
    if !valid_member_id(&member_id) {
        return Err(err(StatusCode::BAD_REQUEST, "member_id: 1-64 chars of a-z A-Z 0-9 - _"));
    }
    let target = body.map(|b| b.0.samples).unwrap_or_default().unwrap_or(8).clamp(3, 20);
    let (tx, rx) = oneshot::channel();
    s.cmd.send(Command::StartEnroll { member_id, target, reply: tx }).map_err(internal)?;
    match rx.await.map_err(internal)? {
        Ok(()) => Ok((StatusCode::ACCEPTED, Json(json!({ "state": "collecting", "target": target }))).into_response()),
        Err(e) => Err(err(StatusCode::CONFLICT, e)),
    }
}

async fn enroll_status(State(s): State<AppState>) -> ApiResult {
    let st = s.shared.status.lock().unwrap().enroll.clone();
    Ok(match st {
        Some(e) => Json(json!(e)).into_response(),
        None => Json(json!({ "state": "idle" })).into_response(),
    })
}

async fn enroll_cancel(State(s): State<AppState>) -> ApiResult {
    s.cmd.send(Command::CancelEnroll).map_err(internal)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn enroll_commit(State(s): State<AppState>, Path(member_id): Path<String>) -> ApiResult {
    let (tx, rx) = oneshot::channel();
    s.cmd.send(Command::TakeEnrollment { member_id: member_id.clone(), reply: tx }).map_err(internal)?;
    let Some(samples) = rx.await.map_err(internal)? else {
        return Err(err(StatusCode::CONFLICT, "no finished enrollment for this member"));
    };
    s.shared.store.put_member(&member_id, &samples).map_err(internal)?;
    reload_gallery(&s)?;
    Ok(Json(json!({ "member_id": member_id, "samples": samples.len() })).into_response())
}

#[derive(Deserialize)]
struct EventsQuery {
    since: Option<i64>,
}

async fn events(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<EventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>, (StatusCode, Json<Value>)> {
    let last_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .or(q.since);

    // subscribe BEFORE reading the backlog so nothing falls in between; dedupe by id
    let rx = s.shared.events.subscribe();
    let backlog = match last_id {
        Some(id) => s.shared.store.events_after(id).map_err(internal)?,
        None => Vec::new(),
    };
    let mut max_sent = last_id.unwrap_or(0);
    let backlog: Vec<_> = backlog
        .into_iter()
        .map(|(id, body)| {
            max_sent = id;
            Ok(SseEvent::default().id(id.to_string()).data(render_event(id, &body)))
        })
        .collect();

    let live = stream::unfold((rx, max_sent), |(mut rx, max)| async move {
        loop {
            match rx.recv().await {
                Ok((id, json)) if id > max => {
                    return Some((Ok(SseEvent::default().id(id.to_string()).data(json)), (rx, id)));
                }
                Ok(_) => continue,
                // too slow: end the stream; EventSource reconnects with
                // Last-Event-ID and catches up from the database
                Err(RecvError::Lagged(_)) | Err(RecvError::Closed) => return None,
            }
        }
    });
    Ok(Sse::new(stream::iter(backlog).chain(live)).keep_alive(KeepAlive::default()))
}

/// Decrements the preview viewer count when the stream is dropped.
struct PreviewClient(Arc<Shared>);
impl Drop for PreviewClient {
    fn drop(&mut self) {
        self.0.preview_clients.fetch_sub(1, Ordering::Relaxed);
    }
}

async fn preview(State(s): State<AppState>) -> Response {
    s.shared.preview_clients.fetch_add(1, Ordering::Relaxed);
    let guard = PreviewClient(s.shared.clone());
    let rx = s.shared.preview.subscribe();
    let body = stream::unfold((rx, guard), |(mut rx, guard)| async move {
        loop {
            if rx.changed().await.is_err() {
                return None;
            }
            let Some(jpg) = rx.borrow_and_update().clone() else { continue };
            let mut part = format!("--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n", jpg.len()).into_bytes();
            part.extend_from_slice(&jpg);
            part.extend_from_slice(b"\r\n");
            return Some((Ok::<_, Infallible>(Bytes::from(part)), (rx, guard)));
        }
    });
    (
        [
            (header::CONTENT_TYPE, "multipart/x-mixed-replace; boundary=frame"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        Body::from_stream(body),
    )
        .into_response()
}

async fn snapshot(State(s): State<AppState>, Path(file): Path<String>) -> ApiResult {
    // only "<digits>.jpg": no path tricks
    let ok = file.strip_suffix(".jpg").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    if !ok {
        return Err(err(StatusCode::BAD_REQUEST, "bad snapshot name"));
    }
    match tokio::fs::read(s.shared.snapshots_dir.join(&file)).await {
        Ok(bytes) => Ok(([(header::CONTENT_TYPE, "image/jpeg")], bytes).into_response()),
        Err(_) => Err(err(StatusCode::NOT_FOUND, "snapshot not found (expired?)")),
    }
}

#[derive(Deserialize)]
struct CameraBody {
    active: bool,
}

/// `{"active": false}` releases the camera (its light goes off); `true` reopens it.
async fn set_camera_active(State(s): State<AppState>, Json(b): Json<CameraBody>) -> ApiResult {
    let was = s.shared.camera_active.swap(b.active, Ordering::Relaxed);
    if was != b.active {
        tracing::info!("camera {} by client", if b.active { "resumed" } else { "paused" });
    }
    Ok(Json(json!({ "active": b.active })).into_response())
}

async fn get_config(State(s): State<AppState>) -> ApiResult {
    let c = s.config.lock().unwrap().config.clone();
    Ok(Json(json!({ "camera": c.camera, "recognition": c.recognition })).into_response())
}

async fn put_config(State(s): State<AppState>, Json(patch): Json<Value>) -> ApiResult {
    let mut loaded = s.config.lock().unwrap();
    let mut merged = serde_json::to_value(&loaded.config.recognition).map_err(internal)?;
    let Value::Object(fields) = patch else {
        return Err(err(StatusCode::BAD_REQUEST, "expected a JSON object"));
    };
    for (k, v) in fields {
        if merged.get(&k).is_none() {
            return Err(err(StatusCode::BAD_REQUEST, format!("unknown or read-only field: {k}")));
        }
        merged[k] = v;
    }
    let r: Recognition = serde_json::from_value(merged).map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    r.validate().map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    loaded.config.recognition = r.clone();
    loaded.save().map_err(internal)?;
    s.cmd.send(Command::SetRecognition(r.clone())).map_err(internal)?;
    Ok(Json(json!(r)).into_response())
}
