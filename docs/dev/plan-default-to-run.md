# Implementation Plan: Default to `run` Command

## Overview

When `xearthlayer` is executed with no arguments, it should default to the `run` command instead of showing the help message.

**Complexity**: Low
**Estimated effort**: 30 minutes
**Dependencies**: None

## Current Behavior

```bash
$ xearthlayer
# Shows help/usage message (requires subcommand)
```

## Desired Behavior

```bash
$ xearthlayer
# Equivalent to: xearthlayer run
# Starts XEarthLayer and mounts all installed packages
```

## Implementation

### File: `xearthlayer-cli/src/main.rs`

#### 1. Make subcommand optional with default

Change the `Cli` struct to make `command` optional:

```rust
#[derive(Parser)]
#[command(name = "xearthlayer")]
#[command(version = xearthlayer::VERSION)]
#[command(about = "Satellite imagery streaming for X-Plane", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}
```

#### 2. Handle None case in main()

Update the match statement to default to `run` when no command is provided:

```rust
fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        None => {
            // Default to 'run' command with all defaults
            commands::run::run(commands::run::RunArgs::default())
        }
        Some(Commands::Init) => commands::init::run(),
        Some(Commands::Config { command }) => commands::config::run(command),
        // ... rest of matches with Some() wrapper
    };

    if let Err(e) = result {
        e.exit();
    }
}
```

### File: `xearthlayer-cli/src/commands/run.rs`

#### 3. Implement Default for RunArgs

Add `#[derive(Default)]` to `RunArgs`:

```rust
#[derive(Default)]
pub struct RunArgs {
    pub provider: Option<ProviderType>,
    pub google_api_key: Option<String>,
    pub mapbox_token: Option<String>,
    pub dds_format: Option<DdsCompression>,
    pub timeout: Option<u64>,
    pub parallel: Option<usize>,
    pub no_cache: bool,
    pub debug: bool,
    pub no_prefetch: bool,
    pub airport: Option<String>,
}
```

## Testing

1. **No arguments**: `xearthlayer` should start the run command
2. **Explicit run**: `xearthlayer run` should behave identically
3. **Other commands**: `xearthlayer init`, `xearthlayer config list` etc. should work as before
4. **Help**: `xearthlayer --help` should still show all commands

## Documentation Updates

- Update `CLAUDE.md` CLI section to note default behavior
- Update man page if one exists

## Rollback

If this causes issues, simply revert to requiring a subcommand by removing `Option<>` wrapper.
