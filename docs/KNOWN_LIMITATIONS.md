# Known limitations and blockers

| Item | Type | Impact | Needed to unblock |
|---|---|---|---|
| Test builds only | Status | CI produces ad-hoc-signed arm64 .app/.dmg artifacts; no public release yet | Agents online + Developer ID |
| Character art is static | Status | Portraits are the owner's concept art; no lip-sync or expressions yet | M15 digital-human renderer + licensed 3D characters |
| Windows | Deferred | macOS only until the product is final (owner decision 26 Nov) | — |
| Hardware not yet measured | Status | No performance or memory figures are claimed | Benchmarks on the owner's MacBook Pro M5 |
| Developer ID signing / notarization | Blocker (later) | Only unsigned/ad-hoc builds possible | Apple Developer credentials as repository secrets |
| Final photorealistic character assets | Blocker (later) | Digital-human renderer will be tested with licensed development assets | Commissioned or licensed character assets |
| Memory engine is in-memory only | Status | Isolation rules are enforced and tested; encrypted SQLite persistence comes next (M20) | Implementation |
