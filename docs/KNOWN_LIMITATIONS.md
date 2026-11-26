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
| Memory engine not yet wired into Tauri app | Status | Isolation + encrypted SQLite (`persist.rs`) are tested in `crates/memory-engine`, while `apps/desktop/src-tauri` still holds `MemoryStore::new()` in RAM | Wire `PersistentMemory` + Keychain key into `AppState` (M20) |
