#![forbid(unsafe_code)]
use hyper_util::rt::TokioIo;
use std::sync::Arc;
use tokio::net::TcpListener;
mod notification_manager;
use r2d2_sqlite::SqliteConnectionManager;
mod notepush_env;
mod relay_connection;
use notepush_env::NotePushEnv;
mod api_request_handler;
mod nip98_auth;
mod utils;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // MARK: - Setup basics

    env_logger::init();

    let env = NotePushEnv::load_env().expect("Failed to load environment variables");
    let listener = TcpListener::bind(&env.relay_address())
        .await
        .expect("Failed to bind to address");
    log::info!("Server running at {}", env.relay_address());

    let manager = SqliteConnectionManager::file(env.db_path.clone());
    let pool: r2d2::Pool<SqliteConnectionManager> =
        r2d2::Pool::new(manager).expect("Failed to create SQLite connection pool");
    // Notification manager is a shared resource that will be used by all connections via a mutex and an atomic reference counter.
    // This is shared to avoid data races when reading/writing to the sqlite database, and reduce outgoing relay connections.
    let mut notification_manager = notification_manager::NotificationManager::new(
        pool,
        env.relay_url.clone(),
        env.nostr_event_cache_max_age,
    )
    .await
    .expect("Failed to create notification manager");

    // setup apns if key path is set
    if let Some(apns_key) = &env.apns_private_key_path {
        notification_manager = notification_manager
            .with_apns(
                apns_key.clone(),
                env.apns_private_key_id.unwrap().clone(),
                env.apns_team_id.unwrap().clone(),
                env.apns_environment.clone(),
                env.apns_topic.unwrap().clone(),
            )
            .expect("Failed to set APNs");
        log::info!("APNS configured for notifications manager!");
    }

    // setup fcm if google services path is set
    if let Some(gsp) = &env.google_services_file_path {
        notification_manager = notification_manager
            .with_fcm(gsp)
            .expect("Failed to setup FCM");
        log::info!("FCM configured for notifications manager!");
    }

    let notification_manager = Arc::new(notification_manager);
    let api_handler = Arc::new(api_request_handler::APIHandler::new(
        notification_manager.clone(),
        env.api_base_url.clone(),
    ));

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let api_handler_clone = api_handler.clone();
        let mut http = hyper::server::conn::http1::Builder::new();
        http.keep_alive(true);

        tokio::task::spawn(async move {
            let service =
                hyper::service::service_fn(|req| api_handler_clone.handle_http_request(req));

            let connection = http.serve_connection(io, service).with_upgrades();

            if let Err(err) = connection.await {
                log::error!("Failed to serve connection: {:?}", err);
            }
        });
    }
}
