Notepush
========

A high performance Nostr relay for sending out push notifications using the Apple Push Notification Service (APNS).

⚠️🔥WIP! Experimental!⚠️🔥

## Installation

1. Get a build or build it yourself using `cargo build --release`
2. On the working directory from which you start this relay, create the `config.yaml` file with the following contents:

```yaml
# (Optional)
# APNs config to send notifications
apns:
  # The path to the Apple private key .p8 file
  key_path: my_key.p8
  key_id: 1234567
  team_id: 1248163264
  environment: development
  topic: com.your_org.your_app

# (Optional)
# FCM config to send notifications
fcm:
  google_services_file_path: "./google-services.json"
  vaapi_key: my_vaapi_pubkey

# (Optional)
# Public URL of this service
api_base_url: http://localhost:8000

# (Optional)
# Relay to pull profile data from (Mute lists)
relay_url: wss://relay.damus.io

# (Optional)
# The max age of the Nostr event cache, in seconds
nostr_event_cache_max_age: 600

# (Optional)
# Relays to pull events from
pull_relays:
  - "wss://relay.damus.io"
  - "wss://relay.primal.net"
  - "wss://nos.lol"
  - "wss://relay.snort.social"
```

6. Run this relay using the built binary or the `cargo run` command. If you want to change the log level, you can set the `RUST_LOG` environment variable to `DEBUG` or `INFO` before running the relay.

Example:
```sh
$ RUST_LOG=DEBUG cargo run
```

## Contributions

For contribution guidelines, please check [this](https://github.com/damus-io/damus/blob/master/docs/CONTRIBUTING.md) document.

## Development setup

1. Install the Rust toolchain
2. Clone this repository
3. Run `cargo build` to build the project
4. Run `cargo test` to run the tests
5. Run `cargo run` to run the project

## Testing utilities

You can use `test/test-inputs` with a websockets test tool such as `websocat` to play around with the relay. If you have Nix installed, you can run:

```sh
$ nix-shell
[nix-shell] $ websocat ws://localhost:8000
<ENTER_FULL_JSON_PAYLOAD_HERE_AND_PRESS_ENTER>
```
