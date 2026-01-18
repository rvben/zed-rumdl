# zed-rumdl

[Zed](https://zed.dev) extension for [rumdl](https://github.com/rvben/rumdl) - a fast Markdown linter and formatter written in Rust.

## Features

- Real-time Markdown linting with inline diagnostics
- Format on save and on-demand formatting
- Automatic binary management (downloads rumdl automatically)
- Respects project configuration (`.rumdl.toml`, `pyproject.toml`)
- Support for custom binary path via LSP settings

## Installation

1. Open Zed
2. Open the Extensions panel (`cmd+shift+x` on macOS)
3. Search for "rumdl"
4. Click Install

The extension will automatically download the appropriate rumdl binary for your platform.

## Configuration

### Zed Settings

Add rumdl as a language server for Markdown files in your Zed settings (`.zed/settings.json` or global settings):

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

### Custom Binary Path

If you want to use a specific rumdl binary instead of the auto-downloaded one:

```json
{
  "lsp": {
    "rumdl": {
      "binary": {
        "path": "/path/to/rumdl"
      }
    }
  }
}
```

### Using a System-Installed rumdl

If you have rumdl installed via Homebrew, Cargo, or another method, the extension will use that instead of downloading:

```bash
# Install via Homebrew
brew install rvben/tap/rumdl

# Or via Cargo
cargo install rumdl

# Or via mise
mise use rumdl@latest
```

### Project Configuration

rumdl reads configuration from:

- `.rumdl.toml` in your project root
- `pyproject.toml` under `[tool.rumdl]`

See the [rumdl documentation](https://github.com/rvben/rumdl/blob/main/docs/CONFIGURATION.md) for all configuration options.

## Troubleshooting

### Extension not working

1. **Check logs**: Open the command palette and run `zed: open log`. Look for lines containing "rumdl".

2. **Verify rumdl is installed**: If using a system installation, verify with:
   ```bash
   which rumdl
   rumdl --version
   ```

3. **Restart Zed**: After installing or upgrading rumdl, restart Zed to pick up the new binary.

### Diagnostics not showing

1. **Check language server is enabled**: Ensure your settings include:
   ```json
   {
     "languages": {
       "Markdown": {
         "language_servers": ["rumdl"]
       }
     }
   }
   ```

2. **Check file type**: Ensure the file is recognized as Markdown (check the language indicator in the status bar).

3. **Check rumdl configuration**: Some rules may be disabled in your `.rumdl.toml`.

### Format on save not working

1. **Enable format on save** in your settings:
   ```json
   {
     "languages": {
       "Markdown": {
         "format_on_save": "on"
       }
     }
   }
   ```

2. **Check if rumdl supports fixing the issue**: Not all lint warnings have auto-fixes. Run `rumdl fix --help` to see supported rules.

### Wrong rumdl version

The extension checks for rumdl in this order:
1. Custom binary path from LSP settings
2. System PATH (`which rumdl`)
3. Auto-downloaded from GitHub releases

To use a specific version, either:
- Set a custom binary path in LSP settings
- Ensure your preferred version is first in PATH
- Remove any cached downloads (in Zed's extension data directory)

## Requirements

- Zed editor
- macOS (Intel or Apple Silicon), Linux (x86_64 or ARM64), or Windows

## Contributing

Contributions are welcome! Please see the [issue templates](.github/ISSUE_TEMPLATE) for reporting bugs or requesting features.

For issues with rumdl's linting rules or CLI, please use the [main rumdl repository](https://github.com/rvben/rumdl/issues).

## License

MIT
