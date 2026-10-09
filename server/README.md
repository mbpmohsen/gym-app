# gym-server

The gym management app: Rust (axum + SQLite) API with the web UI (`../web`) and the
voices (`../assets/voices`) embedded in one executable. Spec: [`../docs/SPEC.md`](../docs/SPEC.md).

## Build & run

```sh
cd web && npm install && npm run build      # release builds embed web/dist: build the UI first
cd ../server && cargo run --release
```
Open http://127.0.0.1:7470 . First run asks for the admin password.

Face features need face-service running (`../face-service`). In `gym-server.toml` either set
`face_service_token`, or point `face_service_config` at face-service's config to read it from there:
```toml
face_service_url = "http://127.0.0.1:7480"
face_service_config = "../face-service/face-service.toml"
```
Config: `gym-server.toml` (created on first run), data in `data/gym.db`.

## Develop

```sh
cd server && cargo run          # debug build reads web/dist from disk
cd web && npm run dev           # http://localhost:5173, /api and /voices proxied to :7470
cd server && cargo test
```

## Layout

| file | |
|---|---|
| `main.rs` | config, startup, routes |
| `db.rs` | SQLite + migrations (`PRAGMA user_version`); never edit a shipped migration, append one |
| `auth.rs` | single admin password (argon2), in-memory sessions in an HttpOnly cookie |
| `settings.rs` | app settings (voice, entry/exit rules) |
| `domain.rs` | **business rules as pure functions** (subscription validity, status, debt, renewal start) + tests |
| `plans.rs` | plans (tariffs); never deleted, deactivated |
| `members.rs` | members, selling subscriptions (plan copied at sale), payments |
| `face.rs` | bridge to face-service (enroll, preview, health) + gallery reconcile every 10 min |
| `validate.rs` | input normalization: Persian digits, Iranian mobile numbers, dates |
| `assets.rs` | embedded UI (SPA fallback) and voices |
| `error.rs` | JSON errors `{ error, message }`, message in Persian for the UI |
