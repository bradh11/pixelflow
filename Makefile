# PixelFlow developer commands. Run `make` (or `make help`) to list them.

APP := app
SHELL_CRATE := app/src-tauri
SHOW ?= examples/shows/demo.pixelflow.json

.DEFAULT_GOAL := help
.PHONY: help setup run ui test test-rust test-app lint fmt build cli-validate cli-map discover clean

help: ## List the available commands
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z_-]+:.*## / {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

setup: ## Install the app's JavaScript dependencies (run once, and after pulling changes)
	cd $(APP) && pnpm install

$(APP)/node_modules: $(APP)/package.json $(APP)/pnpm-lock.yaml
	cd $(APP) && pnpm install
	@touch $@

run: $(APP)/node_modules ## Run the desktop app with live reload
	cd $(APP) && pnpm tauri dev

ui: $(APP)/node_modules ## Run the UI in a browser with a sample show (http://localhost:1420/?demo; sends nothing to controllers)
	cd $(APP) && pnpm dev

test: test-rust test-app ## Run every test

test-rust: ## Run the Rust engine tests
	cargo test --workspace

test-app: $(APP)/node_modules ## Run the UI and desktop-shell tests
	cd $(APP) && pnpm typecheck && pnpm test && pnpm build
	cd $(SHELL_CRATE) && cargo test

lint: $(APP)/node_modules ## Check formatting and lints (same as CI)
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cd $(APP) && pnpm typecheck
	cd $(SHELL_CRATE) && cargo fmt --check && cargo clippy --all-targets -- -D warnings

fmt: ## Format all Rust code
	cargo fmt --all
	cd $(SHELL_CRATE) && cargo fmt

build: $(APP)/node_modules ## Build an installable app (output in app/src-tauri/target/release/bundle)
	cd $(APP) && pnpm tauri build

cli-validate: ## Check a show file: make cli-validate SHOW=path/to/show.pixelflow.json
	cargo run -q -p pf-cli -- validate $(SHOW)

cli-map: ## Print a show's channel map: make cli-map SHOW=path/to/show.pixelflow.json
	cargo run -q -p pf-cli -- map $(SHOW)

discover: ## Find FPP, Falcon, and WLED controllers on your network (read-only)
	cargo run -q -p pf-cli -- discover

clean: ## Remove build outputs (keeps node_modules)
	cargo clean
	cd $(SHELL_CRATE) && cargo clean
	rm -rf $(APP)/dist
