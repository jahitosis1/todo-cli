BINARY_NAME := todo-cli
LINK_NAME := todo-app
LOCAL_BIN := $(HOME)/.local/bin

all: check-cargo
	cargo check
	cargo build

check-cargo:
	@command -v cargo >/dev/null 2>&1 || { \
		echo "Error: cargo is not installed."; \
		echo "Install Rust via rustup: https://rustup.rs/"; \
		exit 1; \
	}

setup: all
	@set -e; \
	SHELL_NAME="$$(basename "$$SHELL")"; \
	case "$$SHELL_NAME" in \
		zsh) RC_FILE="$(HOME)/.zshrc" ;; \
		bash) RC_FILE="$(HOME)/.bashrc" ;; \
		fish) RC_FILE="$(HOME)/.config/fish/config.fish" ;; \
		*) RC_FILE="$(HOME)/.profile" ;; \
	esac; \
	mkdir -p "$(LOCAL_BIN)"; \
	ln -sf "$(CURDIR)/target/debug/$(BINARY_NAME)" "$(LOCAL_BIN)/$(LINK_NAME)"; \
	if [ "$$SHELL_NAME" = "fish" ]; then \
		mkdir -p "$(HOME)/.config/fish"; \
		LINE='fish_add_path $$HOME/.local/bin'; \
	else \
		LINE='export PATH="$$HOME/.local/bin:$$PATH"'; \
	fi; \
	if [ -f "$$RC_FILE" ] && grep -Fqx "$$LINE" "$$RC_FILE"; then \
		echo "~/.local/bin already configured in $$RC_FILE"; \
	else \
		echo "$$LINE" >> "$$RC_FILE"; \
		echo "Added ~/.local/bin to PATH in $$RC_FILE"; \
	fi; \
	echo "Symlinked $(LINK_NAME) -> $(LOCAL_BIN)/$(LINK_NAME)"
