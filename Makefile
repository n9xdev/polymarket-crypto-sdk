CONFIG ?= configs/default.toml
# 8081 avoids common clashes (e.g. other bots on :8080). Override: make dashboard PORT=8080
PORT ?= 8081
RUST_LOG ?= info

CARGO := cargo
NPM := npm

.PHONY: help build test check clean \
	db-up db-down db-wait db-logs \
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
	@echo "  make db-up              Start Postgres (docker compose, localhost:5433)"
	@echo "  make db-down            Stop Postgres container"
	@echo "  make db-logs            Tail Postgres logs"
	@echo ""
	@echo "  make engine             Run trading engine (dry-run unless config says otherwise)"
	@echo "  make dashboard          Start Postgres if needed, then dashboard API (PORT=$(PORT))"
	@echo "  make web-install        npm install in web/"
	@echo "  make web-dev            Next.js dev (auto-picks free port from 3000; WEB_PORT= to fix)"
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

db-up:
	docker compose up -d postgres
	@$(MAKE) db-wait

db-wait:
	@echo "Waiting for Postgres (poly@localhost:5433)..."
	@i=0; while [ $$i -lt 60 ]; do \
		if docker compose exec -T postgres pg_isready -U poly -d poly_crypto >/dev/null 2>&1; then \
			echo "Postgres ready."; exit 0; \
		fi; \
		i=$$((i+1)); sleep 1; \
	done; \
	echo "Postgres did not become ready in time"; exit 1

db-down:
	docker compose down

db-logs:
	docker compose logs -f postgres

engine:
	RUST_LOG=$(RUST_LOG) $(CARGO) run -p poly-crypto-engine -- --config $(CONFIG)

dashboard: db-up
	RUST_LOG=$(RUST_LOG) $(CARGO) run -p poly-dashboard-api -- --config $(CONFIG) --port $(PORT)

web-install:
	cd web && $(NPM) install

web-dev: web-install
	@WEB=$${WEB_PORT:-$$($(CURDIR)/scripts/pick-free-port.sh 3000 3099)}; \
	echo "Next.js: http://localhost:$$WEB (dashboard API :$(PORT))"; \
	cd web && DASHBOARD_API_PORT=$(PORT) $(NPM) run dev -- -p $$WEB

web-build: web-install
	cd web && $(NPM) run build

release-engine:
	$(CARGO) build -p poly-crypto-engine --release

release-dashboard:
	$(CARGO) build -p poly-dashboard-api --release
