# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## プロジェクト概要

Salesforce 開発用の `package.xml` をインタラクティブに生成する Rust 製 CLI ツール。sf CLI (v2) と連携し、TUI でメタデータ型・コンポーネントを選択して package.xml を生成する。

詳細は `docs/specification.md`（仕様書）、`docs/architecture.md`（アーキテクチャ）を参照。

## ビルド・テスト・Lint

```bash
cargo fmt                       # フォーマット
cargo build
cargo test
cargo test <test_name>          # 単一テスト実行
cargo clippy                    # lint
```

各変更後に `cargo fmt && cargo build && cargo test && cargo clippy` が通ることを確認する。

### バージョンアップ

`Cargo.toml` の `version` を変更した場合、NOTICE ファイルの再生成が必要（CI の `notice` ジョブで整合性を検証している）。

```bash
cargo about generate -o NOTICE about.hbs
```

## 開発ルール

- 依存クレートの追加は `cargo add` コマンドを使うこと（Cargo.toml の手動編集ではなく）
- 実装計画を求められた場合は、計画を立てた後に改めてユーザーの承認を得てから実装に着手すること
- ユーザー向けメッセージ（エラーメッセージ、プロンプト等）は英語で記述すること
- コミットメッセージは Conventional Commits 形式で記述すること（CI の `commitlint` で検証される）。例: `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`

## アーキテクチャ

詳細は `docs/architecture.md`（アーキテクチャ）、`docs/specification.md`（仕様）を参照。

### 重要な制約（コード編集時に注意）

- `SfClient` trait は `Sync` バウンドが必要（`runner.rs` のバックグラウンドスレッドで `&dyn SfClient` を渡すため）
- ワイルドカード（`*`）と個別コンポーネントの選択は排他的。`src/wildcard.rs` の `FOLDER_BASED_TYPES` でフォルダベース型を判定
- sf CLI の `--json` 出力は ANSI エスケープが混入しうるため `ansi.rs` で除去してから JSON パース
- `run_sf_command` は子プロセスの SIGINT 終了と `INTERRUPTED` フラグの両方を検出する
- TUI の各コンポーネントは状態（`app.rs`）・描画（`ui.rs`）・イベント（`event.rs`）を厳密に分離
- `PanicHookGuard` (RAII) がパニック時のターミナル復元を保証
- 各フェーズ境界で `signal::check_interrupted()` により Ctrl+C を検出
