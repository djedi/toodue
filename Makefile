.DEFAULT_GOAL := help
COMPOSE := docker compose

# Pick up local port overrides (also read by docker compose itself).
-include .env

.PHONY: help up down restart logs logs-backend logs-frontend ps sh-backend sh-frontend \
        fmt check test build icons db-reset prod-build prod-run clean

help: ## Show this help
	@echo ""
	@echo "  TooDue — development commands"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'
	@echo ""

up: ## Start the dev environment (ports overridable via FRONTEND_PORT/BACKEND_PORT in .env)
	$(COMPOSE) up -d --build
	@echo ""
	@echo "  app       → http://localhost:$(or $(FRONTEND_PORT),5173)"
	@echo "  api       → http://localhost:$(or $(BACKEND_PORT),8080)/api"
	@echo "  first backend build takes a few minutes; watch with: make logs-backend"

down: ## Stop the dev environment
	$(COMPOSE) down

restart: ## Restart both services
	$(COMPOSE) restart

logs: ## Tail logs from all services
	$(COMPOSE) logs -f

logs-backend: ## Tail backend logs
	$(COMPOSE) logs -f backend

logs-frontend: ## Tail frontend logs
	$(COMPOSE) logs -f frontend

ps: ## Show service status
	$(COMPOSE) ps

sh-backend: ## Shell into the backend container
	$(COMPOSE) exec backend bash

sh-frontend: ## Shell into the frontend container
	$(COMPOSE) exec frontend sh

fmt: ## Format the Rust code (runs locally)
	cd backend && cargo fmt

check: ## Type-check backend and build frontend (runs locally)
	cd backend && cargo check
	cd frontend && npm install && npm run build

test: ## Run backend tests (runs locally)
	cd backend && cargo test

icons: ## Regenerate the PWA icon set
	cd frontend && node scripts/gen-icons.mjs

db-reset: ## Delete the dev database (fresh start)
	$(COMPOSE) stop backend
	rm -f data/toodue.db data/toodue.db-wal data/toodue.db-shm
	$(COMPOSE) start backend

prod-build: ## Build the production image (frontend + backend in one)
	docker build -t toodue .

prod-run: ## Run the production image on :8080 with a persistent volume
	docker run --rm -p 8080:8080 -v toodue-data:/data toodue

clean: ## Stop everything and remove build volumes
	$(COMPOSE) down -v
