# Scrimora Link

Scrimora Link is a Tauri v2 desktop companion for Scrimora that runs on Windows and macOS, talks to the local League Client Update (LCU) API over loopback, and exposes a localhost websocket bridge for the Scrimora web app.

## Project Structure

- `src-tauri/src/lib.rs`: Tauri app startup, tray setup, updater wiring, and background tasks.
- `src-tauri/src/bridge.rs`: Local websocket bridge on `127.0.0.1`.
- `src-tauri/src/deep_link.rs`: `scrimora-link://` deep-link session arming.
- `src-tauri/src/lcu.rs`: LCU discovery and request logic.
- `src-tauri/src/app_state.rs`: Origin allowlist and one-shot pairing state.
- `src-tauri/src/messages.rs`: Websocket request and response payloads.
- `src/index.html`: Static UI shown by the desktop shell.
- `.github/workflows/ci.yml`: Verification and unsigned artifact builds for `main`.
- `.github/workflows/release.yml`: Tagged signed releases and updater artifacts.

## Build And Test Commands

- Install JS dependencies: `bun install --frozen-lockfile`
- Format Rust: `cargo fmt`
- Check Rust formatting: `cargo fmt --check`
- Lint Rust: `cargo clippy --all-targets -- -D warnings`
- Run tests: `cargo test --locked`
- Check compilation: `cargo check`
- Build desktop app locally: `cargo build`
- Build Tauri bundles locally: `bunx tauri build --ci --no-sign --config src-tauri/tauri.ci.conf.json`
- Regenerate icons from the SVG source: `bunx tauri icon ./app-icon.svg -o ./src-tauri/icons`

## Code Style

- Follow existing Rust module boundaries; do not collapse bridge, updater, deep-link, and LCU logic into one file.
- Prefer explicit error messages over silent fallbacks when user action is required.
- Keep loopback websocket messages small, typed, and backwards-compatible.
- Avoid adding runtime dependencies unless the standard library or existing crates are clearly insufficient.
- Use ASCII in source files unless there is a strong reason not to.

## Testing Guidance

- Add or update Rust tests for parsing, normalization, origin validation, and other deterministic logic.
- Run `cargo clippy --all-targets -- -D warnings` and `cargo test --locked` before committing.
- For CI-only packaging issues, keep local checks passing first, then adjust workflows.

## Security And Release Notes

- Never commit signing keys, updater private keys, or secrets.
- The updater public key is compiled in through `SCRIMORA_LINK_UPDATER_PUBLIC_KEY`.
- CI artifact builds on `main` are unsigned and intended for internal testing.
- Tagged releases are the production path and expect signing secrets to be present.
- The loopback bridge must only accept explicitly allowed HTTP(S) origins and one-shot deep-link sessions.

## Agent Notes

- When changing startup or plugin initialization, test that the app still launches without panicking.
- When changing Tauri config, keep CI test builds and signed release builds compatible.
- If you change bundle metadata or icons, verify Windows packaging requirements, especially `.ico` availability.
