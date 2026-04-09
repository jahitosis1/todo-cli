all:
	cargo check
	cargo build

setup:
	make
	mkdir -p ~/.local/bin
	rm ~/.local/bin/todo-app
	ln -s "$(CURDIR)/target/debug/todo-cli" ~/.local/bin/todo-app
