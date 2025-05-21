use dotenv::dotenv;
use std::env;

const DEFAULT_DB_PATH: &str = "./apns_notifications.db";
const DEFAULT_HOST: &str = "0.0.0.0";
const DEFAULT_PORT: &str = "8000";
const DEFAULT_RELAY_URL: &str = "wss://relay.damus.io";
const DEFAULT_NOSTR_EVENT_CACHE_MAX_AGE: u64 = 60 * 60; // 1 hour

pub struct NotePushEnv {
    // The path to the Apple private key .p8 file
    pub apns_private_key_path: Option<String>,
    // The Apple private key ID
    pub apns_private_key_id: Option<String>,
    // The Apple team ID
    pub apns_team_id: Option<String>,
    // The APNS environment to send notifications to (Sandbox or Production)
    pub apns_environment: a2::client::Endpoint,
    // The topic to send notifications to (The Apple app bundle ID)
    pub apns_topic: Option<String>,
    // The path to the SQLite database file
    pub db_path: String,
    // The host and port to bind the relay and API to
    pub host: String,
    pub port: String,
    pub api_base_url: String, // The base URL of where the API server is hosted for NIP-98 auth checks
    // The URL of the Nostr relay server to connect to for getting mutelists
    pub relay_url: String,
    // The max age of the Nostr event cache, in seconds
    pub nostr_event_cache_max_age: std::time::Duration,
    // Path to google_services.json for FCM
    pub google_services_file_path: Option<String>,
    // VAAPI key for WebPush (via FCM)
    pub vaapi_key: Option<String>,
}

impl NotePushEnv {
    pub fn load_env() -> Result<NotePushEnv, env::VarError> {
        dotenv().ok();
        let apns_private_key_path = env::var("APNS_AUTH_PRIVATE_KEY_FILE_PATH").ok();
        let apns_private_key_id = env::var("APNS_AUTH_PRIVATE_KEY_ID").ok();
        let apns_team_id = env::var("APPLE_TEAM_ID").ok();
        let db_path = env::var("DB_PATH").unwrap_or(DEFAULT_DB_PATH.to_string());
        let host = env::var("HOST").unwrap_or(DEFAULT_HOST.to_string());
        let port = env::var("PORT").unwrap_or(DEFAULT_PORT.to_string());
        let relay_url = env::var("RELAY_URL").unwrap_or(DEFAULT_RELAY_URL.to_string());
        let apns_environment_string =
            env::var("APNS_ENVIRONMENT").unwrap_or("development".to_string());
        let api_base_url = env::var("API_BASE_URL").unwrap_or(format!("https://{}:{}", host, port));
        let apns_environment = match apns_environment_string.as_str() {
            "development" => a2::client::Endpoint::Sandbox,
            "production" => a2::client::Endpoint::Production,
            _ => a2::client::Endpoint::Sandbox,
        };
        let apns_topic = env::var("APNS_TOPIC").ok();
        let nostr_event_cache_max_age = env::var("NOSTR_EVENT_CACHE_MAX_AGE")
            .unwrap_or(DEFAULT_NOSTR_EVENT_CACHE_MAX_AGE.to_string())
            .parse::<u64>()
            .map(std::time::Duration::from_secs)
            .unwrap_or(std::time::Duration::from_secs(
                DEFAULT_NOSTR_EVENT_CACHE_MAX_AGE,
            ));
        let google_services_file_path = env::var("GOOGLE_SERVICES_FILE_PATH").ok();
        let vaapi_key = env::var("VAAPI_KEY").ok();

        Ok(NotePushEnv {
            apns_private_key_path,
            apns_private_key_id,
            apns_team_id,
            apns_environment,
            apns_topic,
            db_path,
            host,
            port,
            api_base_url,
            relay_url,
            nostr_event_cache_max_age,
            google_services_file_path,
            vaapi_key,
        })
    }

    pub fn relay_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
