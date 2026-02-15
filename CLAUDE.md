# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## プロジェクト概要

Salesforce 開発用の `package.xml` をインタラクティブに生成する Rust 製 CLI ツール。sf CLI (v2) と連携し、TUI でメタデータ型・コンポーネントを選択して package.xml を生成する。

詳細は `docs/specification.md`（仕様書）と `docs/plan-phase1.md`（実装計画・進捗状況）を参照。

## ビルド・テスト・Lint

```bash
cargo build
cargo test
cargo test <test_name>          # 単一テスト実行
cargo clippy                    # lint
```

各変更後に `cargo build && cargo test && cargo clippy` が通ることを確認する。

## 開発ルール

- 依存クレートの追加は `cargo add` コマンドを使うこと（Cargo.toml の手動編集ではなく）
- 実装計画を求められた場合は、計画を立てた後に改めてユーザーの承認を得てから実装に着手すること
- ユーザー向けメッセージ（エラーメッセージ、プロンプト等）は英語で記述すること

## アーキテクチャ

### I/O チャネル

アプリケーション固有の出力は stdout を使わない。進捗メッセージ・エラー・プロンプトは stderr、TUI は /dev/tty、XML はファイル出力。`--help` / `--version` は clap のデフォルト動作（stdout）に従う。

### 処理フロー (`main.rs: run_generate`)

1. sf CLI 存在確認 → 2. API version 決定 → 3. メタデータ型一覧取得 → 4. TUI で選択 → 5. 出力先決定 → 6. XML 生成・書き込み

### sf CLI 連携 (`sf_client.rs`)

`SfClient` trait で sf CLI との連携を抽象化。`RealSfClient` が実装を持ち、テストではモック実装に差し替え可能。sf CLI の `--json` 出力は ANSI エスケープが混入しうるため、`ansi.rs` で除去してから JSON パースする。

### TUI (`tui/`)

状態・描画・イベントを分離した設計:
- `app.rs`: `AppState`（純粋な状態遷移ロジック、I/O なし）
- `ui.rs`: `draw()`（`AppState` を受け取り描画するだけ、状態変更なし）
- `event.rs`: `handle_key_event()` → 副作用なしの `Action` enum を返す。`mod.rs` のイベントループが `Action` を解釈して副作用を実行
- `fuzzy.rs`: `nucleo-matcher` による fuzzy search ラッパー

ワイルドカード（`*`）と個別コンポーネントの選択は排他的。`wildcard.rs` のハードコードリストでフォルダベース型（wildcard 非対応）を判定する。

### エラーと終了コード (`error.rs`)

`AppError` enum の各バリアントが `exit_code()` で終了コードにマッピングされる。`Cancelled` → 130、それ以外 → 1。clap の引数不正は 2。
