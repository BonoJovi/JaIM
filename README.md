# JaIM - Japanese AI-powered Input Method

<div align="center">

**Japanese Input Method / Japanese a Input Method / Japanese AI Method**

[![Version](https://img.shields.io/badge/Version-1.0.0-blue)](https://github.com/BonoJovi/JaIM/releases)
[![Rust](https://img.shields.io/badge/Rust-2024-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT-green)](LICENSE)

**LLM 駆動の次世代日本語入力 for Linux**

</div>

---

## 特徴

- **ローマ字変換** — ローマ字入力からひらがな・カタカナへの変換
- **辞書変換** — IPADIC ベースの 163,000+ エントリ辞書（Trie 検索 + DP 分節）
- **活用形対応** — 動詞・形容詞の活用形を自動生成（食べた、走って、読んだ 等）
- **記号入力** — 矢印（やじるし→）、括弧ペア（かっこ→「」）等の記号辞書
- **LLM リランキング** — Qwen2.5-0.5B による文脈を考慮した候補順位付け
- **文法スコアリング** — 文法ルールによる候補フィルタリング
- **ユーザ学習** — 選択履歴を学習し、候補順位を最適化
- **文節編集** — 文節の移動・伸縮・候補切替
- **かな変換** — F6〜F10 によるひらがな / カタカナ / ローマ字変換（MS-IME 準拠）
- **IBus / Fcitx5 対応** — IBus は zbus (D-Bus) による Pure Rust 実装、Fcitx5 は FFI 経由の C++ アドオン

## アーキテクチャ

3 段階のハイブリッド変換パイプライン：

```
キー入力 → ローマ字→かな変換 → 辞書分節 (DP)
                                    ↓
                            文法スコアリング
                                    ↓
                            LLM リランキング (Qwen2.5-0.5B)
                                    ↓
                            候補リスト → ユーザ
```

## コアコンポーネント

| コンポーネント | 役割 |
|----------------|------|
| **ローマ字変換器** | ASCII → ひらがな / カタカナのステートマシン |
| **辞書エンジン** | Trie ベースのかな→漢字検索 + DP 分節（163K エントリ） |
| **文法エンジン** | 構造検証とスコアリング（9 ルール） |
| **LLM エンジン** | Qwen2.5-0.5B (Q4 量子化) による文脈リランキング |
| **ユーザスコアラ** | 選択履歴の対数スケール学習 |
| **変換エンジン** | パイプライン統合と文節編集 |

## IME フレームワーク対応

| フレームワーク | 状態 | 実装方式 |
|----------------|------|----------|
| IBus | 対応済み | Pure Rust（`zbus` による D-Bus 直接実装） |
| Fcitx5 | 対応済み | Rust エンジン (cdylib) + C++ アドオン (FFI) |

## プロジェクト構成

```
JaIM/
  src/
    main.rs                  # エントリポイント（IBus D-Bus サービス）
    lib.rs                   # ライブラリクレート（Fcitx5 用 cdylib エクスポート）
    ffi.rs                   # C FFI レイヤ（Fcitx5 向けキー処理・UI 状態取得）
    core/
      romaji/                # ローマ字→かな変換ステートマシン
      dictionary/            # Trie + DP 分節 + 組み込み辞書 (IPADIC)
      grammar/               # 文法検証
      llm/                   # ローカル LLM 推論 (llama.cpp バインディング)
      user_scorer.rs         # ユーザ選択履歴学習
    engine/                  # 変換パイプライン統合
    ibus/                    # IBus D-Bus 統合
      engine_impl.rs         # IBus エンジンインターフェース
      factory.rs             # IBus ファクトリインターフェース
      keymap.rs              # キー定数・ヘルパ
      config.rs              # トグルキー設定
    bin/
      generate_dict.rs       # IPADIC → builtin_dict.rs ジェネレータ
  fcitx5/
    jaim_engine.cpp          # Fcitx5 C++ アドオン
    jaim_engine.h            # アドオンヘッダ
    jaim_ffi.h               # Rust FFI ヘッダ
    CMakeLists.txt           # Fcitx5 ビルド設定
    jaim-addon.conf          # アドオン記述ファイル
    jaim-im.conf             # 入力メソッド記述ファイル
  data/
    jaim.xml                 # IBus コンポーネント記述ファイル
```

## 動作要件

- [Rust](https://rustup.rs/)（最新 stable）
- Linux + IBus または Fcitx5
- IPADIC 辞書（`sudo apt install mecab-ipadic`）
- LLM 用: Qwen2.5-0.5B Q4 モデル（約 512MB）を `~/.local/share/jaim/models/` に配置

## ビルド

```bash
# 辞書生成（IPADIC から組み込み辞書を生成）
cargo run --bin generate-dict

# リリースビルド
cargo build --release
```

## インストール

### IBus

```bash
# バイナリをインストール
sudo cp target/release/jaim /usr/bin/ibus-engine-jaim

# IBus コンポーネント記述ファイルをインストール
sudo cp data/jaim.xml /usr/share/ibus/component/jaim.xml

# IBus を再起動
ibus restart
```

### Fcitx5

```bash
# Fcitx5 アドオンをビルド・インストール
cd fcitx5
mkdir -p build && cd build
cmake .. -DCMAKE_INSTALL_PREFIX=/usr
make
sudo make install

# Fcitx5 を再起動
fcitx5 -r -d
```

## セットアップ

### 1. 言語パックのインストール（Ubuntu 22.04 のみ）

Ubuntu 22.04 では言語パックが自動インストールされないため、手動でのインストールが必要です。
24.04 以降は OS インストール時に自動インストールされるため不要です。

設定 → 地域と言語 → インストールされている言語の管理 → 「はい」で言語パックをインストール

### 2. JaIM を登録

#### IBus の場合

設定 → キーボード → 入力ソース → 「+」 → 日本語 → Japanese (JaIM - Japanese AI Input) → 追加

#### Fcitx5 の場合

Fcitx5 設定 → 入力メソッド → 「+」 → JaIM を検索 → 追加

### 3. 確認

トップバー（ディストロによってはタスクバー / タスクトレイ）の入力メソッドアイコンをクリックし、JaIM が表示されていれば登録完了です。PC の再起動が必要な場合があります。

## キーバインド

| キー | 動作 |
|------|------|
| Ctrl+Shift+Space | IME ON/OFF（設定変更可） |
| Space | 漢字変換 |
| Enter | 確定 |
| Escape | キャンセル |
| ← / → | 文節フォーカス移動 |
| Shift+← / Shift+→ | 文節伸縮 |
| ↑ / ↓ / Space | 候補切替 |
| F6 | ひらがなに変換 |
| F7 | 全角カタカナに変換 |
| F8 | 半角カタカナに変換 |
| F9 | 全角英数に変換 |
| F10 | 半角英数に変換 |

## テスト

```bash
cargo test
```

## ライセンス

[MIT](LICENSE) - Copyright (c) 2026 Yoshihiro NAKAHARA
