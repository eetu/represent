use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::AppResult;
use crate::state::AppState;
use crate::store;

pub fn router(state: AppState) -> Router {
    Router::new()
        // Unauthenticated liveness — gatus probes this; keep it auth-free and on
        // a Traefik monitor router that bypasses oauth2-proxy.
        .route("/status", get(status))
        // Everything below requires the forward-auth headers (AuthUser).
        .route("/api/projects", get(api_projects).post(api_create_project))
        .route("/api/projects/{project}", axum::routing::delete(api_delete_project))
        .route("/api/projects/{project}/files", get(api_files))
        .route(
            "/api/projects/{project}/files/{file}",
            put(api_save_file).get(api_read_file).delete(api_delete_file),
        )
        .route(
            "/api/projects/{project}/files/{file}/rename",
            axum::routing::post(api_rename_file),
        )
        .route("/api/projects/{project}/reorder", axum::routing::post(api_reorder))
        .route("/api/projects/{project}/bundle", get(api_bundle))
        // SPA: serve a built asset if the path maps to a real file under
        // static_dir, otherwise return index.html with 200 so the client router
        // owns the route (a hard refresh on a sub-route works). Done as a handler
        // rather than tower-http's ServeDir, whose not_found_service leaks a 404
        // status onto every client route.
        .fallback(get(serve_spa))
        .with_state(state)
}

async fn serve_spa(State(state): State<AppState>, uri: axum::http::Uri) -> axum::response::Response {
    use axum::response::Html;

    let base = &state.cfg.static_dir;
    let rel = uri.path().trim_start_matches('/');

    // Only attempt a file read for a path that stays inside static_dir after
    // normalisation — rejects `..` traversal and absolute escapes.
    if !rel.is_empty() {
        let candidate = base.join(rel);
        if let Ok(canon) = candidate.canonicalize() {
            if let Ok(canon_base) = base.canonicalize() {
                if canon.starts_with(&canon_base) && canon.is_file() {
                    if let Ok(bytes) = tokio::fs::read(&canon).await {
                        let mime = mime_guess::from_path(&canon).first_or_octet_stream();
                        return ([(header::CONTENT_TYPE, mime.as_ref())], bytes).into_response();
                    }
                }
            }
        }
    }

    // SPA shell for "/" and every unmatched client route.
    match tokio::fs::read_to_string(base.join("index.html")).await {
        Ok(html) => Html(html).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

// ---------- public probe ----------

async fn status(State(state): State<AppState>) -> Json<Value> {
    let projects = store::list_projects(&state.cfg.data_dir).await.ok();
    Json(json!({
        "service": "represent",
        "version": env!("CARGO_PKG_VERSION"),
        "data_dir_healthy": projects.is_some(),
        "project_count": projects.map(|p| p.len()),
    }))
}

// ---------- gated api ----------

#[derive(Deserialize)]
struct CreateProject {
    name: String,
}

#[derive(Deserialize)]
struct SaveFile {
    content: String,
}

#[derive(Deserialize)]
struct Reorder {
    files: Vec<String>,
}

#[derive(Deserialize)]
struct Rename {
    to: String,
}

async fn api_projects(_user: AuthUser, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let projects = store::list_projects(&state.cfg.data_dir).await?;
    Ok(Json(json!({ "projects": projects })))
}

async fn api_create_project(
    _user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateProject>,
) -> AppResult<(StatusCode, Json<Value>)> {
    store::create_project(&state.cfg.data_dir, &req.name).await?;
    Ok((StatusCode::CREATED, Json(json!({ "name": req.name }))))
}

async fn api_delete_project(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(project): Path<String>,
) -> AppResult<StatusCode> {
    store::delete_project(&state.cfg.data_dir, &project).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn api_files(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(project): Path<String>,
) -> AppResult<Json<Value>> {
    let files = store::list_files(&state.cfg.data_dir, &project).await?;
    Ok(Json(json!({ "files": files })))
}

async fn api_read_file(
    _user: AuthUser,
    State(state): State<AppState>,
    Path((project, file)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let content = store::read_file(&state.cfg.data_dir, &project, &file).await?;
    Ok(Json(json!({ "name": file, "content": content })))
}

async fn api_save_file(
    _user: AuthUser,
    State(state): State<AppState>,
    Path((project, file)): Path<(String, String)>,
    Json(req): Json<SaveFile>,
) -> AppResult<StatusCode> {
    store::write_file(&state.cfg.data_dir, &project, &file, &req.content).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn api_delete_file(
    _user: AuthUser,
    State(state): State<AppState>,
    Path((project, file)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    store::delete_file(&state.cfg.data_dir, &project, &file).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Rename one file (keeps its demo-order slot; 409 if the target exists).
async fn api_rename_file(
    _user: AuthUser,
    State(state): State<AppState>,
    Path((project, file)): Path<(String, String)>,
    Json(req): Json<Rename>,
) -> AppResult<StatusCode> {
    store::rename_file(&state.cfg.data_dir, &project, &file, &req.to).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Persist a new demo order: the body lists every file in its new sequence.
/// File names are untouched — the order is metadata (a `.order` sidecar).
async fn api_reorder(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(project): Path<String>,
    Json(req): Json<Reorder>,
) -> AppResult<Json<Value>> {
    let files = store::reorder(&state.cfg.data_dir, &project, &req.files).await?;
    Ok(Json(json!({ "files": files })))
}

/// Download the whole project as a zip — the "bundle" the scripts were copied
/// in from, carried back out with the JIT edits applied.
async fn api_bundle(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(project): Path<String>,
) -> AppResult<impl IntoResponse> {
    let bytes = store::bundle(&state.cfg.data_dir, &project).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (header::CONTENT_DISPOSITION, zip_disposition(&project)),
        ],
        bytes,
    ))
}

/// `Content-Disposition` for the bundle. Project names may contain non-ASCII
/// (ö/ä/å…), which header values can't carry raw — RFC 5987: an ASCII
/// fallback in `filename` plus the UTF-8 percent-encoded `filename*`.
fn zip_disposition(project: &str) -> String {
    let ascii: String = project
        .chars()
        .map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '_' })
        .collect();
    let mut encoded = String::new();
    for b in project.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(*b as char)
            }
            _ => encoded.push_str(&format!("%{b:02X}")),
        }
    }
    format!("attachment; filename=\"{ascii}.zip\"; filename*=UTF-8''{encoded}.zip")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_disposition_encodes_non_ascii() {
        let d = zip_disposition("öljydemo 2026");
        assert!(d.contains("filename=\"_ljydemo 2026.zip\""));
        assert!(d.contains("filename*=UTF-8''%C3%B6ljydemo%202026.zip"));
        // Plain ASCII stays readable in both forms.
        let d = zip_disposition("demo");
        assert!(d.contains("filename=\"demo.zip\""));
        assert!(d.ends_with("filename*=UTF-8''demo.zip"));
    }
}
