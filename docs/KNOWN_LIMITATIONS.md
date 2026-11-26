# Known limitations and blockers

| Item | Type | Impact | Needed to unblock |
|---|---|---|---|
| Test builds only | Status | CI produces ad-hoc-signed arm64 .app/.dmg artifacts; no public release yet | Agents online + Developer ID |
| Characters not yet live in the app | Status | Talking clips render on Apple Silicon (probe); the real-time sidecar that streams them into the app is next | M15 sidecar + bundled runtime |
| Source portraits are low resolution | Status | Cropped from the concept image (~330 px faces); fine for LivePortrait, sharper sources will improve quality | Higher-resolution renders of Sharee and Nexa |
| Windows | Deferred | macOS only until the product is final (owner decision 26 Nov) | — |
| Hardware not yet measured | Status | No performance or memory figures are claimed | Benchmarks on the owner's MacBook Pro M5 |
| Developer ID signing / notarization | Blocker (later) | Only unsigned/ad-hoc builds possible | Apple Developer credentials as repository secrets |
| Final photorealistic character assets | Blocker (later) | Digital-human renderer will be tested with licensed development assets | Commissioned or licensed character assets |
| MuseTalk-MLX licence file | Check | The pinned `xocialize/musetalk-mlx` port is described as MIT, but the licence file still has to be confirmed before anything ships with it (upstream MuseTalk weights are MIT) | Confirm the licence file at the pinned commit |
| Encrypted storage can fail to open | Handled | If the Keychain refuses or the key does not match, the app starts on a temporary in-memory store, says so in the Memory node and panel, and never touches the existing database | Owner checks the Keychain entry `com.nexonictechnologies.ai / memory-db-key` |
| Update check | Opt-in | Checking GitHub for a newer release is off by default; when on, it runs off the main thread and only reports a strictly higher version | — |
| Memory engine ready, agents not yet writing to it | Status | Encrypted SQLite + Keychain key + `Today's Focus` `kv` storage are wired in `AppState`; memories stay empty until agent runtimes (M16) or UI entry create them | M16 agent runtimes |
