# Nexonic AI

**Two Minds. One Collective Intelligence. Built Around You.**

Nexonic AI is a personal intelligence environment for Apple Silicon macOS, built by
[Nexonic Technologies](https://nexonictechnologies.com/). It pairs two independent digital
personalities, **Sharee** and **Nexa**, each with its own reasoning context, private memory,
voice and learning history, who collaborate through a shared Collective Intelligence layer.

> **Status: early development.** The repository foundation, security baseline and first
> core crates are in place. There is no installable app yet. See
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
apps/desktop/  Tauri desktop app (planned)
packages/      Shared TypeScript packages: design system, motion, visualization (planned)
services/      Native macOS and local-inference services (planned)
docs/          Architecture, milestones, status and limitations
scripts/       Owner-policy checks and Git hooks
```

## Development

```sh
git clone https://github.com/nexonic-technologies/Nexonic-AI.git
cd Nexonic-AI
git config core.hooksPath scripts/hooks   # owner-only + secret checks before commit/push
cargo test --workspace
```

## Documentation

- [Implementation status](docs/IMPLEMENTATION_STATUS.md)
- [Milestones](docs/MILESTONES.md)
- [Research and architecture](docs/RESEARCH_AND_ARCHITECTURE.md)
- [Design system](docs/DESIGN_SYSTEM.md)
- [Design coverage matrix](docs/DESIGN_COVERAGE_MATRIX.md)
- [Known limitations](docs/KNOWN_LIMITATIONS.md)
- [Security policy](SECURITY.md)

## License

Not yet chosen. Until a license is added, all rights are reserved by Nexonic Technologies.
