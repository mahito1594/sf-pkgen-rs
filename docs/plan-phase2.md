# sf-pkgen Phase 2 実装計画 (plan.2.3)

## Summary

Phase 2 は以下を目的とする。

1. 既知バグ修正（panic hook 復元、Ctrl+C の終了コード 130 保証）
2. テスト容易性の向上（lib+bin 分離と統合テスト）
3. CI/CD 向け UX 改善（非対話モード Phase 1）

実装順は「仕様準拠に直結する修正 → テスト基盤 → 新機能 → 低リスク整合性修正（コメント更新）」とする。
すべてのステップで `cargo build && cargo test && cargo clippy` を通過条件にする。

---

## Scope

### In Scope

1. Panic hook の完全復元（RAII）
2. Ctrl+C ハンドリングの明示化（非 TUI フェーズで 130 を保証）
3. lib+bin リファクタリング（`run_generate` 公開）
4. `run_generate` の統合テスト追加（TUI 到達前の経路）
5. 非対話モード Phase 1（`--non-interactive`, `--all`, `--types`）
6. `wildcard.rs` 参照バージョンコメント更新

### Out of Scope

1. 非同期 TUI コンポーネント取得
2. 非対話モード Phase 2（`--select`）
3. 新サブコマンド追加（`validate`, `diff` など）

---

## Public API / Interface Changes

1. `src/lib.rs` を新規作成し、クレート API を公開する。
2. `run_generate` は以下に統一する。
   - `pub fn run_generate(sf_client: &dyn SfClient, args: &GenerateArgs) -> Result<(), AppError>`
3. `src/main.rs` は CLI パースと `RealSfClient` 注入のみを担当する。
4. `GenerateArgs` に以下を追加する。
   - `non_interactive: bool`
   - `all: bool`
   - `types: Option<Vec<String>>`
5. 非対話モード時は `--output-file` 必須にする（CI ハング防止）。
6. `src/signal.rs` を追加し、割り込み API を提供する。
   - `install_handler_once()`
   - `check_interrupted() -> Result<(), AppError>`
   - `check_and_clear_interrupted() -> bool`（テスト/再実行安全性用）

---

## Branch / Commit Strategy

- **1ブランチ**で作業。Step ごとにコミットし、コミットメッセージで Step 番号を明示する。
- **PR は1つ**にまとめる。レビュー時はコミット単位で差分を確認可能。
- 各コミット時点で `cargo build && cargo test && cargo clippy` がパスすること。

---

## Detailed Plan

## Step 10: Panic hook 完全復元 (RAII)

### Goal
`run_tui` 実行後に panic hook を必ず「元の hook」に戻す。

### Design
1. `PanicHookGuard` を **`src/tui/mod.rs`** 内に定義する。
   - TUI 固有の terminal 復元に直結するため、TUI モジュール外に置く理由がない。
2. `PanicHookGuard` が `original_hook: Option<Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync + 'static>>` を保持。
   - `Option` にするのは `Drop::drop` で `take()` して `set_hook` に `move` するため。
3. `install()` で `panic::take_hook()` を guard に保存し、カスタム hook を設定。
4. `Drop` で guard 内の `original_hook` を `panic::set_hook` で復元。
5. `run_tui` で `_panic_guard` を TUI スコープ全体に束縛。
6. 既存の `let _ = panic::take_hook();`（`src/tui/mod.rs:86`）は削除。

### Files
- `src/tui/mod.rs`: `PanicHookGuard` 追加、`run_tui` 内のフック管理を書き換え

### Acceptance
1. 正常終了後にグローバル hook が差し替わり続けない。
2. panic 時に terminal 復元が行われる。

---

## Step 11: lib+bin 分離と `run_generate` 公開

### Goal
統合テストからオーケストレーションを直接検証できる構成にする。

### Design
1. 新規 `src/lib.rs` を作成し、既存モジュールを宣言。
2. `main.rs` の `run_generate` を `lib.rs` へ移動し `pub` 化。
3. `run_generate` は `GenerateArgs` を受け取る形に統一。
4. `main.rs` は以下のみ。
   - `Cli::parse()`
   - `RealSfClient` 生成
   - `sf_pkgen::run_generate(&sf_client, &args)`
   - エラー表示と終了コード処理

### 公開範囲のルール

統合テストに必要な最小限のみ `pub` にし、それ以外は `pub(crate)` にする。

| 公開レベル | 対象 |
|-----------|------|
| `pub` | `run_generate`, `SfClient`, `AppError`, `GenerateArgs`, `OrgInfo`, `MetadataType`, `MetadataComponent` |
| `pub(crate)` | `tui`, `xml`, `output`, `ansi`, `wildcard` |

具体的には `lib.rs` で以下のように宣言する:

```rust
pub mod cli;
pub mod error;
pub mod sf_client;

mod ansi;
mod non_interactive; // Step 14 で追加
mod output;
mod signal;          // Step 12 で追加
mod tui;
mod wildcard;
mod xml;

// run_generate を lib.rs に定義（pub）
pub fn run_generate(sf_client: &dyn SfClient, args: &GenerateArgs) -> Result<(), AppError> {
    // ... main.rs から移動したロジック
}
```

### `run_generate` シグネチャ変更

**現在** (`src/main.rs:40-44`):
```rust
fn run_generate(
    target_org: Option<&str>,
    api_version: Option<&str>,
    output_file: Option<&std::path::Path>,
) -> Result<(), AppError>
```

**変更後** (`src/lib.rs`):
```rust
pub fn run_generate(
    sf_client: &dyn SfClient,
    args: &GenerateArgs,
) -> Result<(), AppError>
```

`GenerateArgs` のフィールドを直接参照する形に変更。`sf_client` は外部から注入。

### `main.rs` 変更後

```rust
use clap::Parser;
use sf_pkgen::cli::{Cli, Commands};
use sf_pkgen::sf_client::RealSfClient;

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Generate(args) => {
            let sf_client = RealSfClient;
            if let Err(e) = sf_pkgen::run_generate(&sf_client, &args) {
                let msg = e.to_string();
                if !msg.is_empty() {
                    eprintln!("{msg}");
                }
                std::process::exit(e.exit_code());
            }
        }
    }
}
```

### Files
- `src/lib.rs`: 新規作成
- `src/main.rs`: `run_generate` を削除し、`lib.rs` 経由で呼び出す形に簡素化

### Acceptance
1. 挙動回帰なし（既存 138 tests pass を維持）。
2. `tests/` から `sf_pkgen::run_generate` を呼べる。

---

## Step 12: Ctrl+C シグナルハンドリング（仕様 130 保証）

### Goal
非 TUI フェーズ（sf CLI 実行中、プロンプト入力中）で Ctrl+C を `AppError::Cancelled` に正規化する。

### Signal handler 実装: `ctrlc` クレート使用

**判断理由**: クロスプラットフォーム対応、safe コード、メンテ済み。

依存追加: `cargo add ctrlc`

### Design
1. `src/signal.rs`:
   - `AtomicBool INTERRUPTED`（`static`、初期値 `false`）
   - `install_handler_once()`:
     - `std::sync::Once` で二重呼び出しを防止
     - `Once` ブロック内で `ctrlc::set_handler(|| INTERRUPTED.store(true, SeqCst))` を呼ぶ
     - `ctrlc::set_handler` 自体も内部で1回しか登録できないが、`Once` で明示的にガードする
   - `check_interrupted() -> Result<(), AppError>`:
     - `INTERRUPTED.load(SeqCst)` が `true` なら `Err(AppError::Cancelled)` を返す
   - `check_and_clear_interrupted() -> bool`:
     - `INTERRUPTED.swap(false, SeqCst)` を返す
     - テスト間のフラグリセット用
2. `main()` 先頭で `signal::install_handler_once()` を実行。
3. `run_generate` で各フェーズ境界に `signal::check_interrupted()?` を入れる。
   - `check_sf_exists` 後
   - `get_org_info` 後
   - `list_metadata_types` 後
   - `prompt_output_path` 後
4. `run_sf_command` 側にも割り込み判定を追加（`src/sf_client.rs:104-120`）。
   - `Command::output()` 後、JSON parse 前に `signal::check_interrupted()?`
   - 可能なら `ExitStatus` が SIGINT 相当（Unix では `ExitStatusExt::signal() == Some(2)`）なら `Cancelled` を返す
   - これにより parse error が `1` になる取りこぼしを防ぐ

### TUI との競合回避

競合は発生しない。理由:
- crossterm の `enable_raw_mode()` により、TUI 中は Ctrl+C が SIGINT ではなく `KeyEvent` として配信される
- したがって `ctrlc` handler と crossterm は自然に棲み分ける
- TUI 前後（raw mode 外）では `ctrlc` handler が SIGINT を捕捉し、`INTERRUPTED` フラグを立てる
- `run_generate` のフェーズ境界チェックで `Cancelled` に変換される

### `run_sf_command` の変更

`src/sf_client.rs` の `run_sf_command` は現在モジュール内の private 関数。`signal::check_interrupted()` を呼ぶには `signal` モジュールへのアクセスが必要。
- `lib.rs` 構成では `crate::signal::check_interrupted()` で呼び出し可能

```rust
fn run_sf_command(args: &[&str]) -> Result<serde_json::Value, AppError> {
    let output = Command::new("sf")
        .args(args)
        .output()
        .map_err(|e| { ... })?;

    // SIGINT check after child process completes
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if output.status.signal() == Some(2) {
            return Err(AppError::Cancelled);
        }
    }
    crate::signal::check_interrupted()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_sf_response(&stdout, &stderr)
}
```

### Files
- `src/signal.rs`: 新規作成
- `src/lib.rs`: `mod signal;` 追加
- `src/main.rs`: `signal::install_handler_once()` 呼び出し追加
- `src/sf_client.rs`: `run_sf_command` に割り込み判定追加
- `src/lib.rs`（`run_generate`）: フェーズ境界にチェック追加

### Tests (unit)
- `signal::check_interrupted()`: interrupt false → Ok
- `signal::check_interrupted()`: interrupt true → Cancelled
- `signal::check_and_clear_interrupted()`: フラグを戻す

### Acceptance
1. 仕様の「コマンド実行中 Ctrl+C -> 130」を満たす。
2. 既存 TUI の Ctrl+C（crossterm 経由）と競合しない。

---

## Step 13: 統合テスト（TUI 到達前）

### Goal
`run_generate` の主要失敗経路と分岐条件を回帰保護する。

### Test File
`tests/generate_test.rs`

### Mock 設計

手書きの構成可能なモック実装を使用する。外部 mocking crate は不使用。

```rust
struct MockSfClient {
    check_sf_result: Result<(), AppError>,
    org_info_result: Result<OrgInfo, AppError>,
    metadata_types_result: Result<Vec<MetadataType>, AppError>,
    // list_metadata は非対話テスト（Step 14）で使用
    list_metadata_fn: Option<Box<dyn Fn(&str) -> Result<Vec<MetadataComponent>, AppError>>>,
}
```

各テストケースで必要なフィールドだけ設定し、残りはデフォルト（パニック or エラー）にする。

### Cases
1. `check_sf_exists` 失敗 → `SfCliNotFound`
2. `api_version` 未指定で `get_org_info` 失敗 → `ApiVersionError`
3. `api_version` 指定時に `get_org_info` が呼ばれない
   - **検証方法**: `get_org_info` が呼ばれたら `panic!` するモックを使い、パニックしないことで未呼び出しを確認
4. `list_metadata_types` 失敗 → `SfCliError`
5. `metadata_types` 0 件 → `NoMetadataTypes`

### Notes
TUI 本体（`/dev/tty`）は統合テスト対象外。
テストは `run_generate` が TUI に到達する前に `Err` を返すケースのみ。

### Files
- `tests/generate_test.rs`: 新規作成

### Acceptance
1. 5つのテストケースがすべてパス。
2. 既存テスト（138件）に影響なし。

---

## Step 14: 非対話モード Phase 1

### Goal
TUI なしで package.xml を生成できるようにし、CI/CD UX を向上する。

### CLI Rules (確定)

1. `--non-interactive` 指定時:
   - `--all` または `--types` のいずれか必須
   - `--all` と `--types` は同時指定不可
   - `--output-file` 必須（対話プロンプト禁止）
2. `--non-interactive` 未指定時:
   - 現行 TUI フローを維持
3. **`--all` / `--types` を `--non-interactive` なしで指定した場合: エラー**
   - エラーメッセージ: `"--all and --types require --non-interactive."`

### `GenerateArgs` 変更 (`src/cli.rs`)

```rust
#[derive(Debug, clap::Args)]
pub struct GenerateArgs {
    #[arg(short = 'o', long = "target-org")]
    pub target_org: Option<String>,

    #[arg(short = 'a', long = "api-version")]
    pub api_version: Option<String>,

    #[arg(short = 'f', long = "output-file")]
    pub output_file: Option<PathBuf>,

    /// Run in non-interactive mode (requires --all or --types, and --output-file)
    #[arg(long)]
    pub non_interactive: bool,

    /// Select all metadata types (non-interactive mode only)
    #[arg(long)]
    pub all: bool,

    /// Comma-separated list of metadata types (non-interactive mode only)
    #[arg(long, value_delimiter = ',')]
    pub types: Option<Vec<String>>,
}
```

**`--all` と `--types` の排他制御のみ clap で宣言的に処理する**:
```rust
    #[arg(long, conflicts_with = "types")]
    pub all: bool,
```

その他のバリデーション（`--non-interactive` 依存の制約）は `run_generate` 内で手動実装する。

**理由**: 計画のエラーメッセージは独自文言であり、clap 標準のメッセージ（`error: the argument '--all' cannot be used with '--types'`）とは異なるため、制御可能な範囲で手動バリデーションを行う。ただし `--all` vs `--types` の排他は clap で処理しても十分（clap のメッセージで問題ない）。

### バリデーション実装場所

`run_generate` の冒頭（sf CLI 呼び出し前）で実行する。

```rust
pub fn run_generate(sf_client: &dyn SfClient, args: &GenerateArgs) -> Result<(), AppError> {
    // Validation: --all/--types require --non-interactive
    if !args.non_interactive && (args.all || args.types.is_some()) {
        return Err(AppError::ValidationError {
            message: "--all and --types require --non-interactive.".to_string(),
        });
    }

    if args.non_interactive {
        if !args.all && args.types.is_none() {
            return Err(AppError::ValidationError {
                message: "In non-interactive mode, specify --all or --types.".to_string(),
            });
        }
        if args.output_file.is_none() {
            return Err(AppError::ValidationError {
                message: "In non-interactive mode, --output-file is required.".to_string(),
            });
        }
    }

    // ... rest of the flow
}
```

### AppError 追加

**汎用 `ValidationError` バリアントを1つ追加する**。

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    // ... 既存バリアント ...

    #[error("{message}")]
    ValidationError { message: String },
}
```

- exit_code は `1`（既存パターン準拠）。
- **理由**: 5つのエラーメッセージはすべて CLI 引数バリデーション。個別バリアントは過剰であり、メッセージ文字列で十分識別可能。

### Parsing/Normalization Rules for `--types`

1. `value_delimiter=','` で受理（clap が分割）
2. 各要素は `trim()` 後に処理
3. 空要素（例: `ApexClass,,Report`）はエラー
   - メッセージ: `"Metadata type list for --types must not contain empty entries."`
4. 重複型は 1 つに正規化（最初の出現を残す、最終 map は `BTreeMap` で安定化）
5. 型名は case-sensitive（現仕様準拠）

### Resolution Algorithm

1. 対象型集合を決定（`all` or `types`）
2. 未知型が含まれたら即エラー
   - 判定: `metadata_types`（`list_metadata_types` で取得済み）に含まれない型名
   - メッセージ: `"Unknown metadata type: <Type>."`
3. 各型の選択値を生成
   - wildcard 対応型: `["*"]`（`wildcard::supports_wildcard()` で判定）
   - wildcard 非対応型: `sf_client.list_metadata()` で全 `fullName` 取得
   - **取得失敗時: 即座にエラー終了**（CI での予測可能性を優先）
   - 取得 0 件の型は除外
4. 最終選択 map が空なら `NoComponentsSelected`

### New Module
`src/non_interactive.rs`

```rust
use std::collections::BTreeMap;
use crate::error::AppError;
use crate::sf_client::{MetadataType, SfClient};
use crate::wildcard::supports_wildcard;

pub fn resolve(
    sf_client: &dyn SfClient,
    metadata_types: &[MetadataType],
    all: bool,
    types: Option<&[String]>,
    target_org: Option<&str>,
    api_version: &str,
) -> Result<BTreeMap<String, Vec<String>>, AppError> {
    // ... implementation
}
```

### run_generate Branch

```rust
// After metadata_types fetch and sort:
let selections = if args.non_interactive {
    non_interactive::resolve(
        sf_client,
        &metadata_types,
        args.all,
        args.types.as_deref(),
        args.target_org.as_deref(),
        &api_version,
    )?
} else {
    tui::run_tui(metadata_types, sf_client, args.target_org.as_deref(), &api_version)?
};
```

### Error Messages (English, 完全リスト)

| # | メッセージ | AppError バリアント |
|---|----------|-------------------|
| 1 | `In non-interactive mode, specify --all or --types.` | `ValidationError` |
| 2 | `--all and --types cannot be used together.` | clap `conflicts_with` (clap 標準メッセージ) |
| 3 | `In non-interactive mode, --output-file is required.` | `ValidationError` |
| 4 | `--all and --types require --non-interactive.` | `ValidationError` |
| 5 | `Unknown metadata type: <Type>.` | `ValidationError` |
| 6 | `Metadata type list for --types must not contain empty entries.` | `ValidationError` |

### Files
- `src/cli.rs`: `GenerateArgs` にフィールド追加
- `src/error.rs`: `ValidationError` バリアント追加
- `src/non_interactive.rs`: 新規作成
- `src/lib.rs`: `mod non_interactive;` 追加、`run_generate` にバリデーションと分岐追加

### Tests

#### unit tests — `cli.rs`
- `--non-interactive --all --output-file ...` 正常パース
- `--non-interactive --types ApexClass,Report --output-file ...` 正常パース
- `--non-interactive` 単独パース（バリデーションは `run_generate` 側なのでパース自体は成功）
- `--all` + `--types` → clap エラー

#### unit tests — `non_interactive.rs`
- unknown type → `ValidationError`
- empty token in `--types` → `ValidationError`
- dedup（重複型の正規化）
- wildcard 対応型 → `["*"]`
- folder-based 型 → `list_metadata` で fullName 取得
- 全型取得 0 件 → `NoComponentsSelected`
- `list_metadata` 失敗 → 即座にエラー

#### unit tests — `error.rs`
- `ValidationError` の exit_code が `1`
- `ValidationError` の display

### Acceptance
1. `--non-interactive --all --output-file package.xml` で XML 生成（統合テスト）。
2. `--non-interactive --types ApexClass,Report --output-file package.xml` で XML 生成（統合テスト）。
3. 不正な組み合わせでエラー終了。

---

## Step 15: Wildcard コメント整合性更新

### Goal
実装挙動は変えず、参照バージョンコメントのみ更新。

### Change
`src/wildcard.rs` の参照タグを `v6.3.1` → `v12.31.11` に更新。

```rust
// Before:
// Based on: https://github.com/forcedotcom/source-deploy-retrieve/blob/v6.3.1/src/registry/metadataRegistry.json

// After:
// Based on: https://github.com/forcedotcom/source-deploy-retrieve/blob/v12.31.11/src/registry/metadataRegistry.json
```

### 事前確認
実装時に v12.31.11 の `metadataRegistry.json` を確認し、`FOLDER_BASED_TYPES` リストに差分があれば報告する（計画外の変更は行わない）。

### Acceptance
テスト結果・挙動は不変。

---

## Test Plan (Complete)

## Automated

1. 既存全テストパス維持（138件）
2. 追加 unit tests
   - `signal`:
     - interrupt false → Ok
     - interrupt true → Cancelled
     - `check_and_clear` がフラグを戻す
   - `cli`:
     - `--non-interactive --all --output-file ...` 正常
     - `--non-interactive --types ... --output-file ...` 正常
     - `--non-interactive` 単独パース成功
     - `--all` + `--types` clap エラー
   - `error`:
     - `ValidationError` exit_code = 1
     - `ValidationError` display
   - `non_interactive`:
     - unknown type
     - empty token
     - dedup
     - wildcard/folder-based の分岐
     - list_metadata 失敗 → 即座にエラー
     - final empty selection → `NoComponentsSelected`
3. 統合テスト (`tests/generate_test.rs`)
   - Step 13 の 5 ケース（TUI 到達前の失敗経路）
   - 非対話 `--all` 成功（XML 生成）
   - 非対話 `--types ApexClass,Report` 成功
   - 非対話 unknown type 失敗
   - 非対話 `--all`/`--types` without `--non-interactive` → エラー
   - 非対話 `--non-interactive` without `--all`/`--types` → エラー
   - 非対話 `--non-interactive` without `--output-file` → エラー

## Manual

1. `sf-pkgen generate`（既存 TUI 正常）
2. `sf-pkgen generate --non-interactive --all --output-file package.xml`
3. `sf-pkgen generate --non-interactive --types ApexClass,Report --output-file package.xml`
4. sf CLI 呼び出し中に Ctrl+C → 終了コード 130
5. プロンプト待ちで Ctrl+C → 終了コード 130

---

## Documentation Updates

1. `specification.md`
   - 「初期スコープ外」から非対話モードを移動
   - 新 CLI 仕様、組み合わせ制約、エラー文言、`--output-file` 必須条件を追加
2. `plan-phase1.md`
   - Phase 2 の進捗と完了項目を反映

---

## Assumptions and Defaults

1. `--types` は case-sensitive（現仕様維持）
2. 非対話では対話入力を一切行わない（`--output-file` 必須）
3. 非同期 TUI は今回は実施しない（影響範囲が大きいため）
4. `--select` は次フェーズに延期
5. Ctrl+C 判定は「ctrlc クレート + 子プロセス終了状態」で防御的に扱う
6. `--all` モードで `list_metadata` が一部の型で失敗した場合は即座にエラー終了（CI 予測可能性優先）
7. `--all` / `--types` を `--non-interactive` なしで指定した場合はエラー（意図しない動作を防止）
