use crate::nip98_auth;
use crate::notification_manager::{NotificationBackend, UserNotificationSettings};
use crate::relay_connection::RelayConnection;
use http_body_util::Full;
use hyper::body::Buf;
use hyper::body::Bytes;
use hyper::body::Incoming;
use hyper::{Request, Response, StatusCode};
use std::borrow::Cow;

use http_body_util::BodyExt;
use serde_json::from_value;

use crate::notification_manager::NotificationManager;
use hyper::Method;
use log::warn;
use matchit::{Params, Router};
use nostr::prelude::url::form_urlencoded;
use nostr::PublicKey;
use serde_json::{json, Value};
use std::str::FromStr;
use std::sync::Arc;
use thiserror::Error;

pub struct APIHandler {
    notification_manager: Arc<NotificationManager>,
    base_url: String,
}

impl APIHandler {
    pub fn new(notification_manager: Arc<NotificationManager>, base_url: String) -> Self {
        APIHandler {
            notification_manager,
            base_url,
        }
    }

    // MARK: - HTTP handling

    pub async fn handle_http_request(
        &self,
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, hyper::http::Error> {
        // Check if the request is a websocket upgrade request.
        if hyper_tungstenite::is_upgrade_request(&req) {
            return match self.handle_websocket_upgrade(req).await {
                Ok(response) => Ok(response),
                Err(err) => {
                    log::error!("Error handling websocket upgrade request: {}", err);
                    Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(http_body_util::Full::new(Bytes::from(
                            "Internal server error",
                        )))?)
                }
            };
        }

        // If not, handle the request as a normal API request.
        let final_api_response: APIResponse = match self.try_to_handle_http_request(req).await {
            Ok(api_response) => APIResponse {
                status: api_response.status,
                body: api_response.body,
            },
            Err(err) => {
                // Detect if error is a APIError::AuthenticationError and return a 401 status code
                if let Some(api_error) = err.downcast_ref::<APIError>() {
                    match api_error {
                        APIError::AuthenticationError(message) => APIResponse {
                            status: StatusCode::UNAUTHORIZED,
                            body: json!({ "error": "Unauthorized", "message": message }),
                        },
                        _ => APIResponse {
                            status: StatusCode::BAD_REQUEST,
                            body: json!({ "error": api_error.to_string() }),
                        },
                    }
                } else {
                    // Otherwise, return a 500 status code
                    let random_case_uuid = uuid::Uuid::new_v4();
                    log::error!(
                        "Error handling request: {} (Case ID: {})",
                        err,
                        random_case_uuid
                    );
                    APIResponse {
                        status: StatusCode::INTERNAL_SERVER_ERROR,
                        body: json!({ "error": "Internal server error", "message": format!("Case ID: {}", random_case_uuid) }),
                    }
                }
            }
        };

        Response::builder()
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .status(final_api_response.status)
            .body(http_body_util::Full::new(Bytes::from(
                final_api_response.body.to_string(),
            )))
    }

    async fn handle_websocket_upgrade(
        &self,
        mut req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, Box<dyn std::error::Error>> {
        let (response, websocket) = hyper_tungstenite::upgrade(&mut req, None)?;
        log::info!("New websocket connection.");

        let new_notification_manager = self.notification_manager.clone();
        tokio::spawn(async move {
            match RelayConnection::run(websocket, new_notification_manager).await {
                Ok(_) => {}
                Err(e) => {
                    log::error!("Error with websocket connection: {:?}", e);
                }
            }
        });

        Ok(response)
    }

    async fn try_to_handle_http_request(
        &self,
        mut req: Request<Incoming>,
    ) -> Result<APIResponse, Box<dyn std::error::Error>> {
        let parsed_request = self.parse_http_request(&mut req).await?;
        let api_response: APIResponse = self.handle_parsed_http_request(&parsed_request).await?;
        log::info!(
            "[{}] {} (Authorized pubkey: {}): {}",
            req.method(),
            req.uri(),
            parsed_request.authorized_pubkey,
            api_response.status
        );
        Ok(api_response)
    }

    async fn parse_http_request(
        &self,
        req: &mut Request<Incoming>,
    ) -> Result<ParsedRequest, Box<dyn std::error::Error>> {
        // 1. Read the request body
        let body_buffer = req.body_mut().collect().await?.aggregate();
        let body_bytes = body_buffer.chunk();
        let body_bytes = if body_bytes.is_empty() {
            None
        } else {
            Some(body_bytes)
        };

        // 2. NIP-98 authentication
        let authorized_pubkey = match self.authenticate(req, body_bytes).await? {
            Ok(pubkey) => pubkey,
            Err(auth_error) => {
                return Err(Box::new(APIError::AuthenticationError(auth_error)));
            }
        };

        // 3. Parse the request
        Ok(ParsedRequest {
            uri: req.uri().path().to_string(),
            method: req.method().clone(),
            body_bytes: body_bytes.map(|b| b.to_vec()),
            authorized_pubkey,
            query: match req.uri().query() {
                Some(q) => form_urlencoded::parse(q.as_bytes())
                    .map(|(k, v)| (k.into_owned(), v.into_owned()))
                    .collect(),
                None => vec![],
            },
        })
    }

    // MARK: - Router

    async fn handle_parsed_http_request(
        &self,
        parsed_request: &ParsedRequest,
    ) -> Result<APIResponse, Box<dyn std::error::Error>> {
        enum RouteMatch {
            UserInfo,
            UserSetting,
        }
        let mut routes = Router::new();
        routes.insert("/user-info/{pubkey}/{deviceToken}", RouteMatch::UserInfo)?;
        routes.insert(
            "/user-info/{pubkey}/{deviceToken}/preference",
            RouteMatch::UserSetting,
        )?;

        match routes.at(&parsed_request.uri) {
            Ok(m) => match m.value {
                RouteMatch::UserInfo if parsed_request.method == Method::PUT => {
                    return self.handle_user_info(parsed_request, m.params).await;
                }
                RouteMatch::UserInfo if parsed_request.method == Method::DELETE => {
                    return self.handle_user_info_remove(parsed_request, m.params).await;
                }
                RouteMatch::UserSetting if parsed_request.method == Method::GET => {
                    return self.get_user_settings(parsed_request, m.params).await;
                }
                RouteMatch::UserSetting if parsed_request.method == Method::PUT => {
                    return self.set_user_settings(parsed_request, m.params).await;
                }
                _ => {
                    // fallthrough to 404
                }
            },
            Err(e) => {
                warn!("Match failed: {}", e);
            }
        }

        Ok(APIResponse {
            status: StatusCode::NOT_FOUND,
            body: json!({ "error": "Not found" }),
        })
    }

    // MARK: - Authentication

    async fn authenticate(
        &self,
        req: &Request<Incoming>,
        body_bytes: Option<&[u8]>,
    ) -> Result<Result<PublicKey, String>, Box<dyn std::error::Error>> {
        let auth_header = match req.headers().get("Authorization") {
            Some(header) => header,
            None => return Ok(Err("Authorization header not found".to_string())),
        };

        Ok(nip98_auth::nip98_verify_auth_header(
            auth_header.to_str()?.to_string(),
            &format!(
                "{}{}",
                self.base_url,
                req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("")
            ),
            req.method().as_str(),
            body_bytes,
        )
        .await)
    }

    // MARK: - Endpoint handlers

    async fn handle_user_info(
        &self,
        req: &ParsedRequest,
        url_params: Params<'_, '_>,
    ) -> Result<APIResponse, Box<dyn std::error::Error>> {
        let device_token = get_required_param(&url_params, "deviceToken")?;
        let pubkey = get_required_param(&url_params, "pubkey")?;
        let pubkey = check_pubkey(&pubkey, req)?;

        let backend = match NotificationBackend::from_str(
            req.query
                .iter()
                .find(|c| c.0 == "backend")
                .map(|c| &c.1)
                .unwrap_or(&"apns".to_string()),
        ) {
            Ok(token) => token,
            Err(_) => {
                return Ok(APIResponse {
                    status: StatusCode::BAD_REQUEST,
                    body: json!({ "error": "backend is invalid" }),
                })
            }
        };

        // Early return if backend is not supported
        if !self.notification_manager.has_backend(backend) {
            return Ok(APIResponse {
                status: StatusCode::BAD_REQUEST,
                body: json!({ "error": "Backend not supported" }),
            });
        }

        // Proceed with the main logic after passing all checks
        self.notification_manager
            .save_user_device_info_if_not_present(pubkey, &device_token, backend)
            .await?;
        Ok(APIResponse {
            status: StatusCode::OK,
            body: json!({ "message": "User info saved successfully" }),
        })
    }

    async fn handle_user_info_remove(
        &self,
        req: &ParsedRequest,
        url_params: Params<'_, '_>,
    ) -> Result<APIResponse, Box<dyn std::error::Error>> {
        let device_token = get_required_param(&url_params, "deviceToken")?;
        let pubkey = get_required_param(&url_params, "pubkey")?;
        let pubkey = check_pubkey(&pubkey, req)?;

        // Proceed with the main logic after passing all checks
        self.notification_manager
            .remove_user_device_info(&pubkey, &device_token)
            .await?;

        Ok(APIResponse {
            status: StatusCode::OK,
            body: json!({ "message": "User info removed successfully" }),
        })
    }

    async fn set_user_settings(
        &self,
        req: &ParsedRequest,
        url_params: Params<'_, '_>,
    ) -> Result<APIResponse, Box<dyn std::error::Error>> {
        let device_token = get_required_param(&url_params, "deviceToken")?;
        let pubkey = get_required_param(&url_params, "pubkey")?;
        let pubkey = check_pubkey(&pubkey, req)?;

        // Proceed with the main logic after passing all checks
        let body = req.body_json()?;

        let settings: UserNotificationSettings = match from_value(body.clone()) {
            Ok(settings) => settings,
            Err(_) => {
                return Ok(APIResponse {
                    status: StatusCode::BAD_REQUEST,
                    body: json!({ "error": "Invalid settings" }),
                });
            }
        };

        self.notification_manager
            .save_user_notification_settings(&pubkey, device_token.to_string(), settings)
            .await?;

        Ok(APIResponse {
            status: StatusCode::OK,
            body: json!({ "message": "User settings saved successfully" }),
        })
    }

    async fn get_user_settings(
        &self,
        req: &ParsedRequest,
        url_params: Params<'_, '_>,
    ) -> Result<APIResponse, Box<dyn std::error::Error>> {
        let device_token = get_required_param(&url_params, "deviceToken")?;
        let pubkey = get_required_param(&url_params, "pubkey")?;
        let pubkey = check_pubkey(&pubkey, req)?;

        // Proceed with the main logic after passing all checks
        let settings = self
            .notification_manager
            .get_user_notification_settings(&pubkey, device_token.to_string())
            .await?;

        Ok(APIResponse {
            status: StatusCode::OK,
            body: json!(settings),
        })
    }
}

// MARK: - Extensions

impl Clone for APIHandler {
    fn clone(&self) -> Self {
        APIHandler {
            notification_manager: self.notification_manager.clone(),
            base_url: self.base_url.clone(),
        }
    }
}

// MARK: - Helper types

// Define enum error types including authentication error
#[derive(Debug, Error)]
enum APIError {
    #[error("Missing required parameter {0}")]
    MissingParameter(String),
    #[error("Invalid parameter {0}")]
    InvalidParameter(String),
    #[error("Authentication error: {0}")]
    AuthenticationError(String),
}

struct ParsedRequest {
    uri: String,
    method: Method,
    body_bytes: Option<Vec<u8>>,
    authorized_pubkey: nostr::PublicKey,
    query: Vec<(String, String)>,
}

impl ParsedRequest {
    fn body_json(&self) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        if let Some(body_bytes) = &self.body_bytes {
            Ok(serde_json::from_slice(body_bytes)?)
        } else {
            Ok(json!({}))
        }
    }
}

struct APIResponse {
    status: StatusCode,
    body: Value,
}

fn get_required_param(params: &Params<'_, '_>, key: &str) -> Result<String, APIError> {
    match params.get(key) {
        Some(token) => urlencoding::decode(token)
            .map(|s| match s {
                Cow::Borrowed(s) => s.to_owned(),
                Cow::Owned(s) => s,
            })
            .map_err(|_| APIError::InvalidParameter(key.to_string())),
        None => Err(APIError::MissingParameter(key.to_string())),
    }
}

fn parse_pubkey(pubkey: &str) -> Result<PublicKey, APIError> {
    PublicKey::from_hex(pubkey).map_err(|_| APIError::InvalidParameter("pubkey".to_string()))
}

fn check_pubkey(pubkey: &str, req: &ParsedRequest) -> Result<PublicKey, APIError> {
    let pubkey = parse_pubkey(pubkey)?;
    if pubkey != req.authorized_pubkey {
        Err(APIError::AuthenticationError(
            "pubkey doesnt match authorized pubkey".to_string(),
        ))
    } else {
        Ok(pubkey)
    }
}
