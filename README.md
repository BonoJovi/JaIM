# JaIM - Japanese AI-powered Input Method

<div align="center">

**Japanese Input Method / Japanese a Input Method / Japanese AI Method**

[![Version](https://img.shields.io/badge/Version-0.9.1-blue)](https://github.com/BonoJovi/JaIM/releases)
[![Rust](https://img.shields.io/badge/Rust-2024-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT-green)](LICENSE)

**LLM-powered intelligent Japanese input for Linux —**

**mozc を超える、AI 駆動の次世代日本語入力！**

</div>

---

## Features

- **Romaji-to-Kana conversion** — ローマ字入力からひらがな・カタカナへの変換
- **Dictionary-based conversion** — IPADIC ベースの 163,000+ エントリ辞書（Trie 検索）
- **Conjugation support** — 動詞・形容詞の活用形を自動生成（食べた、走って、読んだ 等）
- **Symbol input** — 矢印（やじるし→）、括弧ペア（かっこ→「」）等の記号辞書
- **LLM reranking** — Qwen2.5-0.5B による文脈を考慮した候補順位付け
- **Grammar scoring** — 文法ルールによる候補フィルタリング
- **User learning** — ユーザの選択履歴を学習し、候補順位を最適化
- **Segment editing** — 文節の移動・伸縮・候補切替
- **Kana conversion** — F6（ひらがな）/ F7（カタカナ）/ F8（半角カタカナ）
- **Pure Rust** — IBus 連携も zbus (D-Bus) で実装、C バインディング不要

## Architecture

JaIM uses a hybrid 3-stage conversion pipeline combining traditional dictionary lookup with AI-powered contextual understanding:

```
Keystroke → Romaji-to-Kana → Dictionary Segmentation (DP)
                                      ↓
                              Grammar Scoring
                                      ↓
                              LLM Reranking (Qwen2.5-0.5B)
                                      ↓
                              Candidate List → User
```

## Core Components

| Component | Role |
|-----------|------|
| **Romaji Converter** | ASCII → Hiragana/Katakana state machine |
| **Dictionary** | Trie-based kana-to-kanji lookup + DP segmentation (163K entries) |
| **Grammar Engine** | Structural validation and scoring (9 rules from Promps-Ent) |
| **LLM Engine** | Contextual reranking via Qwen2.5-0.5B (Q4 quantized) |
| **User Scorer** | Selection history learning with logarithmic scaling |
| **Conversion Engine** | Pipeline orchestrator with segment editing |

## IME Framework Support

| Framework | Status | Integration |
|-----------|--------|-------------|
| IBus | Supported | Pure Rust via `zbus` (D-Bus) |
| Fcitx5 | Planned (v1.0.0) | TBD |

## Project Structure

```
JaIM/
  src/
    main.rs                  # Entry point (IBus D-Bus service)
    core/
      romaji/                # Romaji-to-kana state machine
      dictionary/            # Trie + DP segmentation + builtin dict (IPADIC)
      grammar/               # Grammar validation (Promps-Ent derived)
      llm/                   # Local LLM inference (llama.cpp bindings)
      user_scorer.rs         # User selection history learning
    engine/                  # Conversion pipeline orchestrator
    ibus/                    # IBus D-Bus integration
      engine_impl.rs         # IBus engine interface
      factory.rs             # IBus factory interface
      keymap.rs              # Key constants and helpers
      config.rs              # Toggle key configuration
    bin/
      generate_dict.rs       # IPADIC → builtin_dict.rs generator
  data/
    jaim.xml                 # IBus component descriptor
```

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- Linux with IBus installed
- IPADIC dictionary (`sudo apt install mecab-ipadic`)
- For LLM: Qwen2.5-0.5B Q4 model (~512MB) at `~/.local/share/jaim/models/`

## Build

```bash
# Generate dictionary from IPADIC
cargo run --bin generate-dict

# Build release binary
cargo build --release
```

## Install

```bash
# Install binary
sudo cp target/release/jaim /usr/bin/ibus-engine-jaim

# Install IBus component descriptor
sudo cp data/jaim.xml /usr/share/ibus/component/jaim.xml

# Restart IBus
ibus restart
```

## Setup

### 1. Install Language Pack (Ubuntu 22.04 only)

Ubuntu 22.04 では、言語パックが自動インストールされないため、手動でインストールする必要があります。
24.04 以降は OS インストール時に自動インストールされるため、このステップは不要です。

設定 → 地域と言語 → インストールされている言語の管理 → 「はい」で言語パックをインストール

### 2. Register JaIM with IBus

設定 → キーボード → 入力ソース → 「+」 → 日本語 → Japanese (JaIM - Japanese AI Input) → 追加

PC を再起動してください。

### 3. Verify

トップバー（ディストロによってはタスクバー / タスクトレイ）の "A" をクリックして、JaIM が表示されていれば登録完了です。

## Key Bindings

| Key | Action |
|-----|--------|
| Ctrl+Shift+Space | IME ON/OFF (configurable) |
| Space | Convert to kanji |
| Enter | Commit |
| Escape | Cancel |
| Left / Right | Move segment focus |
| Shift+Left / Shift+Right | Resize segment |
| Up / Down / Space | Cycle candidates |
| F6 | Convert to hiragana |
| F7 | Convert to katakana |
| F8 | Convert to half-width katakana |

## Test

```bash
cargo test
```

## License

[MIT](LICENSE) - Copyright (c) 2026 Yoshihiro NAKAHARA
