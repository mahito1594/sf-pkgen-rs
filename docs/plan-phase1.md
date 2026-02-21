# sf-pkgen 実装計画

## Context

Salesforce 開発用の `package.xml` をインタラクティブに生成する CLI ツール「sf-pkgen」を、仕様書 (`specification.md`) に基づいて Rust で新規実装する。

レビュー負担を軽減するため、機能ごとに 9 ステップに分割し、各ステップが独立してコンパイル・テスト可能な PR 単位となるよう設計する。

### 進捗状況

| Step | 内容 | 状況 |
|------|------|------|
| 1 | プロジェクト初期化と CLI 引数パース | ✅ 完了 |
| 2 | エラー型と ANSI エスケープ除去 | ✅ 完了 |
| 3 | sf CLI クライアント (trait + 実装) | ✅ 完了 |
| 4 | ワイルドカード判定と XML 生成 | ✅ 完了 |
| 5 | 出力先バリデーションと書き込み | ✅ 完了 |
| 6 | TUI 状態管理 | ✅ 完了 |
| 7a | TUI レンダリング | ✅ 完了 |
| 7b | TUI イベントループと sf CLI 連携 | ✅ 完了 |
| 8 | エンドツーエンド統合 | ✅ 完了 |
| 9 | レビュー指摘対応 (TOCTOU, ターミナル復元, dead code) | ✅ 完了 |

**Phase 2** (`docs/plan-phase2.md` 参照)

| Step | 内容 | 状況 |
|------|------|------|
| 10 | Panic hook 完全復元 (RAII) | ✅ 完了 |
| 11 | lib+bin 分離と `run_generate` 公開 | ✅ 完了 |
| 12 | Ctrl+C シグナルハンドリング | ✅ 完了 |
| 13 | 統合テスト（TUI 到達前） | ✅ 完了 |
| 14a | 非対話モード — CLI 引数・バリデーション | ✅ 完了 |
| 14b | 非対話モード — resolve ロジック・統合テスト | ✅ 完了 |
| 15 | Wildcard コメント整合性更新 | ✅ 完了 |

**Phase 2 後の追加改善**

| 内容 | 状況 |
|------|------|
| TUI イベントバッチングによるカーソル遅延軽減 | ✅ 完了 |
| バイナリクレート単一構成へのリファクタリング | ✅ 完了 |
| TUI コンポーネントロードの非同期化 (`std::thread::scope` + `mpsc::channel`) | ✅ 完了 |
| TUI に Vim スタイル hjkl キーバインド追加 | ✅ 完了 |

## モジュール構成

```
sf-pkgen/
  Cargo.toml
  src/
    lib.rs               -- クレートルート、run_generate 定義、モジュール宣言
    main.rs              -- エントリーポイント（CLI パース + RealSfClient 注入のみ）
    cli.rs               -- clap derive 構造体
    error.rs             -- エラー型 (thiserror)
    sf_client.rs         -- sf CLI 連携 (trait + 実装)
    ansi.rs              -- ANSI エスケープ除去
    signal.rs            -- Ctrl+C シグナルハンドリング (ctrlc クレート)
    non_interactive.rs   -- 非対話モードの選択解決ロジック
    wildcard.rs          -- ワイルドカード対応判定
    xml.rs               -- package.xml 生成
    output.rs            -- 出力先バリデーション・書き込み
    tui/
      mod.rs             -- TUI エントリーポイント (ターミナル設定、メインループ、PanicHookGuard)
      app.rs             -- アプリケーション状態管理
      event.rs           -- キーイベント処理
      ui.rs              -- レンダリング (2ペインレイアウト)
      fuzzy.rs           -- nucleo-matcher ラッパー
  tests/
    generate_test.rs     -- run_generate 統合テスト (MockSfClient)
```

### I/O チャネル設計

仕様書に従い、各出力を以下のチャネルに固定する:

| 出力種別 | チャネル |
|---------|---------|
| `--help` / `--version` | stdout (clap のデフォルト動作) |
| TUI (型選択・コンポーネント選択) | `/dev/tty` (ratatui + crossterm が管理) |
| TUI のキー入力 | `/dev/tty` (ratatui + crossterm が管理) |
| 進捗メッセージ (「メタデータ型を取得中...」等) | stderr (`eprintln!`) |
| 出力先プロンプト (表示) | stderr |
| 出力先プロンプト (入力) | stdin |
| 完了メッセージ | stderr |
| 生成された XML | ファイル出力のみ |

**重要**: アプリケーション固有の出力は stdout を使わない。`--help` / `--version` は clap のデフォルト動作 (stdout) に従う。

---

## Step 1: プロジェクト初期化と CLI 引数パース ✅

**状況**: 実装完了 (ブランチ `feat/step-1`)

**目的**: Cargo プロジェクトのセットアップと clap による引数パース

**作成ファイル**:
- `Cargo.toml` — edition 2024, 依存: `clap` v4.5.58 (derive feature)
- `.gitignore` — Rust 標準
- `src/main.rs` — `Cli::parse()` でサブコマンドをマッチし、仮の出力を `eprintln!` で行う
- `src/cli.rs` — clap derive 構造体:
  - `Cli` (Parser) → `Commands` enum → `Generate(GenerateArgs)`
  - `GenerateArgs`: `--target-org` (`-o`), `--api-version` (`-a`), `--output-file` (`-f`)

**テスト**: `Cli::try_parse_from` による引数パースのユニットテスト (7件、全パス)

**規模**: ~80 行

---

## Step 2: エラー型と ANSI エスケープ除去 ✅

**目的**: エラーハンドリング基盤と ANSI 除去ユーティリティ

**作成ファイル**:
- `src/error.rs` — `thiserror` によるエラー enum:
  - `SfCliNotFound` → 終了コード 1
  - `SfCliError { message: String }` → 終了コード 1
  - `JsonParseError { stderr: String }` → 終了コード 1
  - `ApiVersionError { message: String }` → 終了コード 1
  - `NoMetadataTypes` → 終了コード 1
  - `NoComponentsSelected` → 終了コード 1
  - `OutputPathError { message: String }` → 終了コード 1
  - `IoError(std::io::Error)` → 終了コード 1
  - `Cancelled` → 終了コード 130
  - `fn exit_code(&self) -> i32` メソッドで終了コードへマッピング

  **エラーメッセージ (仕様書準拠)**:
  - `SfCliNotFound`: `"sf CLI not found. Visit https://developer.salesforce.com/tools/salesforcecli to install it."`
  - `JsonParseError`: stderr 内容を表示 + `"There may be an issue with sf CLI or its plugins. Run 'sf plugins --core' and verify that @salesforce/plugin-org is included."`
  - `ApiVersionError`: sf CLI のメッセージ + `"Please specify the API version explicitly with the --api-version option."`
  - `NoMetadataTypes`: `"No metadata types were found."`
  - `NoComponentsSelected`: `"No metadata components selected."`
  - `OutputPathError`: パスに応じた仕様書記載のメッセージ

- `src/ansi.rs` — `strip_ansi_escapes(input: &str) -> String`
  - パターン: `\x1b\[[0-9;]*[a-zA-Z]`
  - `std::sync::OnceLock` でコンパイル済み Regex をキャッシュ

**追加依存**: `thiserror`, `regex`

**テスト**: ANSI コード混入文字列・プレーン文字列・空文字列のユニットテスト、終了コードマッピングのテスト、エラーメッセージの内容テスト

**規模**: ~150 行

---

## Step 3: sf CLI クライアント (trait + 実装) ✅

**目的**: sf CLI コマンドの実行・JSON パース・エラーハンドリング

**作成ファイル**:
- `src/sf_client.rs`:
  - JSON レスポンス構造体 (`SfResponse<T>`: status, result, message, name, stack)
  - ドメイン型: `MetadataType` (xml_name, in_folder), `MetadataComponent` (full_name), `OrgInfo` (api_version)
  - `SfClient` trait:
    - `check_sf_exists() -> Result<(), AppError>`
    - `get_org_info(target_org) -> Result<OrgInfo, AppError>`
    - `list_metadata_types(target_org, api_version) -> Result<Vec<MetadataType>, AppError>`
    - `list_metadata(metadata_type, target_org, api_version) -> Result<Vec<MetadataComponent>, AppError>`
      - **注意**: このメソッドは `Result` を返すが、TUI 側で非致命エラーとして扱う (右ペインにエラー表示するのみ)。呼び出し側の責務で `Err` をキャッチし、`ComponentLoadState::Error(msg)` に変換する
  - `RealSfClient` 実装:
    - `run_sf_command()` ヘルパー: コマンド実行 → ANSI 除去 → JSON パース → status 確認
    - 仕様書の「sf コマンド実行結果の処理手順」に準拠
    - エラー時メッセージ: `message` があればその値、なければ `name` + `stack` を使用

**追加依存**: `serde` (derive), `serde_json`

**テスト**: JSON パースロジックのユニットテスト (成功/エラー/不正 JSON のフィクスチャ)。trait によりモックが可能。

**規模**: ~250 行

---

## Step 4: ワイルドカード判定と XML 生成 ✅

**目的**: 純粋ロジックの 2 モジュール (I/O 依存なし)

**作成ファイル**:
- `src/wildcard.rs`:
  - `metadataRegistry.json` の特定バージョンに基づくフォルダベース型 (ワイルドカード非対応型) のハードコードリスト
  - 参照元バージョンをコメントとして記録
  - `pub fn supports_wildcard(xml_name: &str) -> bool` — 非対応リストに含まれない型は `true` (デフォルトでワイルドカード対応として扱う)
  - 代表的な非対応型: `Dashboard`, `Document`, `EmailTemplate`, `Report` 等
- `src/xml.rs`:
  - `PackageXmlInput { types: BTreeMap<String, Vec<String>>, api_version: String }`
  - `pub fn generate_package_xml(input: &PackageXmlInput) -> String`
  - quick-xml による XML 構築、仕様書のフォーマット規約に完全準拠:
    - XML 宣言: `<?xml version="1.0" encoding="UTF-8"?>`
    - `<types>` は `<name>` のアルファベット順 (case-sensitive、BTreeMap で自然保証)
    - `<types>` 内の要素順序: `<members>` → `<name>` (固定)
    - `<members>` はアルファベット順、`*` は常に先頭
    - インデント: スペース 4 つ、改行: LF、末尾改行あり

**追加依存**: `quick-xml`

**テスト**: ワイルドカード判定・XML 生成の網羅的ユニットテスト (ワイルドカード/個別/混合/ソート/XML 宣言/末尾改行)

**規模**: ~200 行

---

## Step 5: 出力先バリデーションと書き込み ✅

**目的**: 出力ファイルパスの検証とファイル書き込み

**作成ファイル**:
- `src/output.rs`:
  - `validate_output_path(path: &Path) -> Result<(), AppError>`:
    - パスがディレクトリでないこと → `"{path} is a directory."`
    - ファイルが既に存在しないこと → `"{path} already exists."`
    - 親ディレクトリが存在すること → `"Directory {parent} does not exist."`
    - **親なし相対パスの扱い**: `path.parent()` が `None` または空の場合、カレントディレクトリ (`.`) を親とみなす (例: `package.xml` → 親は `.`)
  - `write_output(path: &Path, content: &str) -> Result<(), AppError>`
    - 書き込み失敗時: `"{path}: {error details}"`
  - `prompt_output_path() -> Result<PathBuf, AppError>`:
    - stderr にプロンプト表示、stdin から読み取り
    - 空入力はエラー → `"Please enter an output file path."`

**追加依存**: dev-dependencies に `tempfile`

**テスト**: tempdir を使ったバリデーションのユニットテスト (ディレクトリ判定、既存ファイル判定、親なし相対パス、親ディレクトリ不在)

**規模**: ~120 行

---

## Step 6: TUI 状態管理 (レンダリングなし) ✅

**目的**: TUI のデータモデルと状態遷移ロジック (純粋ロジック)

**作成ファイル**:
- `src/tui/mod.rs` — モジュール宣言
- `src/tui/app.rs`:
  - `FocusPane` enum (`Left`, `Right`)
  - `ComponentLoadState` enum (`NotLoaded`, `Loading`, `Loaded(Vec<String>)`, `Error(String)`)
  - `AppState` 構造体:
    - 左ペイン: metadata_types, filtered_indices, left_cursor, search_query, is_searching
    - 右ペイン: component_cache (HashMap<String, ComponentLoadState>), right_cursor, selections (HashMap<String, HashSet<String>>)
    - 共通: focus, should_quit, cancelled
  - メソッド:
    - `new(metadata_types)` — 初期化
    - `highlighted_type() -> Option<&MetadataType>` — カーソル位置の型
    - `move_cursor_up()`, `move_cursor_down()` — 左右ペインのカーソル移動
    - `switch_focus()` — Tab でフォーカス切替
    - `toggle_selection()` — Space で選択/解除 (ワイルドカード排他制御: `*` 選択時は個別解除、個別選択時は `*` 解除)
    - `start_search()`, `update_search(char)`, `backspace_search()`, `end_search()`
    - `apply_fuzzy_filter()` — fuzzy.rs を呼び出してフィルタ適用
    - `confirm() -> Option<BTreeMap<String, Vec<String>>>` — 選択結果があれば返す
    - `cancel()` — キャンセル
    - `set_components(type_name, Result<Vec<String>, String>)` — コンポーネント取得結果の反映
- `src/tui/fuzzy.rs`:
  - `fuzzy_filter(query: &str, items: &[String]) -> Vec<(usize, u32)>`
  - **注**: 仕様の「nucleo」は nucleo プロジェクトを指す。実装では低レベルの `nucleo-matcher` クレートを直接使用する。メタデータ型数 (~200) では非同期処理の `nucleo` クレート本体は不要なため。

**追加依存**: `nucleo-matcher`

**テスト**: 全状態遷移のユニットテスト (カーソル移動、ラップ動作、フォーカス切替、選択トグル、ワイルドカード排他制御、検索フィルタ、確定/キャンセル)

**規模**: ~350 行

---

## Step 7a: TUI レンダリング ✅

**目的**: ratatui による描画ロジックの実装

**作成ファイル**:
- `src/tui/ui.rs`:
  - `draw(frame: &mut Frame, app: &AppState)`:
    - `Layout::horizontal` で左右 2 分割
    - 左ペイン: `Block` + タイトル「メタデータ型」、`List` ウィジェットで型名表示、カーソルハイライト、検索モード時は入力欄を表示
    - 右ペイン: `Block` + タイトル (ハイライト中の型名)、状態に応じた表示:
      - `NotLoaded`: 空
      - `Loading`: 「取得中...」
      - `Error(msg)`: エラーメッセージ
      - `Loaded`: `List` + チェックボックス (`[x]`/`[ ]`)。ワイルドカード対応型は `*` を先頭に表示
    - 下部バー: キー操作ヒント (`Tab: ペイン切替  Space: 選択/解除  Enter: 確定  /: 検索  Esc: キャンセル`)

**追加依存**: `ratatui`, `crossterm`

**テスト**: ratatui の `TestBackend` によるレンダリングスナップショットテスト

**規模**: ~200 行

---

## Step 7b: TUI イベントループと sf CLI 連携 ✅

**目的**: crossterm イベントループとコンポーネント取得の実装

**作成ファイル**:
- `src/tui/event.rs`:
  - `Action` enum: `None`, `LoadComponents(String)`, `Confirm(BTreeMap<String, Vec<String>>)`, `Cancel`
  - `handle_key_event(app: &mut AppState, key: KeyEvent) -> Action`
  - フォーカスと検索状態に応じたキーマッピング
- `src/tui/mod.rs` 更新:
  - `run_tui(metadata_types, sf_client, target_org, api_version) -> Result<BTreeMap<String, Vec<String>>, AppError>`
  - ターミナルのセットアップ/リストア:
    - raw mode 有効化、alternate screen 切替
    - **パニックフック**: パニック時にターミナルをリストアしてからパニック情報を表示
  - メインループ: 描画 → イベントポーリング → アクション処理
  - `LoadComponents` アクション時:
    - `component_cache` を確認、`NotLoaded` なら `Loading` に変更して `sf_client.list_metadata()` を呼び出し
    - **`supports_wildcard(xml_name)` を呼び出して、対応型の場合はコンポーネントリスト先頭に `*` を挿入**
    - `Err` の場合は `ComponentLoadState::Error(msg)` に変換 (TUI は継続)

**テスト**: `handle_key_event` のユニットテスト (各フォーカス状態・検索モードでのキーマッピング)

**規模**: ~250 行

---

## Step 8: エンドツーエンド統合 ✅

**目的**: main.rs で全モジュールを結合し、仕様書の処理フローを完成

**修正ファイル**:
- `src/main.rs`:
  ```
  1. sf CLI の存在確認
  2. API version の決定 (--api-version 指定あり → その値、未指定 → sf org display で取得)
  3. メタデータ型一覧の取得
  4. 取得結果が 0 件 → NoMetadataTypes エラーで終了
  5. TUI で選択
  6. 出力先の決定 (--output-file 指定あり → その値、未指定 → プロンプト)
  7. 出力先バリデーション
  8. package.xml の生成
  9. ファイル書き出し
  10. 完了メッセージ (stderr)
  ```
- **Ctrl+C / シグナルハンドリング**:
  - TUI 中: crossterm のイベントで `Esc`/`Ctrl+C` を検知 → `Cancelled` エラー → 終了コード 130
  - sf CLI 子プロセス実行中: `ctrlc` クレートまたは Unix シグナルハンドラで `SIGINT` をキャッチし、終了コード 130 で終了
  - 進捗メッセージは `eprintln!` で stderr に出力 (スピナー等は不使用)

**追加依存**: `ctrlc` (必要に応じて)

**テスト**: モック `SfClient` を使ったオーケストレーションのテスト、実 org での手動 E2E テスト

**規模**: ~120 行

---

## 依存関係サマリ

| Step | 追加 Cargo 依存 |
|------|----------------|
| 1 | `clap` (derive) |
| 2 | `thiserror`, `regex` |
| 3 | `serde` (derive), `serde_json` |
| 4 | `quick-xml` |
| 5 | dev: `tempfile` |
| 6 | `nucleo-matcher` |
| 7a | `ratatui`, `crossterm` |
| 7b | — |
| 8 | — |
| 12 | `ctrlc` |
| 14b | dev: `tempfile` |

## 設計上の重要な判断

1. **trait ベースの sf CLI クライアント**: `SfClient` trait により、TUI やオーケストレーションのテストで実際の Salesforce org 不要
2. **AppState とレンダリングの分離**: 状態遷移ロジック (Step 6) とレンダリング (Step 7a) を完全分離し、各々独立してユニットテスト可能
3. **Action ベースのイベント処理**: `handle_key_event` は副作用なしの `Action` enum を返す。メインループ (Step 7b) で解釈
4. **BTreeMap で選択結果管理**: XML 出力時のアルファベット順ソートを自然に保証
5. **nucleo-matcher の直接利用**: 仕様の「nucleo」は nucleo プロジェクトを指す。メタデータ型数 (~200) では非同期の `nucleo` クレート本体は不要で、低レベルの `nucleo-matcher` を直接利用
6. **コンポーネント取得エラーの非致命的扱い**: `SfClient::list_metadata()` は `Result` を返すが、TUI 側で `Err` をキャッチし右ペインにエラー表示するのみ (プロセス全体は終了しない)
7. **stdout の利用方針**: アプリケーション固有の出力は stdout を使わない。`--help` / `--version` は clap のデフォルト動作 (stdout) に従う

## リスクと対策

- **TUI のブロッキング**: `sf org list metadata` 呼び出し時に TUI がフリーズする。初期バージョンでは「取得中...」表示で許容し、将来的にバックグラウンドスレッド化を検討
- **パニック時のターミナル状態**: パニックフックでターミナルをリストアする (ratatui の一般的パターン)
- **Ctrl+C の多層ハンドリング**: TUI 中は crossterm イベント、子プロセス実行中は OS シグナル、それぞれで終了コード 130 を保証

## 検証方法

各ステップで:
1. `cargo build` が通ること
2. `cargo test` が通ること
3. `cargo clippy` で警告がないこと

最終統合後:
1. 実際の Salesforce org に対して `sf-pkgen generate` を実行
2. メタデータ型の fuzzy search、コンポーネント選択、ワイルドカード排他制御を手動確認
3. 生成された `package.xml` が仕様書のフォーマット規約に準拠していることを確認
4. エラーケース (sf CLI 未インストール、認証切れ、不正パス等) の動作確認
5. Ctrl+C で終了コード 130 が返ることを確認
