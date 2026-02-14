# sf-pkgen 仕様書

Salesforce 開発用の `package.xml` をインタラクティブに生成する CLI ツール。

## 概要

- ターミナル上でメタデータ型を fuzzy search・複数選択し、`package.xml` を生成する
- Salesforce CLI (`sf`) の認証情報・コマンドを利用して org のメタデータ情報を取得する
- 生成した XML はファイルに書き出す（`--output-file` 指定 or プロンプトで保存先を尋ねる）

## 注意事項

- ワイルドカード対応のメタデータ型は `*`（全件取得）または個別コンポーネントを選択可能
- ワイルドカード非対応の型（フォルダベースの型等）は個別コンポーネントのみ選択可能
- ワイルドカード対応の判定は `source-deploy-retrieve` の `metadataRegistry.json` に基づくハードコードリストで行う（詳細は「ワイルドカード対応型リスト」セクションを参照）

## 技術スタック

| 項目 | 選定 |
|------|------|
| 言語 | Rust |
| CLI フレームワーク | clap（derive feature） |
| TUI フレームワーク | ratatui + crossterm |
| Fuzzy search | nucleo |
| XML 生成 | quick-xml |
| JSON パース（sf CLI 出力） | serde + serde_json |
| ANSI エスケープ除去 | regex |
| プロセス実行 | std::process::Command |

## コマンド体系

```
sf-pkgen <SUBCOMMAND>

SUBCOMMANDS:
  generate    package.xml を生成する
  help        ヘルプを表示する
```

### `sf-pkgen generate`

```
sf-pkgen generate [OPTIONS]

OPTIONS:
  -o, --target-org <ALIAS|USERNAME>   対象 org（省略時: sf CLI のデフォルト org）
  -a, --api-version <VERSION>         API version（省略時: sf CLI のデフォルト値）
  -f, --output-file <PATH>            出力先ファイルパス（省略時: プロンプトで尋ねる）
```

## ユーザーフロー

```
$ sf-pkgen generate

  メタデータ型を取得中...

  ┌─ メタデータ型 ────────────────┬─ コンポーネント ─────────────┐
  │ > ApexClass                   │ [x] *                        │
  │   ApexTrigger                 │ [ ] AccountController        │
  │   ApexComponent               │ [ ] ContactService           │
  │   ApexPage                    │ [ ] OpportunityHelper        │
  │   LightningComponentBundle    │ [ ] TaskScheduler            │
  │   CustomObject                │ [ ] UserManager              │
  │   Report                      │ ...                          │
  │   ...                         │                              │
  │                               │                              │
  └───────────────────────────────┴──────────────────────────────┘
  Tab: ペイン切替  Space: 選択/解除  Enter: 確定  /: 検索
  Esc/Ctrl+C: キャンセル

  出力先ファイルパス: manifest/package.xml⏎

  manifest/package.xml に出力しました。
```

### 操作説明

- **左ペイン**: メタデータ型一覧。fuzzy search に対応し、カーソル移動でハイライト中の型を変更する
- **右ペイン**: ハイライト中の型のコンポーネント一覧。Space で選択/解除する
  - ワイルドカード対応型: 先頭に `*` エントリが表示される
  - ワイルドカード非対応型: 個別コンポーネントのみ表示される
  - `*` と個別コンポーネントは排他的に選択される（`*` を選択すると個別選択は解除され、個別コンポーネントを選択すると `*` は解除される）
- **Tab**: 左右ペイン間のフォーカス切替
- **Space**: 右ペインでコンポーネントを選択/解除
- **/**: 左ペインで fuzzy search を開始
- **Enter**: 選択を確定し、package.xml を生成
- **Esc / Ctrl+C**: キャンセルして終了

## 内部処理フロー

```
1. sf CLI の存在確認
   └─ 見つからない場合: エラーメッセージを表示して終了（コード 1）

2. API version を決定
   a. --api-version 指定あり → その値を使用
   b. --api-version 未指定 → sf org display [-o <org>] --json を実行し
      result.apiVersion を取得
   ※ 以降の sf コマンド呼び出しと package.xml の <version> には、
     ここで決定した値を一貫して使用する

3. メタデータ型一覧を取得
   $ sf org list metadata-types [-o <org>] --api-version <ver> --json
   └─ 失敗時: エラー詳細を表示して終了（コード 1）

4. ratatui ベースの TUI でメタデータ型とコンポーネントを提示・選択
   - 左ペイン: メタデータ型一覧（fuzzy search 対応、カーソル移動でハイライト）
   - 右ペイン: ハイライト中の型のコンポーネント一覧（選択可能）
     - ワイルドカード対応型: 先頭に `*` エントリを表示
     - ワイルドカード非対応型: 個別コンポーネントのみ表示
     - コンポーネント一覧は `sf org list metadata -m <Type> [-o <org>] --api-version <ver> --json` で取得（キャッシュ）
   - 選択結果の保持: 型 → 選択されたコンポーネントのリスト（`*` または個別の `fullName` リスト）のマッピング
   - `*` と個別コンポーネントは排他: `*` 選択時は個別選択を自動解除し、個別選択時は `*` を自動解除する
   └─ Enter 確定: 1つ以上のコンポーネントが選択されている場合
   └─ Enter 確定で選択コンポーネントが0件の場合: エラーメッセージを表示して終了（コード 1）
   └─ Esc / Ctrl+C: 終了（コード 130）

5. 出力先を決定
   a. --output-file 指定あり → その値を使用
   b. --output-file 未指定 → プロンプトで尋ねる（デフォルトなし）
   以下のバリデーションを行う:
   └─ パスが空: エラー終了（コード 1）
   └─ パスがディレクトリ: エラー終了（コード 1）
   └─ ファイルが既に存在する: エラー終了（コード 1）
   └─ 親ディレクトリが存在しない（相対パスで親指定なしの場合はカレントディレクトリを親とみなす）: エラー終了（コード 1）

6. 選択されたメタデータ型 + API version から package.xml を構築

7. XML をファイルに書き出し
```

## sf CLI 連携仕様

### 前提条件

- `sf` コマンドが PATH に存在すること
- `plugin-org` プラグインがインストールされていること（`sf org list metadata-types` を提供）
- 対象 org への認証が完了していること

### JSON レスポンス共通構造

sf CLI の `--json` 出力は以下の共通構造を持つ:

```jsonc
// 成功時
{
  "status": 0,
  "result": { ... }
}

// エラー時
{
  "status": 1,
  "name": "ErrorName",
  "message": "エラーの説明",
  "stack": "..."
}
```

### sf コマンド実行結果の処理手順

sf CLI の `--json` 出力は環境によって ANSI エスケープコードが混入する、
JSON が不正になる等の既知の問題がある。sf-pkgen はすべての sf コマンド呼び出しで
以下の手順に従い結果を処理する（ただしコンポーネント一覧取得は例外: 同じパース手順（ANSI 除去 → JSON パース → status 確認）を適用するが、エラー時にプロセスを終了せず右ペインにエラーメッセージを表示するのみとする。詳細は該当セクションを参照）:

1. **stdout の正規化**: ANSI エスケープシーケンス（パターン: `\x1b\[[0-9;]*[a-zA-Z]`）を除去する
2. **JSON パース**: 正規化した stdout を JSON としてパースする
   - パース失敗時: stderr の内容をそのまま表示し、プラグイン不足の可能性をヒントとして付記して終了（コード 1）
3. **status 確認**: JSON の `status` フィールドを確認する
   - `status` == 0: `result` を返す
   - `status` != 0: エラーメッセージを stderr に表示して終了（コード 1）
     - `message` が存在すればその値を表示
     - `message` が空または存在しない場合は `name` と `stack` を表示（デバッグ用）

### API version の決定

| 条件 | `sf` に渡す `--api-version` | `<version>` の出力値 |
|------|---------------------------|---------------------|
| `--api-version` 指定あり | ユーザー指定値 | ユーザー指定値 |
| `--api-version` 未指定 | `sf org display` で取得した値 | `sf org display --json` の `result.apiVersion` |

未指定時の取得コマンド:

```bash
sf org display [-o <org>] --json
# result.apiVersion（例: "62.0"）を使用
```

`--api-version` に指定された値はツール側ではバリデーションを行わず、sf CLI にそのまま渡す。
不正な値が指定された場合は sf CLI がエラーを返し、そのエラーメッセージをそのまま表示する。

### メタデータ型一覧取得

```bash
sf org list metadata-types [-o <org>] --api-version <ver> --json
```

レスポンス（`result.metadataObjects`）から以下を利用する:

- `xmlName`: package.xml の `<name>` に使用する型名
- `directoryName`: 表示の補助情報（必要に応じて）
- `inFolder`: フォルダ型かどうか（参考情報）

### コンポーネント一覧取得

TUI でハイライト中のメタデータ型について、個別のメタデータコンポーネント一覧を取得し、右ペインに選択肢として表示する。

```bash
sf org list metadata -m <MetadataType> [-o <org>] --api-version <ver> --json
```

- レスポンスの各要素から `fullName` を取得し、右ペインに選択肢として表示する
- ワイルドカード対応型の場合、取得した一覧の先頭に `*` エントリを追加する
- 取得結果はセッション中キャッシュし、同じ型への再アクセス時は再取得しない
- 取得失敗時は右ペインにエラーメッセージ（sf CLI の `message`）を表示する（TUI 全体のエラーにはしない）
- 取得失敗した型のコンポーネントは選択不可とする（Enter 確定時、選択済みコンポーネントが他の型にあれば正常に進行する）
- 取得中は右ペインに「取得中...」を表示する

### エラー検知

| sf CLI の応答 | 判定方法 | sf-pkgen の対応 |
|--------------|---------|----------------|
| コマンドが存在しない | `Command::new("sf")` の実行失敗（OS エラー） | `sf CLI が見つかりません。...` と表示して終了 |
| JSON パース失敗（Unknown command、ANSI 混入、不正 JSON 等） | 正規化後の stdout が有効な JSON でない | stderr の内容を表示 + `sf CLI のプラグインが不足している可能性があります。...` をヒントとして付記して終了 |
| 認証エラー・その他 | JSON の `status` != 0 | `message`（なければ `name` + `stack`）を表示して終了 |

## 出力仕様

### package.xml フォーマット

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Package xmlns="http://soap.sforce.com/2006/04/metadata">
    <!-- ワイルドカード選択時 -->
    <types>
        <members>*</members>
        <name>ApexClass</name>
    </types>
    <!-- 個別選択時 -->
    <types>
        <members>AccountController</members>
        <members>ContactService</members>
        <name>ApexClass</name>
    </types>
    <!-- 選択したコンポーネントがある型の数だけ繰り返し -->
    <version>{apiVersion}</version>
</Package>
```

フォーマット規約:

- `<types>` は `<name>` のアルファベット順（大文字小文字区別あり）でソート
- `<types>` 内の要素順序: `<members>` → `<name>`（固定）
- `<members>` はアルファベット順（大文字小文字区別あり）でソート（`*` は常に先頭）
- API version は小数点付き文字列（例: `"62.0"`）
- XML 宣言を含む
- インデントはスペース 4 つ
- 改行コードは LF（`\n`）固定
- ファイル末尾に改行あり

### 出力先

| 種別 | 出力先 |
|------|--------|
| TUI（型選択・コンポーネント選択） | /dev/tty（ratatui + crossterm が管理） |
| TUI のキー入力 | /dev/tty（ratatui + crossterm が管理） |
| 進捗メッセージ（「メタデータ型を取得中...」等） | stderr |
| 出力先プロンプト（表示） | stderr |
| 出力先プロンプト（入力） | stdin |
| 生成された XML | --output-file で指定されたファイル、またはプロンプトで指定されたファイル |
| 完了メッセージ | stderr |

進捗メッセージは `eprintln!` による単純なテキスト出力とする（スピナー等は使用しない）。

## 終了コード

| コード | 状況 |
|--------|------|
| 0 | 正常終了（XML 出力完了） |
| 1 | 一般エラー（sf CLI エラー、プラグイン不足、選択ゼロ等） |
| 2 | 引数不正（clap が処理） |
| 130 | Ctrl+C によるキャンセル（TUI 操作中・コマンド実行中とも） |

## エラーハンドリング

| 状況 | 対応 |
|------|------|
| `sf` コマンドが PATH に存在しない | `sf CLI が見つかりません。https://developer.salesforce.com/tools/salesforcecli を参照してインストールしてください。` → 終了コード 1 |
| sf コマンドの stdout が JSON としてパースできない | stderr の内容を表示 + `sf CLI またはプラグインに問題がある可能性があります。'sf plugins --core' を実行し、@salesforce/plugin-org が含まれているか確認してください。` → 終了コード 1 |
| API version の取得失敗（`sf org display` のエラー） | sf CLI のエラーメッセージを表示 + `--api-version オプションで API version を明示的に指定してください。` → 終了コード 1 |
| org 認証切れ・デフォルト org なし | sf CLI の `message` を stderr に表示 → 終了コード 1 |
| メタデータ型の取得失敗 | sf CLI の `message` を stderr に表示 → 終了コード 1 |
| メタデータ型の取得結果が 0 件 | `メタデータ型が取得できませんでした。` → 終了コード 1 |
| ユーザーが1つもコンポーネントを選択せず確定 | `メタデータコンポーネントが選択されていません。` → 終了コード 1 |
| 出力先パスが空（プロンプトで未入力） | `出力先ファイルパスを入力してください。` → 終了コード 1 |
| 出力先パスがディレクトリ | `{path} はディレクトリです。` → 終了コード 1 |
| 出力先ファイルが既に存在する | `{path} は既に存在します。` → 終了コード 1 |
| 出力先の親ディレクトリが存在しない | `{parent} ディレクトリが存在しません。` → 終了コード 1 |
| 出力先ファイルへの書き込み失敗 | stderr に `{path}: {エラー詳細}` を表示 → 終了コード 1 |
| Ctrl+C によるキャンセル | 終了コード 130（TUI 中は ratatui/crossterm が検知、コマンド実行中は OS のシグナルハンドリング） |

## 実装上の注意

- **TUI レンダリング**: ratatui + crossterm は /dev/tty に直接レンダリングするため、stdout/stderr を汚染しない。左ペインでメタデータ型をブラウズし、右ペインでコンポーネントの選択を行う。コンポーネント一覧はハイライト中の型が変わるたびにアプリ側キャッシュを参照して表示する。
- **sf CLI バージョン**: sf CLI v2 を前提とする。旧 `sfdx` コマンドはサポート対象外。

## ワイルドカード対応型リスト

メタデータ型がワイルドカード（`<members>*</members>`）に対応しているかどうかの判定は、[`source-deploy-retrieve`](https://github.com/forcedotcom/source-deploy-retrieve) の [`metadataRegistry.json`](https://github.com/forcedotcom/source-deploy-retrieve/blob/main/src/registry/metadataRegistry.json) を参照元としたハードコードリストで行う。

### 判定基準

`metadataRegistry.json` の各型定義において、`folderType` プロパティが **存在しない** 型をワイルドカード対応とする。`folderType` が設定されている型（フォルダベースの型）は、ワイルドカードでの一括取得が正しく動作しないため、個別コンポーネントのみ選択可能とする。

### 管理方針

- ワイルドカード非対応型のリストをソースコード内にハードコードする
- リストは `metadataRegistry.json` の特定バージョン（タグ）に基づいて作成する
- リストの更新は手動で行い、参照元のバージョンをコメントとして記録する
- ハードコードリストに含まれない未知の型は、デフォルトでワイルドカード対応として扱う

### ワイルドカード非対応型の例

以下はフォルダベースの型の代表例（`metadataRegistry.json` で `folderType` が設定されている型）:

- `Dashboard`（`folderType: "DashboardFolder"`）
- `Document`（`folderType: "DocumentFolder"`）
- `EmailTemplate`（`folderType: "EmailFolder"`）
- `Report`（`folderType: "ReportFolder"`）

## 初期スコープ外（将来検討）

- オフラインモード（プリセット型一覧による org 接続不要の生成）
- 非対話モード（`--types <A,B,C>` や `--all` による引数指定）
- 既存 `package.xml` とのマージ
- `validate` サブコマンド（package.xml の構文検証）
- `diff` サブコマンド（2つの package.xml の差分表示）
- 設定ファイルによるプリセット（よく使う型の組み合わせを保存）
- Windows 対応（`sf.cmd` へのフォールバック等）
