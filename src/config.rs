use config::Config;
use serde::Deserialize;
use std::error::Error;

pub const DEFAULT_DB_PATH: &str = "./notifications.db";
pub const DEFAULT_HOST: &str = "0.0.0.0";
pub const DEFAULT_PORT: &str = "8000";
pub const DEFAULT_RELAY_URL: &str = "wss://relay.damus.io";
pub const DEFAULT_NOSTR_EVENT_CACHE_MAX_AGE: u64 = 60 * 60; // 1 hour

#[derive(Clone, Deserialize)]
pub struct ApnsConfig {
    // The path to the Apple private key .p8 file
    pub key_path: String,
    // The Apple private key ID
    pub key_id: String,
    // The Apple team ID
    pub team_id: String,
    // The APNS environment to send notifications to (Sandbox or Production)
    pub environment: String,
    // The topic to send notifications to (The Apple app bundle ID)
    pub topic: String,
}

impl ApnsConfig {
    pub fn endpoint(&self) -> a2::client::Endpoint {
        match self.environment.to_ascii_lowercase().as_str() {
            "production" => a2::client::Endpoint::Production,
            _ => a2::client::Endpoint::Sandbox,
        }
    }
}

#[derive(Clone, Deserialize)]
pub struct FcmConfig {
    // Path to google_services.json for FCM
    pub google_services_file_path: String,
    // VAAPI public key for WebPush (via FCM)
    pub vaapi_key: Option<String>,
}

#[derive(Clone, Deserialize)]
pub struct NotePushConfig {
    /// Config for APNs messaging
    pub apns: Option<ApnsConfig>,
    /// Config for FCM messaging
    pub fcm: Option<FcmConfig>,

    /// Database path
    pub db_path: Option<String>,
    /// Host to bind http server
    pub host: Option<String>,
    /// Port to bind http server
    pub port: Option<String>,

    /// Public http URL
    pub api_base_url: String, // The base URL of where the API server is hosted for NIP-98 auth checks
    // The URL of the Nostr relay server to connect to for getting mutelists
    pub relay_url: Option<String>,
    // The max age of the Nostr event cache, in seconds
    pub nostr_event_cache_max_age: Option<u64>,

    // Relays to pull events from
    #[serde(default)]
    pub pull_relays: Vec<String>,
}

impl NotePushConfig {
    pub fn load_env() -> Result<NotePushConfig, Box<dyn Error>> {
        let config = Config::builder()
            .add_source(config::File::with_name("config.yaml"))
            .add_source(config::Environment::default())
            .build()?;

        Ok(config.try_deserialize()?)
    }

    pub fn host(&self) -> String {
        self.host
            .as_ref()
            .map(|c| c.to_owned())
            .unwrap_or(DEFAULT_HOST.to_owned())
    }

    pub fn port(&self) -> String {
        self.host
            .as_ref()
            .map(|c| c.to_owned())
            .unwrap_or(DEFAULT_PORT.to_owned())
    }

    /// host:port to bind
    pub fn relay_address(&self) -> String {
        format!("{}:{}", self.host(), self.port())
    }
}
