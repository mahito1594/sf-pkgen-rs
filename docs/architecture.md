# sf-pkgen アーキテクチャ

## クレート構成

バイナリクレート単一構成。`src/main.rs` がクレートルートで、モジュール宣言・`run_generate` 関数・`main` 関数を含む。ライブラリクレートは存在しない。

各サブモジュールは `pub(crate)` で可視性を限定している。

## モジュール構成

```
src/
  main.rs          -- クレートルート: モジュール宣言、run_generate、main
  cli.rs           -- clap derive 構造体 (Cli, Commands, GenerateArgs)
  error.rs         -- AppError enum (thiserror)、exit_code() マッピング
  sf_client.rs     -- SfClient trait + RealSfClient 実装、JSON パース
  ansi.rs          -- ANSI エスケープシーケンス除去 (regex)
  signal.rs        -- Ctrl+C シグナルハンドリング (ctrlc + AtomicBool)
  wildcard.rs      -- ワイルドカード対応判定 (フォルダベース型リスト)
  xml.rs           -- package.xml 生成 (quick-xml)
  output.rs        -- 出力先バリデーション・ファイル書き込み・プロンプト
  tui.rs           -- TUI モジュール宣言と run_tui の re-export
  tui/
    app.rs         -- AppState: 純粋な状態遷移ロジック (I/O なし)
    ui.rs          -- draw(): AppState を受け取り描画 (状態変更なし)
    event.rs       -- handle_key_event() → 副作用なしの Action enum を返す
    fuzzy.rs       -- nucleo-matcher による fuzzy search ラッパー
    runner.rs      -- ターミナル setup/teardown、PanicHookGuard、イベントループ、
                      バックグラウンドコンポーネントロード
```

## I/O チャネル設計

各出力の送り先は specification.md の「出力先」テーブルを参照。主な使い分け:

- **TUI**: `/dev/tty` に直接レンダリング（ratatui + crossterm）
- **進捗・エラー・プロンプト**: stderr
- **XML**: `--output-file` またはプロンプトで指定されたファイル
- **`--help` / `--version`**: stdout（clap のデフォルト動作）

## 処理フロー

`main.rs: run_generate` は以下の 9 ステップを実行する。各フェーズ境界で `signal::check_interrupted()` を呼び出し、Ctrl+C を検出する。

```
1. sf CLI 存在確認 (check_sf_exists)
   └─ 失敗: SfCliNotFound → 終了コード 1

2. API version 決定
   a. --api-version 指定あり → その値を使用
   b. 未指定 → sf org display で取得

3. メタデータ型一覧取得 (list_metadata_types)
   └─ 0 件: NoMetadataTypes → 終了コード 1

4. TUI で選択 (run_tui)
   └─ 0 件選択: NoComponentsSelected → 終了コード 1
   └─ Esc / Ctrl+C: Cancelled → 終了コード 130

5. 出力先決定
   a. --output-file 指定あり → その値
   b. 未指定 → stderr にプロンプト表示、stdin から読み取り

6. 出力先バリデーション (validate_output_path)

7. package.xml 生成 (generate_package_xml)

8. ファイル書き出し (write_output)
   └─ create_new(true) で TOCTOU 対策

9. 完了メッセージ表示
```

## sf CLI 連携

### SfClient trait

`sf_client.rs` で定義される `SfClient` trait（`Sync` バウンド付き）が sf CLI との連携を抽象化する。

```rust
pub(crate) trait SfClient: Sync {
    fn check_sf_exists(&self) -> Result<(), AppError>;
    fn get_org_info(&self, target_org: Option<&str>) -> Result<OrgInfo, AppError>;
    fn list_metadata_types(&self, target_org: Option<&str>, api_version: &str) -> Result<Vec<MetadataType>, AppError>;
    fn list_metadata(&self, metadata_type: &str, target_org: Option<&str>, api_version: &str) -> Result<Vec<MetadataComponent>, AppError>;
}
```

`Sync` バウンドは `runner.rs` のバックグラウンドスレッドに `&dyn SfClient` を渡すために必要。

### RealSfClient

`RealSfClient` が `SfClient` の実体実装。テストでは `MockSfClient` に差し替える。

### ANSI 除去と JSON パース

sf CLI の `--json` 出力は環境によって ANSI エスケープコードが混入する。`run_sf_command` は以下の手順で処理する:

1. **ANSI 除去**: `ansi.rs` の `strip_ansi_escapes()` で CSI シーケンス（パターン: `\x1b\[[\x20-\x3f]*[\x40-\x7e]`）を除去
2. **JSON パース**: 正規化した stdout を `SfResponse` にデシリアライズ
3. **status 確認**: `status == 0` → `result` を返す、`status != 0` → `SfCliError` を返す

### SIGINT 検出

`run_sf_command` は子プロセス完了後に 2 つのチェックを行う:
- Unix: `ExitStatusExt::signal() == Some(2)` で子プロセスの SIGINT 終了を検出
- `signal::check_interrupted()` で `AtomicBool` フラグを確認

## TUI アーキテクチャ

### 設計原則: 状態・描画・イベントの分離

TUI は 4 つのモジュールに分離されている:

| モジュール | 責務 | I/O |
|-----------|------|-----|
| `app.rs` | 状態遷移ロジック (`AppState`) | なし（純粋ロジック） |
| `ui.rs` | 描画 (`draw()`) | 読み取りのみ（`&AppState`） |
| `event.rs` | キーイベント処理 (`handle_key_event()`) | なし（`Action` enum を返す） |
| `runner.rs` | ターミナル管理、イベントループ、コンポーネントロード | あり |

### Action enum パターン

`handle_key_event()` は副作用なしの `Action` enum を返す。`runner.rs` のイベントループが `Action` を解釈して副作用を実行する。

```rust
enum Action {
    None,
    LoadComponents(String),
    Confirm(BTreeMap<String, Vec<String>>),
    NoComponentsSelected,
    Cancel,
}
```

### バックグラウンドコンポーネントロード

コンポーネント一覧の取得は `std::thread::scope` + `mpsc::channel` でバックグラウンドスレッド化されている。

- sf CLI 呼び出し中もキー入力を受け付ける
- 同時実行は最大 1 スレッド（`loading_active` フラグで制御）
- ローディング中は 50ms ポーリング、アイドル時はブロッキング読み取り
- 高速カーソル移動時の中間位置に対する stale な `Loading` エントリは `cleanup_stale_loading()` で除去

### イベントバッチング

キーイベントの処理は以下の流れで行われる:

1. 最初のイベントをブロッキング読み取り
2. キューに溜まった後続イベントを `Duration::ZERO` ポーリングですべて消費
3. 各イベントから生じた `LoadComponents` アクションの type_name を記録
4. バッチ処理後、最終カーソル位置のコンポーネントのみをロード

### PanicHookGuard (RAII)

`runner.rs` の `PanicHookGuard` がパニック時のターミナル復元を保証する。

- `install()`: グローバルパニックフックを保存し、ターミナル復元付きのカスタムフックを設定
- `Drop`: 元のパニックフックを復元

### ワイルドカード排他制御

`*`（全件取得）と個別コンポーネントの選択は排他的:
- `*` を選択 → 個別選択をすべてクリア
- 個別コンポーネントを選択 → `*` をクリア

`wildcard.rs` のハードコードリスト（`FOLDER_BASED_TYPES`）でフォルダベース型を判定し、該当する型には `*` エントリを表示しない。

## シグナルハンドリング

`signal.rs` が Ctrl+C ハンドリングを管理する。

- `ctrlc` クレートで Ctrl+C を捕捉し、`AtomicBool` (`INTERRUPTED`) フラグを立てる
- `install_handler_once()`: `std::sync::Once` で二重呼び出しを防止
- `check_interrupted()`: フラグが `true` なら `Err(AppError::Cancelled)` を返す
- `run_generate` のフェーズ境界と `run_sf_command` の子プロセス完了後にフラグを確認

### TUI との棲み分け

競合は発生しない:
- TUI 中は crossterm の `enable_raw_mode()` により、Ctrl+C が SIGINT ではなく `KeyEvent` として配信される
- TUI 外では `ctrlc` ハンドラが SIGINT を捕捉する

## エラーハンドリング

### AppError enum

`error.rs` の `AppError` enum が全エラーを表現する。各バリアントが `exit_code()` で終了コードにマッピングされる。

| バリアント | 終了コード | メッセージ |
|-----------|-----------|-----------|
| `SfCliNotFound` | 1 | `sf CLI not found. Visit https://...` |
| `SfCliError` | 1 | sf CLI の `message` フィールド |
| `JsonParseError` | 1 | stderr + プラグイン確認ヒント |
| `ApiVersionError` | 1 | sf CLI のメッセージ + `--api-version` ヒント |
| `NoMetadataTypes` | 1 | `No metadata types were found.` |
| `NoComponentsSelected` | 1 | `No metadata components selected.` |
| `OutputPathError` | 1 | パスに応じたメッセージ |
| `IoError` | 1 | I/O エラーの詳細 |
| `Cancelled` | 130 | (空文字列) |

clap の引数不正は終了コード 2（clap が処理）。

## テスト戦略

### 構成

テストは `src/main.rs` 内の `#[cfg(test)] mod tests` と各サブモジュール内に配置。

### MockSfClient

`main.rs` のテストモジュールに手書きの `MockSfClient` を定義。各フィールドで trait メソッドの振る舞いを制御する。外部 mocking クレートは不使用。

### テスト対象

- **TUI 到達前の失敗経路**: `check_sf_exists` 失敗、`get_org_info` 失敗、`list_metadata_types` 失敗、メタデータ 0 件
- **状態遷移**: カーソル移動、フォーカス切替、選択トグル、ワイルドカード排他制御、fuzzy search
- **レンダリング**: `ratatui::TestBackend` によるスナップショットテスト
- **イベント処理**: 各キーマッピングと `Action` の対応
- **ユーティリティ**: ANSI 除去、fuzzy filter、XML 生成、出力パスバリデーション

## 設計判断

| 判断 | 根拠 |
|------|------|
| trait ベースの sf CLI 抽象化 (`SfClient`) | TUI やオーケストレーションのテストで実際の Salesforce org 不要 |
| `AppState` とレンダリングの分離 | 状態遷移とレンダリングを独立してユニットテスト可能 |
| `Action` enum パターン | `handle_key_event` が副作用なしで、テスト容易性が高い |
| `BTreeMap` で選択結果管理 | XML 出力時のアルファベット順ソートを自然に保証 |
| `nucleo-matcher` の直接利用 | メタデータ型数 (~200) では非同期の `nucleo` クレート本体は不要。低レベル API で十分 |
| コンポーネント取得エラーの非致命的扱い | 右ペインにエラー表示するのみで TUI は継続。他の型の選択を妨げない |
| `std::thread::scope` + `mpsc::channel` | sf CLI 呼び出し中もキー入力を受け付ける。tokio 等の非同期ランタイム導入を避けつつ並行性を実現 |
| `PanicHookGuard` (RAII) | パニック時のターミナル復元を確実に保証し、正常終了時にグローバルフックを元に戻す |
| `create_new(true)` によるファイル作成 | `validate_output_path` と `write_output` 間の TOCTOU (Time-of-Check to Time-of-Use) レースコンディションを防止 |
