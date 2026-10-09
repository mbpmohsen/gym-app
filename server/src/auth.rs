//! Single admin, password-only. First run: no password yet => the UI shows a setup screen.
//! Sessions: random token in an HttpOnly cookie, kept in memory (a restart logs out),
//! sliding 12h expiry.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use argon2::{
    password_hash::{phc::PasswordHash, PasswordHasher, PasswordVerifier},
    Argon2,
};
use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use rand::RngCore;
use serde::Deserialize;
use serde_json::json;

use crate::db;
use crate::error::{ApiError, ApiResult};
use crate::AppState;

const COOKIE: &str = "gym_sid";
const IDLE: Duration = Duration::from_secs(12 * 3600);
const MIN_PASSWORD: usize = 4;
const PASSWORD_KEY: &str = "admin_password_hash";

#[derive(Default)]
pub struct Sessions(Mutex<HashMap<String, Instant>>);

impl Sessions {
    fn create(&self) -> String {
        let mut b = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut b);
        let token: String = b.iter().map(|x| format!("{x:02x}")).collect();
        self.0.lock().unwrap().insert(token.clone(), Instant::now());
        token
    }

    /// Valid => refreshes the idle timer.
    fn touch(&self, token: &str) -> bool {
        let mut m = self.0.lock().unwrap();
        m.retain(|_, t| t.elapsed() < IDLE);
        match m.get_mut(token) {
            Some(t) => {
                *t = Instant::now();
                true
            }
            None => false,
        }
    }

    fn remove(&self, token: &str) {
        self.0.lock().unwrap().remove(token);
    }
}

fn token_from(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|kv| kv.trim().strip_prefix(&format!("{COOKIE}=")).map(str::to_string))
}

fn set_cookie(token: &str, max_age: u64) -> (header::HeaderName, String) {
    (header::SET_COOKIE, format!("{COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}"))
}

/// Middleware for every /api route except /api/auth/*.
pub async fn require(State(s): State<AppState>, req: Request, next: Next) -> Response {
    match token_from(req.headers()) {
        Some(t) if s.sessions.touch(&t) => next.run(req).await,
        _ => ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", "ابتدا وارد شوید").into_response(),
    }
}

fn password_set(s: &AppState) -> ApiResult<bool> {
    Ok(db::get_setting(&s.db.conn(), PASSWORD_KEY)?.is_some())
}

pub async fn state(State(s): State<AppState>, headers: HeaderMap) -> ApiResult<Json<serde_json::Value>> {
    let authenticated = token_from(&headers).is_some_and(|t| s.sessions.touch(&t));
    Ok(Json(json!({ "setup_required": !password_set(&s)?, "authenticated": authenticated })))
}

#[derive(Deserialize)]
pub struct PasswordBody {
    password: String,
}

pub async fn setup(State(s): State<AppState>, Json(b): Json<PasswordBody>) -> ApiResult<Response> {
    if password_set(&s)? {
        return Err(ApiError::new(StatusCode::CONFLICT, "already_setup", "رمز قبلاً تعیین شده است"));
    }
    if b.password.chars().count() < MIN_PASSWORD {
        return Err(ApiError::bad_request(format!("رمز باید حداقل {MIN_PASSWORD} کاراکتر باشد")));
    }
    let hash = Argon2::default().hash_password(b.password.as_bytes()).map_err(ApiError::internal)?.to_string();
    db::set_setting(&s.db.conn(), PASSWORD_KEY, &hash)?;
    tracing::info!("admin password set");
    let token = s.sessions.create();
    Ok(([set_cookie(&token, IDLE.as_secs())], Json(json!({ "ok": true }))).into_response())
}

pub async fn login(State(s): State<AppState>, Json(b): Json<PasswordBody>) -> ApiResult<Response> {
    let stored = db::get_setting(&s.db.conn(), PASSWORD_KEY)?;
    let ok = stored
        .as_deref()
        .and_then(|h| PasswordHash::new(h).ok())
        .is_some_and(|h| Argon2::default().verify_password(b.password.as_bytes(), &h).is_ok());
    if !ok {
        // slow down guessing
        tokio::time::sleep(Duration::from_millis(700)).await;
        tracing::warn!("failed login");
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "bad_password", "رمز اشتباه است"));
    }
    let token = s.sessions.create();
    Ok(([set_cookie(&token, IDLE.as_secs())], Json(json!({ "ok": true }))).into_response())
}

pub async fn logout(State(s): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(t) = token_from(&headers) {
        s.sessions.remove(&t);
    }
    ([set_cookie("", 0)], StatusCode::NO_CONTENT).into_response()
}

#[derive(Deserialize)]
pub struct ChangeBody {
    current: String,
    new: String,
}

pub async fn change_password(State(s): State<AppState>, Json(b): Json<ChangeBody>) -> ApiResult<Json<serde_json::Value>> {
    let stored = db::get_setting(&s.db.conn(), PASSWORD_KEY)?.unwrap_or_default();
    let ok = PasswordHash::new(&stored)
        .is_ok_and(|h| Argon2::default().verify_password(b.current.as_bytes(), &h).is_ok());
    if !ok {
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "bad_password", "رمز فعلی اشتباه است"));
    }
    if b.new.chars().count() < MIN_PASSWORD {
        return Err(ApiError::bad_request(format!("رمز باید حداقل {MIN_PASSWORD} کاراکتر باشد")));
    }
    let hash = Argon2::default().hash_password(b.new.as_bytes()).map_err(ApiError::internal)?.to_string();
    db::set_setting(&s.db.conn(), PASSWORD_KEY, &hash)?;
    Ok(Json(json!({ "ok": true })))
}
