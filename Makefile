all:
	cargo check
	cargo build

setup:
	$(MAKE)
	mkdir -p "$(HOME)/.local/bin"
	ln -sf "$(CURDIR)/target/debug/todo-cli" "$(HOME)/.local/bin/todo-app"
	@if ! printf '%s\n' "$$PATH" | grep -qE '(^|:)'$$HOME'/.local/bin(:|$$)'; then \
		echo 'export PATH="$$HOME/.local/bin:$$PATH"' >> "$(HOME)/.zshrc"; \
		echo 'Added ~/.local/bin to PATH in ~/.zshrc'; \
	else \
		echo '~/.local/bin already in PATH'; \
	fi
