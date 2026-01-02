# Implementation Plan: Improved `make install`

## Overview

Improve the `make install` experience to install `xearthlayer` to an appropriate bin directory following Linux conventions, with override capability.

**Complexity**: Low
**Estimated effort**: 45 minutes
**Dependencies**: None

## Current Behavior

```makefile
install: release
    $(CARGO) install --path .
    # Installs to ~/.cargo/bin
```

## Desired Behavior

```bash
make install              # Installs to ~/.local/bin/xearthlayer (user-local, no sudo)
make install PREFIX=/usr/local  # Installs to /usr/local/bin/xearthlayer (requires sudo)
```

## Design Decisions

### Default Location: `~/.local/bin`

Following the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/latest/), `~/.local/bin` is the standard location for user-local executables on modern Linux distributions.

**Rationale**:
- No sudo required for installation
- Automatically in PATH on most modern distros (Fedora, Ubuntu 20.04+, Arch)
- Consistent with other user-installed tools (pipx, cargo, etc.)
- Avoids conflicts with system package managers

### Alternative with PREFIX

For system-wide installation or custom locations:
- `PREFIX=/usr/local` → `/usr/local/bin` (traditional, needs sudo)
- `PREFIX=/opt/xearthlayer` → `/opt/xearthlayer/bin` (isolated)
- `PREFIX=$HOME/bin` → `~/bin/xearthlayer` (older convention)

## Implementation

### File: `Makefile`

```makefile
##@ Installation

# Installation directory configuration
# Default: ~/.local/bin (user-local, XDG compliant)
# Override: make install PREFIX=/usr/local
PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin

.PHONY: install
install: release ## Install binary to $(BINDIR)
	@echo "$(BLUE)Installing xearthlayer to $(BINDIR)...$(NC)"
	@# Create bin directory if needed
	@mkdir -p "$(BINDIR)"
	@# Copy binary
	@cp target/release/xearthlayer "$(BINDIR)/"
	@chmod 755 "$(BINDIR)/xearthlayer"
	@echo "$(GREEN)Installed: $(BINDIR)/xearthlayer$(NC)"
	@# Check if directory is in PATH
	@if ! echo "$$PATH" | tr ':' '\n' | grep -q "^$(BINDIR)$$"; then \
		echo ""; \
		echo "$(YELLOW)Warning: $(BINDIR) is not in your PATH$(NC)"; \
		echo "Add it to your shell profile:"; \
		echo "  echo 'export PATH=\"$(BINDIR):\$$PATH\"' >> ~/.bashrc"; \
		echo "  # or for zsh:"; \
		echo "  echo 'export PATH=\"$(BINDIR):\$$PATH\"' >> ~/.zshrc"; \
	fi

.PHONY: uninstall
uninstall: ## Remove installed binary from $(BINDIR)
	@echo "$(BLUE)Uninstalling xearthlayer from $(BINDIR)...$(NC)"
	@if [ -f "$(BINDIR)/xearthlayer" ]; then \
		rm "$(BINDIR)/xearthlayer"; \
		echo "$(GREEN)Uninstalled: $(BINDIR)/xearthlayer$(NC)"; \
	else \
		echo "$(YELLOW)Not found: $(BINDIR)/xearthlayer$(NC)"; \
	fi
```

## Usage Examples

```bash
# User-local installation (recommended)
make install
# Output: Installed: /home/user/.local/bin/xearthlayer

# System-wide installation (requires sudo)
sudo make install PREFIX=/usr/local
# Output: Installed: /usr/local/bin/xearthlayer

# Custom location
make install PREFIX=/opt/xearthlayer
# Output: Installed: /opt/xearthlayer/bin/xearthlayer

# Custom bin directory directly
make install BINDIR=/custom/path/bin
# Output: Installed: /custom/path/bin/xearthlayer
```

## Testing

1. **Default install**: `make install` creates `~/.local/bin/xearthlayer`
2. **PREFIX override**: `make install PREFIX=/tmp/test` creates `/tmp/test/bin/xearthlayer`
3. **BINDIR override**: `make install BINDIR=/tmp/test` creates `/tmp/test/xearthlayer`
4. **PATH warning**: If target not in PATH, warning is shown
5. **Uninstall**: `make uninstall` removes the binary
6. **Permissions**: Binary has 755 permissions

## Documentation Updates

- Update README.md installation section
- Update CLAUDE.md CLI commands section
- Add note about PATH configuration

## Compatibility Notes

| Distro | `~/.local/bin` in PATH by default |
|--------|-----------------------------------|
| Ubuntu 20.04+ | Yes |
| Fedora | Yes |
| Arch Linux | Yes (via bash profile) |
| Debian | Partial (need to create dir first) |
| openSUSE | Yes |

For distros that don't have it in PATH, the warning message provides instructions.
