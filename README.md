# ClaudeCraft

A Minecraft-style game written from scratch in Rust, for Android phones.
Goal: join Java Edition servers, with an optional Bedrock mode chosen on the server screen.

## Status

- [x] Server list screen with an edition picker (Java version or Bedrock)
- [x] Java server status ping (`net/`, tested)
- [ ] Bedrock server status ping (RakNet)
- [ ] Join a Java server (login + play packets)
- [ ] World rendering
- [ ] Microsoft account login
- [ ] Join a Bedrock server

## Layout

- `net/` – dependency-free networking crate (`cargo test --manifest-path net/Cargo.toml`)
- `src/` – the game (macroquad)
- `.github/workflows/build.yml` – runs the tests and builds a debug APK; download it from the Actions run's artifacts
