CONFIG ?= configs/default.toml
PORT ?= 8080
RUST_LOG ?= info

CARGO := cargo
NPM := npm

.PHONY: help build test check clean \
	engine dashboard web-install web-dev web-build \
	release-engine release-dashboard

help:
	@echo "polymarket-crypto-sdk"
	@echo ""
	@echo "  make build              Build Rust workspace (debug)"
	@echo "  make test               Run Rust tests"
	@echo "  make check              cargo check --workspace"
	@echo "  make clean              cargo clean"
	@echo ""
	@echo "  make engine             Run trading engine (dry-run unless config says otherwise)"
	@echo "  make dashboard          Run read-only dashboard API (PORT=$(PORT))"
	@echo "  make web-install        npm install in web/"
	@echo "  make web-dev            Next.js dev server (proxies API to :$(PORT))"
	@echo "  make web-build          Production build of web/"
	@echo ""
	@echo "  make release-engine     Build engine binary (release)"
	@echo "  make release-dashboard  Build dashboard-api binary (release)"
	@echo ""
	@echo "Variables: CONFIG=$(CONFIG)  PORT=$(PORT)  RUST_LOG=$(RUST_LOG)"

build:
	$(CARGO) build --workspace

test:
	$(CARGO) test --workspace

check:
	$(CARGO) check --workspace

clean:
	$(CARGO) clean

engine:
	RUST_LOG=$(RUST_LOG) $(CARGO) run -p poly-crypto-engine -- --config $(CONFIG)

dashboard:
	RUST_LOG=$(RUST_LOG) $(CARGO) run -p poly-dashboard-api -- --config $(CONFIG) --port $(PORT)

web-install:
	cd web && $(NPM) install

web-dev: web-install
	cd web && $(NPM) run dev

web-build: web-install
	cd web && $(NPM) run build

release-engine:
	$(CARGO) build -p poly-crypto-engine --release

release-dashboard:
	$(CARGO) build -p poly-dashboard-api --release
