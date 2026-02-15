# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## プロジェクト概要

Salesforce 開発用の `package.xml` をインタラクティブに生成する Rust 製 CLI ツール。sf CLI (v2) と連携し、TUI でメタデータ型・コンポーネントを選択して package.xml を生成する。

詳細は `specification.md`（仕様書）と `plan.md`（実装計画・進捗状況）を参照。

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

アプリケーション固有の出力は stdout を使わない。進捗メッセージ・エラー・プロンプトは stderr、TUI は /dev/tty、XML はファイル出力。`--help` / `--version` は clap のデフォルト動作（stdout）に従う。

モジュール構成と設計方針の詳細は `plan.md` を参照。
