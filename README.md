# Nexonic AI

**Two Minds. One Collective Intelligence. Built Around You.**

Nexonic AI is a personal intelligence environment for Apple Silicon macOS, built by
[Nexonic Technologies](https://nexonictechnologies.com/). It pairs two independent digital
personalities, **Sharee** and **Nexa**, each with its own reasoning context, private memory,
voice and learning history, who collaborate through a shared Collective Intelligence layer.

> **Status: early development.** The macOS app shell runs with the dashboard, live system
> detection, permissions and memory isolation. Agents, voice and awareness are not connected yet. See
> [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) for what is real today.

## Planned stack

| Layer | Technology |
|---|---|
| Desktop shell | Tauri 2 |
| System services and orchestration | Rust |
| Interface | React + TypeScript |
| Native macOS integration | Swift (ScreenCaptureKit, AVFoundation, Vision, Accessibility, Keychain) |
| Rendering | Three.js, WebGL 2 / WebGPU |
| Local inference | Apple MLX, llama.cpp |
| Persistence | SQLite (encrypted), vector retrieval, knowledge graph |
| Cloud models | OpenRouter first, then bring-your-own-key providers |

## Repository layout

```text
crates/        Rust libraries (permissions, memory engine, model router, agent runtime, …)
apps/desktop/  Tauri 2 macOS app: React interface + Rust core
packages/      Shared TypeScript packages (design-system)
services/      Native macOS and local-inference services (planned)
docs/          Architecture, milestones, status and limitations
scripts/       Owner-policy checks and Git hooks
```

## Development

```sh
git clone https://github.com/nexonic-technologies/Nexonic-AI.git
cd Nexonic-AI
git config core.hooksPath scripts/hooks   # owner-only + secret checks before commit/push
npm ci
npm run dev          # runs the macOS app with hot reload (needs Xcode command-line tools + Rust)
cargo test --workspace
```

Platform: **macOS on Apple Silicon** first. Windows comes after the macOS product is final.

## Documentation

- [Implementation status](docs/IMPLEMENTATION_STATUS.md)
- [Milestones](docs/MILESTONES.md)
- [Research and architecture](docs/RESEARCH_AND_ARCHITECTURE.md)
- [Design system](docs/DESIGN_SYSTEM.md)
- [Design coverage matrix](docs/DESIGN_COVERAGE_MATRIX.md)
- [Known limitations](docs/KNOWN_LIMITATIONS.md)
- [Security policy](SECURITY.md)

## License

Nexonic AI is licensed under the [GNU Affero General Public License v3.0](LICENSE) (AGPL-3.0-only).
Commercial licences, signed builds and premium character and voice packs are available from
[Nexonic Technologies](https://nexonictechnologies.com/). Character artwork in
`apps/desktop/src/assets/characters/` is © Nexonic Technologies and is not covered by the AGPL.

### Bundled model

Release builds include [Qwen3.5 2B](https://huggingface.co/Qwen/Qwen3.5-2B) (Apache-2.0, © Alibaba Cloud Qwen team)
as a Q4_K_M GGUF file, run with [llama.cpp](https://github.com/ggml-org/llama.cpp) (MIT). The file is not stored in
this repository: the `Desktop` workflow downloads it and verifies its SHA-256 before packaging.
