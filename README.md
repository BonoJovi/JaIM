# JaIM - Japanese AI-powered Input Method

<div align="center">

**Japanese Input Method / Japanese a Input Method / Japanese AI Method**

[![Version](https://img.shields.io/badge/Version-0.1.0-blue)](https://github.com/BonoJovi/JaIM/releases)
[![Rust](https://img.shields.io/badge/Rust-2024-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT-green)](LICENSE)

**LLM-powered intelligent Japanese input for Linux —**

**mozc を超える、AI 駆動の次世代日本語入力！**

</div>

---

## Architecture

JaIM uses a hybrid 3-stage conversion pipeline combining traditional dictionary lookup with AI-powered contextual understanding:

```
Keystroke → Romaji-to-Kana → Dictionary Lookup (< 1ms)
                                    ↓
                            Grammar Scoring (< 1ms)
                                    ↓
                            LLM Reranking (20-40ms)
                                    ↓
                            Candidate List → User
```

### Multi-threaded Pipeline

Input processing runs on multiple threads for minimal latency:

- **Main thread** — Key event handling, UI updates
- **Worker 1** — Dictionary lookup, updated on each keystroke
- **Worker 2** — Grammar scoring, filters candidates as they arrive
- **Worker 3** — LLM KV cache warming, pre-computes context during typing

By the time the user presses Space, most computation is already done.

## Core Components

| Component | Role | Latency |
|-----------|------|---------|
| **Romaji Converter** | ASCII → Hiragana/Katakana state machine | < 0.1ms |
| **Dictionary** | Trie-based kana-to-kanji lookup | < 1ms |
| **Grammar Engine** | Structural validation and scoring (from Promps-Ent) | < 1ms |
| **LLM Engine** | Contextual reranking + final validation (Qwen2.5-0.5B Q4) | 20-40ms |

## IME Framework Support

| Framework | Status | Integration |
|-----------|--------|-------------|
| IBus | Primary target | Pure Rust via `zbus` (D-Bus) |
| Fcitx5 | Planned | C++ shim + Rust FFI |

## Project Structure

```
JaIM/
  src/
    main.rs              # Entry point
    core/                # Core conversion components
      romaji/            # Romaji-to-kana state machine
      dictionary/        # Trie-based dictionary lookup
      grammar/           # Grammar validation (Promps-Ent derived)
      llm/               # Local LLM inference (Qwen2.5-0.5B)
    engine/              # Conversion pipeline orchestrator
    ibus/                # IBus D-Bus integration
  docs/                  # Documentation
  scripts/               # Build and release scripts
```

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- Linux with IBus installed
- For LLM: ~512MB RAM for the quantized model

## Build

```bash
cargo build --release
```

## License

[MIT](LICENSE) - Copyright (c) 2026 Yoshihiro NAKAHARA
