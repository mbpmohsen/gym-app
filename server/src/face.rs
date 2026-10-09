//! Bridge to face-service (SPEC §2). The browser never talks to face-service:
//! its token stays here and every call goes through the admin session.
//!
//! Also keeps both galleries in step (SPEC §4.10): at startup and every 10 minutes,
//! faces of deleted/archived members are removed from face-service, and members whose
//! face is missing there get `face_enrolled = 0`.

use std::time::Duration;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use futures_util::TryStreamExt;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{ApiError, ApiResult};
use crate::AppState;

const RECONCILE_EVERY: Duration = Duration::from_secs(600);
/// face-service may start after us (both start with Windows): retry soon
const RECONCILE_RETRY: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct FaceClient {
    http: reqwest::Client,
    /// separate client without a total timeout, for the never-ending preview stream
    stream: reqwest::Client,
    base: String,
    token: String,
}

fn unreachable() -> ApiError {
    ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "face_service_down", "سرویس تشخیص چهره در دسترس نیست")
}

impl FaceClient {
    pub fn new(base: &str, token: &str) -> Self {
        Self {
            http: reqwest::Client::builder().timeout(Duration::from_secs(5)).build().expect("http client"),
            stream: reqwest::Client::builder().connect_timeout(Duration::from_secs(3)).build().expect("http client"),
            base: base.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/v1{path}", self.base)
    }

    /// Sends a request; maps face-service errors to ours (keeping its message for 4xx).
    pub(crate) async fn call(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> ApiResult<Value> {
        let mut req = self.http.request(method, self.url(path)).bearer_auth(&self.token);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let res = req.send().await.map_err(|e| {
            tracing::warn!("face-service {path}: {e}");
            unreachable()
        })?;
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        if status.is_success() {
            return Ok(serde_json::from_str(&text).unwrap_or(Value::Null));
        }
        let msg = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["error"].as_str().map(str::to_string)).unwrap_or(text);
        tracing::warn!("face-service {path} -> {status}: {msg}");
        Err(match status.as_u16() {
            401 => ApiError::new(StatusCode::BAD_GATEWAY, "face_service_auth", "توکن سرویس تشخیص چهره اشتباه است (تنظیمات gym-server.toml)"),
            404 => ApiError::not_found(msg),
            409 => ApiError::conflict(match msg.as_str() {
                "another enrollment is in progress" => "ثبت چهره‌ی دیگری در جریان است".into(),
                "no finished enrollment for this member" => "ثبت چهره هنوز تمام نشده است".into(),
                _ => msg,
            }),
            _ => ApiError::new(StatusCode::BAD_GATEWAY, "face_service_error", format!("خطای سرویس تشخیص چهره: {msg}")),
        })
    }

    pub async fn delete_face(&self, member_id: i64) -> ApiResult<()> {
        match self.call(reqwest::Method::DELETE, &format!("/gallery/{member_id}"), None).await {
            Ok(_) => Ok(()),
            Err(e) if e.status() == StatusCode::NOT_FOUND => Ok(()), // already gone
            Err(e) => Err(e),
        }
    }

    /// Sets face-service's recognition cooldown if it differs.
    pub async fn ensure_cooldown(&self, secs: f64) -> ApiResult<()> {
        let c = self.call(reqwest::Method::GET, "/config", None).await?;
        if c["recognition"]["cooldown_secs"].as_f64().is_some_and(|v| (v - secs).abs() < 0.5) {
            return Ok(());
        }
        self.call(reqwest::Method::PUT, "/config", Some(json!({ "cooldown_secs": secs }))).await?;
        tracing::info!("face-service cooldown set to {secs}s");
        Ok(())
    }

    /// Turns the camera on or releases it (light off).
    pub async fn set_camera(&self, active: bool) -> ApiResult<()> {
        self.call(reqwest::Method::POST, "/camera", Some(json!({ "active": active }))).await.map(|_| ())
    }

    /// The event stream (SSE), resumed after `last_id`.
    pub async fn events(&self, last_id: Option<&str>) -> anyhow::Result<reqwest::Response> {
        let mut req = self.stream.get(self.url("/events")).bearer_auth(&self.token);
        if let Some(id) = last_id {
            req = req.header("Last-Event-ID", id);
        }
        let res = req.send().await?;
        anyhow::ensure!(res.status().is_success(), "face-service /events -> {}", res.status());
        Ok(res)
    }

    /// JPEG of an event's face, if face-service still has it.
    pub async fn snapshot(&self, id: i64) -> Option<bytes::Bytes> {
        let res = self.http.get(self.url(&format!("/snapshots/{id}.jpg"))).bearer_auth(&self.token).send().await.ok()?;
        if !res.status().is_success() {
            return None;
        }
        res.bytes().await.ok()
    }

    async fn gallery_ids(&self) -> ApiResult<Vec<i64>> {
        let v = self.call(reqwest::Method::GET, "/gallery", None).await?;
        Ok(v.as_array()
            .map(|a| a.iter().filter_map(|m| m["member_id"].as_str()?.parse().ok()).collect())
            .unwrap_or_default())
    }
}

// ---------- routes ----------

/// Status for the header indicator: service reachable? camera connected?
pub async fn health(State(s): State<AppState>) -> Json<Value> {
    match s.face.call(reqwest::Method::GET, "/health", None).await {
        Ok(h) => Json(json!({ "reachable": true, "camera": h["camera"], "viewers": s.live.viewers(), "fps": h["fps"], "enroll": h["enroll"], "events": s.live.connected() })),
        Err(e) => Json(json!({ "reachable": false, "error": e.message() })),
    }
}

#[derive(Deserialize)]
pub struct StartBody {
    #[serde(default = "eight")]
    samples: u32,
}
fn eight() -> u32 {
    8
}

/// Body optional: `{"samples": n}` or nothing.
pub async fn enroll_start(State(s): State<AppState>, Path(id): Path<i64>, body: axum::body::Bytes) -> ApiResult<Json<Value>> {
    let archived: bool = s
        .db
        .conn()
        .query_row("SELECT archived FROM members WHERE id = ?1", [id], |r| r.get(0))
        .map_err(|_| ApiError::not_found("عضو پیدا نشد"))?;
    if archived {
        return Err(ApiError::bad_request("این عضو بایگانی شده است"));
    }
    let samples = if body.iter().all(u8::is_ascii_whitespace) {
        eight()
    } else {
        serde_json::from_slice::<StartBody>(&body).map_err(|_| ApiError::bad_request("درخواست نامعتبر است"))?.samples
    };
    Ok(Json(s.face.call(reqwest::Method::POST, &format!("/enroll/{id}/start"), Some(json!({ "samples": samples }))).await?))
}

pub async fn enroll_status(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(Json(s.face.call(reqwest::Method::GET, "/enroll", None).await?))
}

pub async fn enroll_cancel(State(s): State<AppState>) -> ApiResult<StatusCode> {
    s.face.call(reqwest::Method::DELETE, "/enroll", None).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Saves the finished enrollment in face-service, then marks the member enrolled.
pub async fn enroll_commit(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let r = s.face.call(reqwest::Method::POST, &format!("/enroll/{id}/commit"), None).await?;
    s.db.conn().execute("UPDATE members SET face_enrolled = 1 WHERE id = ?1", [id])?;
    tracing::info!("face enrolled for member {id}");
    Ok(Json(r))
}

pub async fn delete(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    s.face.delete_face(id).await?;
    s.db.conn().execute("UPDATE members SET face_enrolled = 0 WHERE id = ?1", [id])?;
    Ok(StatusCode::NO_CONTENT)
}

/// Live camera (MJPEG) passed through as a stream.
pub async fn preview(State(s): State<AppState>) -> ApiResult<Response> {
    let res = s.face.stream.get(s.face.url("/preview")).bearer_auth(&s.face.token).send().await.map_err(|_| unreachable())?;
    if !res.status().is_success() {
        return Err(unreachable());
    }
    let ct = res.headers().get(header::CONTENT_TYPE).cloned();
    let body = Body::from_stream(res.bytes_stream().map_err(std::io::Error::other));
    let mut out = (StatusCode::OK, [(header::CACHE_CONTROL, "no-cache")], body).into_response();
    if let Some(ct) = ct {
        out.headers_mut().insert(header::CONTENT_TYPE, ct);
    }
    Ok(out)
}

// ---------- reconcile ----------

pub fn spawn_reconcile(s: AppState) {
    tokio::spawn(async move {
        loop {
            let wait = match reconcile(&s).await {
                Ok(()) => RECONCILE_EVERY,
                Err(e) => {
                    tracing::warn!("face reconcile skipped: {}", e.message());
                    RECONCILE_RETRY
                }
            };
            tokio::time::sleep(wait).await;
        }
    });
}

pub async fn reconcile(s: &AppState) -> ApiResult<()> {
    let in_face = s.face.gallery_ids().await?;
    let (active, enrolled): (Vec<i64>, Vec<i64>) = {
        let c = s.db.conn();
        let ids = |sql: &str| -> rusqlite::Result<Vec<i64>> { c.prepare(sql)?.query_map([], |r| r.get(0))?.collect() };
        (ids("SELECT id FROM members WHERE archived = 0")?, ids("SELECT id FROM members WHERE face_enrolled = 1")?)
    };
    // faces nobody should match anymore
    for id in in_face.iter().filter(|id| !active.contains(id)) {
        s.face.delete_face(*id).await?;
        tracing::info!("reconcile: removed face of member {id} (deleted or archived)");
    }
    // flags that lie
    let c = s.db.conn();
    for id in enrolled.iter().filter(|id| !in_face.contains(id)) {
        c.execute("UPDATE members SET face_enrolled = 0 WHERE id = ?1", [id])?;
        tracing::warn!("reconcile: member {id} has no face in face-service, marked not enrolled");
    }
    for id in in_face.iter().filter(|id| active.contains(id)) {
        c.execute("UPDATE members SET face_enrolled = 1 WHERE id = ?1 AND face_enrolled = 0", [id])?;
    }
    Ok(())
}
