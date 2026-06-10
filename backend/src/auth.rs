//! Forward-auth trust. The app is deployed behind oauth2-proxy (Traefik
//! forward-auth, see ../raspi tasks/traefik.py `_gated_hosts`), which injects
//! `X-Auth-Request-User` / `X-Auth-Request-Email` on authenticated requests.
//!
//! We don't run our own login. The only check the binary makes is that those
//! headers are present — defense-in-depth so a request that somehow reached the
//! loopback port without traversing the proxy is rejected. The values are PII
//! and are **never logged**.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::error::AppError;
use crate::state::AppState;

const HDR_USER: &str = "x-auth-request-user";
const HDR_EMAIL: &str = "x-auth-request-email";

/// The authenticated principal, extracted from the forward-auth headers.
/// Present on every `/api/*` handler purely as a gate — projects are shared,
/// nothing is keyed per-user.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user: String,
    pub email: String,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if state.cfg.dev_auth {
            return Ok(AuthUser {
                user: "dev".into(),
                email: "dev@localhost".into(),
            });
        }
        let user = header(parts, HDR_USER);
        let email = header(parts, HDR_EMAIL);
        match (user, email) {
            (Some(user), Some(email)) if !user.is_empty() => Ok(AuthUser { user, email }),
            // Missing/blank → the request did not come through oauth2-proxy.
            // Never log the (absent) values.
            _ => Err(AppError::Unauthorized),
        }
    }
}

fn header(parts: &Parts, name: &str) -> Option<String> {
    parts
        .headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    fn state(dev_auth: bool) -> AppState {
        let mut cfg = crate::config::Config::from_env().unwrap();
        cfg.dev_auth = dev_auth;
        AppState::new(cfg)
    }

    async fn extract(req: Request<()>, dev_auth: bool) -> Result<AuthUser, AppError> {
        let (mut parts, _) = req.into_parts();
        AuthUser::from_request_parts(&mut parts, &state(dev_auth)).await
    }

    #[tokio::test]
    async fn rejects_when_headers_absent_in_prod() {
        let req = Request::builder().body(()).unwrap();
        assert!(matches!(
            extract(req, false).await,
            Err(AppError::Unauthorized)
        ));
    }

    #[tokio::test]
    async fn accepts_when_headers_present() {
        let req = Request::builder()
            .header(HDR_USER, "alice")
            .header(HDR_EMAIL, "alice@example.com")
            .body(())
            .unwrap();
        let u = extract(req, false).await.unwrap();
        assert_eq!(u.user, "alice");
        assert_eq!(u.email, "alice@example.com");
    }

    #[tokio::test]
    async fn dev_auth_bypasses() {
        let req = Request::builder().body(()).unwrap();
        let u = extract(req, true).await.unwrap();
        assert_eq!(u.user, "dev");
    }
}
