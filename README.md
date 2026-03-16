# KeepTeamsAvailable

Keep your Microsoft Teams status as **Available** without affecting your daily usage.

## How it works

The program periodically simulates a quick Caps Lock key toggle (press and release) via macOS IOKit, generating just enough input activity to prevent Teams from switching your status to "Away". The toggle happens so fast (~100ms) that it won't interfere with your normal typing or workflow.

## Requirements

- macOS
- Accessibility permissions (System Settings > Privacy & Security > Accessibility)

## Build

```bash
cargo build --release
```

The binary will be at `target/release/online`.

## Usage

```bash
./online
```

Press `Ctrl+C` to stop. The program will automatically restore Caps Lock to the off state on exit.

## License

See [LICENSE](LICENSE) for details.
