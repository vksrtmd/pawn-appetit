use axum::{extract::Query, response::IntoResponse, routing::get, Extension, Router};
use log::info;
use oauth2::{
    basic::BasicClient, reqwest::async_http_client, AuthUrl, AuthorizationCode, ClientId,
    CsrfToken, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use std::{
    collections::HashMap,
    net::{SocketAddr, TcpListener},
    sync::Mutex,
};
use tauri::{Emitter, Manager};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;

use crate::{error::Error, AppState};

fn create_client(redirect_url: RedirectUrl) -> BasicClient {
    let client_id = ClientId::new("com.pawnappetit".to_string());
    let auth_url = AuthUrl::new("https://lichess.org/oauth".to_string());
    let token_url = TokenUrl::new("https://lichess.org/api/token".to_string());

    BasicClient::new(client_id, None, auth_url.unwrap(), token_url.ok())
        .set_redirect_uri(redirect_url)
}

/// One login attempt: the CSRF state and PKCE verifier are never reused across attempts.
struct Attempt {
    csrf_token: CsrfToken,
    pkce_challenge: PkceCodeChallenge,
    pkce_verifier: PkceCodeVerifier,
    client: BasicClient,
}

impl Attempt {
    fn new(addr: SocketAddr) -> Self {
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let redirect_url = RedirectUrl::new(format!("http://{addr}/callback"))
            .expect("loopback redirect url is valid");
        Attempt {
            csrf_token: CsrfToken::new_random(),
            pkce_challenge,
            pkce_verifier,
            client: create_client(redirect_url),
        }
    }
}

struct Pending {
    attempt: Attempt,
    /// Stops the callback server; dropping it (a newer attempt replaced this one) stops it too.
    shutdown: oneshot::Sender<()>,
}

#[derive(Default)]
pub struct AuthState {
    pending: Mutex<Option<Pending>>,
}

/// Returns the authorization code, or the message to show the user.
fn parse_callback(query: &HashMap<String, String>, expected_csrf: &str) -> Result<String, String> {
    if let Some(error) = query.get("error") {
        return Err(match error.as_str() {
            "access_denied" => "Login was cancelled".to_string(),
            other => format!("Login failed: {other}"),
        });
    }
    if query.get("state").map(String::as_str) != Some(expected_csrf) {
        return Err("Login failed: state mismatch".to_string());
    }
    query
        .get("code")
        .cloned()
        .ok_or_else(|| "Login failed: no authorization code received".to_string())
}

fn emit_error(app: &tauri::AppHandle, message: String) {
    log::warn!("OAuth error: {message}");
    if let Err(e) = app.emit("oauth_error", message) {
        log::error!("Failed to emit oauth_error: {}", e);
    }
}

#[tauri::command]
#[specta::specta]
pub async fn authenticate(
    username: String,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), Error> {
    info!("Authenticating user {}", username);
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let attempt = Attempt::new(listener.local_addr()?);
    let (auth_url, _) = attempt
        .client
        .authorize_url(|| attempt.csrf_token.clone())
        .add_scope(Scope::new("preference:read".to_string()))
        .add_extra_param("username", username)
        .set_pkce_challenge(attempt.pkce_challenge.clone())
        .url();

    let (shutdown, shutdown_rx) = oneshot::channel();
    *state.auth.pending.lock().unwrap() = Some(Pending { attempt, shutdown });

    let server_app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_server(server_app.clone(), listener, shutdown_rx).await {
            emit_error(&server_app, format!("Login failed: {e}"));
        }
    });

    if let Err(e) = app.opener().open_url(auth_url, None::<String>) {
        state.auth.pending.lock().unwrap().take();
        return Err(e.into());
    }
    Ok(())
}

async fn authorize(
    app: Extension<tauri::AppHandle>,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let pending = app.state::<AppState>().auth.pending.lock().unwrap().take();
    let Some(Pending { attempt, shutdown }) = pending else {
        return "No login in progress".to_string();
    };
    // The server serves one callback; this request still completes during graceful shutdown.
    let _ = shutdown.send(());

    let code = match parse_callback(&query, attempt.csrf_token.secret()) {
        Ok(code) => code,
        Err(message) => {
            emit_error(&app, message.clone());
            return message;
        }
    };

    match attempt
        .client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(attempt.pkce_verifier)
        .request_async(async_http_client)
        .await
    {
        Ok(token) => {
            if let Err(e) = app.emit("access_token", token.access_token().secret()) {
                log::error!("Failed to emit access token: {}", e);
            }
            "Login complete. You can close this tab.".to_string()
        }
        Err(e) => {
            log::error!("OAuth token exchange failed: {}", e);
            emit_error(&app, "Login failed: token exchange error".to_string());
            "Login failed. You can close this tab.".to_string()
        }
    }
}

async fn run_server(
    handle: tauri::AppHandle,
    listener: TcpListener,
    shutdown: oneshot::Receiver<()>,
) -> Result<(), axum::Error> {
    let app = Router::new()
        .route("/callback", get(authorize))
        .layer(Extension(handle));

    axum::Server::from_tcp(listener)
        .map_err(axum::Error::new)?
        .serve(app.into_make_service())
        .with_graceful_shutdown(async {
            let _ = shutdown.await;
        })
        .await
        .map_err(axum::Error::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn each_attempt_has_fresh_csrf_and_verifier() {
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let (a, b) = (Attempt::new(addr), Attempt::new(addr));
        assert_ne!(a.csrf_token.secret(), b.csrf_token.secret());
        assert_ne!(a.pkce_verifier.secret(), b.pkce_verifier.secret());
    }

    #[test]
    fn callback_with_matching_state_yields_code() {
        assert_eq!(
            parse_callback(&q(&[("code", "abc"), ("state", "s")]), "s"),
            Ok("abc".to_string())
        );
    }

    #[test]
    fn cancelled_login_is_reported() {
        let err = parse_callback(&q(&[("error", "access_denied")]), "s").unwrap_err();
        assert_eq!(err, "Login was cancelled");
    }

    #[test]
    fn state_mismatch_and_missing_code_are_errors() {
        assert!(parse_callback(&q(&[("code", "abc"), ("state", "x")]), "s").is_err());
        assert!(parse_callback(&q(&[("state", "s")]), "s").is_err());
    }
}
