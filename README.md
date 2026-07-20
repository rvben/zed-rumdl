# zed-rumdl

[Zed](https://zed.dev) extension for [rumdl](https://github.com/rvben/rumdl) - a fast Markdown linter and formatter written in Rust.

## Features

- Real-time Markdown linting with inline diagnostics
- Format on save and on-demand formatting
- Automatic binary management (downloads rumdl automatically)
- Respects project configuration (`.rumdl.toml`, `pyproject.toml`)
- Configurable via Zed's LSP settings

## Installation

1. Open Zed
2. Open the Extensions panel (`cmd+shift+x` on macOS)
3. Search for "rumdl"
4. Click Install

The extension will automatically download the appropriate rumdl binary for your platform.

## Configuration

### Zed Settings

rumdl starts automatically for Markdown files once the extension is installed.
To format with rumdl, set it as the formatter explicitly. Zed's default
(`"formatter": "auto"`) formats Markdown with its built-in Prettier
integration, so without this setting Prettier formats your files, not rumdl:

```json
{
  "languages": {
    "Markdown": {
      "formatter": {
        "language_server": {
          "name": "rumdl"
        }
      },
      "format_on_save": "on"
    }
  }
}
```

If you set `language_servers` explicitly, keep the `"..."` placeholder so
other language servers stay enabled alongside rumdl:

```json
{
  "languages": {
    "Markdown": {
      "language_servers": ["rumdl", "..."]
    }
  }
}
```

### Custom Binary Path

If you want to use a specific rumdl binary instead of the auto-downloaded one,
you can also pass custom arguments (for example rumdl's global `--config` flag)
and environment variables:

```json
{
  "lsp": {
    "rumdl": {
      "binary": {
        "path": "/path/to/rumdl",
        "arguments": ["server", "--config", "/path/to/.rumdl.toml"],
        "env": {
          "RUMDL_LOG": "debug"
        }
      }
    }
  }
}
```

Only `path` is required; `arguments` defaults to `["server"]`, and `env`
entries are added to the server's environment. A configured `path` takes
precedence over rumdl on your PATH, which in turn takes precedence over the
auto-downloaded binary. Changes apply on the next language server restart, no
Zed restart needed.

### LSP Settings

You can pass configuration to rumdl via Zed's LSP settings. The shape matches
rumdl's LSP configuration: server options at the top level (camelCase), rule
configuration nested under `settings`:

```json
{
  "lsp": {
    "rumdl": {
      "settings": {
        "configPath": ".rumdl.toml",
        "enableAutoFix": true,
        "settings": {
          "lineLength": 120,
          "disable": ["MD033"],
          "MD013": {
            "lineLength": 120
          }
        }
      }
    }
  }
}
```

These are sent to rumdl both as `initialize` options and on
`workspace/didChangeConfiguration`, so edits apply without restarting the
server. To send a different payload at initialization, set
`lsp.rumdl.initialization_options`, which takes precedence over `settings`
for the `initialize` request.

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

3. **Restart the language server**: After installing or upgrading rumdl, run `editor: restart language server` from the command palette (or restart Zed) to pick up the new binary.

### Diagnostics not showing

1. **Check language server is enabled**: rumdl is enabled by default for Markdown. If you override `language_servers` in your settings, make sure the list includes `"rumdl"`.

2. **Check file type**: Ensure the file is recognized as Markdown (check the language indicator in the status bar).

3. **Check rumdl configuration**: Some rules may be disabled in your `.rumdl.toml`.

### Format on save not working

1. **Set rumdl as the formatter and enable format on save**:
   ```json
   {
     "languages": {
       "Markdown": {
         "formatter": {
           "language_server": {
             "name": "rumdl"
           }
         },
         "format_on_save": "on"
       }
     }
   }
   ```
   Without the `formatter` setting, Zed formats Markdown with its built-in Prettier integration instead of rumdl.

2. **Check if rumdl supports fixing the issue**: Not all lint warnings have auto-fixes. Run `rumdl check --fix` to apply the fixes rumdl can make automatically.

### Wrong rumdl version

The extension checks for rumdl in this order:
1. Custom binary path from LSP settings
2. System PATH (`which rumdl`)
3. Auto-downloaded from GitHub releases

To use a specific version, either:
- Set a custom binary path in LSP settings
- Ensure your preferred version is first in PATH
- Remove any cached downloads (in Zed's extension data directory)

Settings and PATH are re-checked every time the language server starts, so switching binaries only needs `editor: restart language server`, not a Zed restart. If GitHub is unreachable, the extension falls back to the most recently downloaded binary.

## Requirements

- Zed editor
- macOS (Intel or Apple Silicon), Linux (x86_64 or ARM64), or Windows (x86_64)

## Contributing

Contributions are welcome! Please see the [issue templates](.github/ISSUE_TEMPLATE) for reporting bugs or requesting features.

For issues with rumdl's linting rules or CLI, please use the [main rumdl repository](https://github.com/rvben/rumdl/issues).

## License

MIT
