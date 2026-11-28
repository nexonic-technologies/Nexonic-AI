# Known limitations and blockers

| Item | Type | Impact | Needed to unblock |
|---|---|---|---|
| Preview releases only | Status | GitHub releases carry an ad-hoc-signed arm64 DMG; macOS asks the user to approve the first launch (Privacy & Security → Open Anyway) | Developer ID signing and notarization |
| Characters not yet live in the app | Status | Talking clips render on Apple Silicon (probe); the real-time sidecar that streams them into the app is next | M15 sidecar + bundled runtime |
| Live portraits are pre-rendered | Status | Gaze, blinks and moods play from a LivePortrait frame atlas (99 frames per character); in-between head angles cross-fade. Lip sync while speaking needs the real-time sidecar | M15 sidecar |
| Source portraits are low resolution | Status | Cropped from the concept image (~330 px faces); fine for LivePortrait, sharper sources will improve quality | Higher-resolution renders of Sharee and Nexa |
| Windows | Deferred | macOS only until the product is final (owner decision 26 Nov) | — |
| Hardware not yet measured | Status | No performance or memory figures are claimed (the owner's Mac is an Apple M5 with 16 GB; the on-device benchmark still has to run) | `llama-bench` on the owner's MacBook Pro M5 |
| Developer ID signing / notarization | Blocker (later) | Only unsigned/ad-hoc builds possible | Apple Developer credentials as repository secrets |
| Final photorealistic character assets | Blocker (later) | Digital-human renderer will be tested with licensed development assets | Commissioned or licensed character assets |
| MuseTalk-MLX licence file | Check | The pinned `xocialize/musetalk-mlx` port is described as MIT, but the licence file still has to be confirmed before anything ships with it (upstream MuseTalk weights are MIT) | Confirm the licence file at the pinned commit |
| Memory recall is keyword-based | Status | Sharee and Nexa recall memories that share words with the message (English, Urdu, Roman Urdu); a memory phrased differently may not be recalled, and old conversations are not yet summarised | Semantic search (on-device embeddings) and consolidation, M20/M23 |
| Encrypted storage can fail to open | Handled | If the Keychain refuses or the key does not match, the app starts on a temporary in-memory store, says so in the Memory node and panel, and never touches the existing database | Owner checks the Keychain entry `com.nexonictechnologies.ai / memory-db-key` |
| Update check | Opt-in | Checking GitHub for a newer release is off by default; when on, it runs off the main thread and only reports a strictly higher version | — |
| Memory engine ready, agents not yet writing to it | Status | Encrypted SQLite + Keychain key + `Today's Focus` `kv` storage are wired in `AppState`; memories stay empty until agent runtimes (M16) or UI entry create them | M16 agent runtimes |

## Brains (M17/M18)

- The on-device model (Qwen3.5 2B) is included only in builds made by the `Desktop` workflow; development builds show it as not included and can use a cloud brain.
- The 2B model is small: good for conversation, short reasoning and Urdu/English, weaker than large cloud models on hard maths and long code. Cloud brains cover that.
- Apple Silicon Macs start at 8 GB of memory, so the 4 GB case does not arise on macOS; the context window shrinks automatically on smaller Macs.
- No automatic fallback between brains yet: if the chosen brain fails, the error is shown and the customer can switch in Intelligence.
- Default cloud model ids are editable because providers rename models over time.
