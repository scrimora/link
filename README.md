# Scrimora Link

Scrimora Link is the desktop companion for Scrimora. It runs on Windows and
macOS, connects to the local League Client Update API, and exposes a loopback
websocket bridge for the Scrimora web app.

## Download

Download the latest installer from GitHub Releases:

https://github.com/scrimora/link/releases/latest

Install the asset for your platform:

- Windows: use the Windows installer asset.
- macOS Apple Silicon: use the `aarch64` macOS asset.
- macOS Intel: use the `x86_64` macOS asset.

## Updates

Release builds check GitHub Releases for updates. The Tauri updater uses the
`latest.json` release asset and verifies update signatures before installing.

## Telemetry

Release builds can send privacy-conscious operational telemetry to Umami at
`https://analytics.scrimora.app`. Telemetry is enabled by default when the build
includes `SCRIMORA_LINK_UMAMI_WEBSITE_ID`; set
`SCRIMORA_LINK_TELEMETRY_DISABLED=1` to disable it for local or diagnostic
builds.

Telemetry events do not include summoner names, PUUIDs, match IDs, game IDs,
participant names, raw LCU payloads, lockfile data, or websocket origins.

## Development

Install dependencies:

```sh
bun install --frozen-lockfile
```

Run local verification:

```sh
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

Build local unsigned bundles:

```sh
bunx tauri build --ci --no-sign --config src-tauri/tauri.ci.conf.json
```

## Security

The loopback bridge only accepts explicitly allowed HTTP(S) origins and
one-shot deep-link pairing sessions. Do not commit updater private keys,
signing keys, certificates, or GitHub release secrets.
