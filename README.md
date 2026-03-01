# sf-pkgen

An interactive CLI tool for generating Salesforce `package.xml` files.
Browse metadata types with fuzzy search, select components via a TUI, and generate a well-formatted `package.xml`.

![sf-pkgen demo](docs/demo.gif)

## Runtime Prerequisites

- [Salesforce CLI](https://developer.salesforce.com/tools/salesforcecli) (`sf`) v2

## Installation

### Homebrew (macOS)

```bash
brew install mahito1594/tap/sf-pkgen
```

### Pre-built binaries

Download a pre-built binary from [GitHub Releases](https://github.com/mahito1594/sf-pkgen-rs/releases).

Binaries are available for:
- macOS (Apple Silicon)
- Linux (x86_64, statically linked)
- Windows (x86_64)

### Build from source

```bash
cargo build --release
```

The binary will be at `target/release/sf-pkgen`.

## Usage

```
sf-pkgen generate [OPTIONS]
```

### Options

| Option | Short | Description |
|--------|-------|-------------|
| `--target-org <ALIAS\|USERNAME>` | `-o` | Target org (defaults to sf CLI default org) |
| `--api-version <VERSION>` | `-a` | API version, e.g. `"62.0"` (defaults to sf CLI default) |
| `--output-file <PATH>` | `-f` | Output file path (prompted if omitted) |

### Example

```bash
# Use default org, prompt for output path
sf-pkgen generate

# Specify org and output file
sf-pkgen generate -o my-sandbox -f manifest/package.xml

# Specify API version explicitly
sf-pkgen generate -a 62.0 -f package.xml
```

### TUI Keybindings

#### Normal Mode

| Key | Action |
|-----|--------|
| `j` / `Down` | Move cursor down |
| `k` / `Up` | Move cursor up |
| `h` / `Left` | Focus left pane |
| `l` / `Right` | Focus right pane |
| `Tab` | Toggle pane focus |
| `Space` | Select / deselect component (right pane) |
| `/` | Start fuzzy search (left pane) |
| `Enter` | Confirm selection |
| `Esc` | Cancel |
| `Ctrl+C` | Cancel |

#### Search Mode

| Key | Action |
|-----|--------|
| Type characters | Filter metadata types |
| `Backspace` | Delete last character |
| `Enter` | Confirm search and return to normal mode |
| `Esc` | Cancel search |
| `Ctrl+C` | Cancel TUI |

### Exit Codes

| Code | Description |
|------|-------------|
| 0 | Success (XML written) |
| 1 | General error (sf CLI error, no metadata types, no selection, invalid path, etc.) |
| 2 | Invalid arguments (handled by clap) |
| 130 | Cancelled by Ctrl+C or Esc |

## License

[MIT](LICENSE)
