# zed-rumdl

[Zed](https://zed.dev) extension for [rumdl](https://github.com/rvben/rumdl) - a fast Markdown linter and formatter written in Rust.

## Features

- Real-time Markdown linting with inline diagnostics
- Format on save and on-demand formatting
- Automatic binary management (downloads rumdl automatically)
- Respects project configuration (`.rumdl.toml`, `pyproject.toml`)

## Installation

1. Open Zed
2. Open the Extensions panel (`cmd+shift+x` on macOS)
3. Search for "rumdl"
4. Click Install

The extension will automatically download the appropriate rumdl binary for your platform.

## Configuration

### Using a system-installed rumdl

If you have rumdl installed via Homebrew, Cargo, or another method, the extension will use that instead of downloading.

```bash
# Install via Homebrew
brew install rvben/tap/rumdl

# Or via Cargo
cargo install rumdl

# Or via mise
mise use rumdl@latest
```

### Project configuration

rumdl reads configuration from:
- `.rumdl.toml` in your project root
- `pyproject.toml` under `[tool.rumdl]`

See the [rumdl documentation](https://github.com/rvben/rumdl/blob/main/docs/CONFIGURATION.md) for all configuration options.

### Zed settings

You can configure rumdl behavior in your Zed settings (`.zed/settings.json`):

```json
{
  "languages": {
    "Markdown": {
      "language_servers": ["rumdl"],
      "format_on_save": "on"
    }
  }
}
```

## Requirements

- Zed editor
- macOS (Intel or Apple Silicon), Linux (x86_64 or ARM64), or Windows

## License

MIT
