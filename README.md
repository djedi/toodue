# TooDue

A fast, open-source task manager you can self-host. Rust backend, Svelte PWA frontend,
real-time sync over SSE.

## Features

- **Inbox / Today / Upcoming / Projects** — the four tabs you expect, responsive from phone to desktop
- **Full task model** — name, description, date + time, repeating schedules, deadline,
  priority (P1–P4), project, sub-tasks, comments, and file attachments
- **Repeating tasks** — daily, weekly, monthly, or yearly schedules create the next occurrence
  when completed; overdue schedules advance to the next future date and recurring parent tasks
  carry their sub-tasks forward
- **Nested projects** and project sharing: invite another TooDue user by email and stay in sync
- **Real-time** — server-sent events push every change to all members instantly
- **PWA** — installable on iOS/Android/desktop, light/dark/system theme
- **Calendar integration** — a per-user iCal feed URL you can subscribe to from Google Calendar
  or Fantastical (tasks with dates appear as events, deadlines as separate all-day events)
- **AI agents (MCP)** — a remote MCP server at `/mcp` with browser OAuth: point Claude, ChatGPT,
  Cursor, or any MCP client at it, approve access, and let your agent manage your to-dos
- **Small and fast** — a single Rust binary with an embedded SQLite database serves the API and
  the static frontend

## Local development

Requirements: Docker (with Compose). Everything runs in containers.

```sh
make up      # start backend (:8080) + frontend (:5173)
make logs    # tail everything
make down    # stop
make help    # all commands
```

Open http://localhost:5173, sign up, and go. The backend recompiles on save (cargo-watch);
the frontend hot-reloads (Vite). Data lives in `./data/toodue.db`.

To test sharing/real-time locally, open a second browser (or private window), register a second
user, then share a project with that user's email from Project → settings.

## Self-hosting (production)

One container serves everything:

```sh
make prod-build
make prod-run    # http://localhost:8080, data in the `toodue-data` volume
```

Or run the production Compose stack:

```sh
cp .env.prod.example .env.prod
# Edit TOODUE_PORT/BIND_ADDR if your reverse proxy runs in Docker and needs host reachability.
docker compose --env-file .env.prod -f docker-compose.prod.yml up -d --build
```

Or with the image directly:

```sh
docker build -t toodue .
docker run -d --name toodue -p 8080:8080 -v toodue-data:/data toodue
```

Put it behind your reverse proxy of choice with TLS (SSE needs no special configuration, but
disable response buffering for `/api/events` if your proxy buffers by default).

| Env var      | Default       | Meaning                        |
| ------------ | ------------- | ------------------------------ |
| `PORT`       | `8080`        | HTTP port                      |
| `DATA_DIR`   | `./data`      | SQLite DB + attachment storage |
| `STATIC_DIR` | `./static`    | Built frontend to serve        |
| `RUST_LOG`   | `toodue=info` | Log filter                     |
| `PUBLIC_URL` | (from request)| Public origin, used for OAuth/MCP metadata and Google sync |

## Calendar integration

### Google Calendar two-way sync

Set `PUBLIC_URL`, `GOOGLE_CLIENT_ID`, and `GOOGLE_CLIENT_SECRET` (OAuth web client with
redirect URI `<PUBLIC_URL>/api/google/callback`), then Settings (the gear icon) →
**Connect Google Calendar**. TooDue creates a dedicated "TooDue" calendar and mirrors your
dated tasks into it instantly. Moving or rescheduling an event in Google Calendar updates the
task's date in TooDue (via a webhook watch channel), and shared-project members each get
their own synced copy. Date/time changes flow inbound; all other task fields are owned by
TooDue.

### Read-only iCal feed

Settings (the gear icon) → **Calendar feed** copies your personal iCal URL. Subscribe to it:

- **Google Calendar**: Settings → Add calendar → From URL
- **Fantastical**: File → New Calendar Subscription

The URL contains a private token. If it leaks, rotate it with `POST /api/me/calendar`.

## AI agents (MCP)

TooDue serves a remote [MCP](https://modelcontextprotocol.io) server at `<your origin>/mcp`.
Add that URL to your agent as a remote MCP server. The agent opens TooDue in your browser,
you choose what it may do (view / create & update / delete), and it's connected. Manage and
disconnect agents in Settings → **AI agents**. Set `PUBLIC_URL` in production so OAuth
metadata advertises the right origin behind your proxy.

Setup for each client: [`docs/mcp.html`](docs/mcp.html). Technical reference:
[`docs/api-mcp.md`](docs/api-mcp.md).

## Architecture

- `backend/` — Rust (axum + sqlx/SQLite). REST API under `/api`, SSE stream at `/api/events`,
  iCal feed at `/api/calendar/<token>.ics`. Serves the built frontend in production.
- `frontend/` — Svelte 5 + Vite + Tailwind 4, Lucide icons, `vite-plugin-pwa`.
  A hash-routed SPA; state lives in `src/lib/state.svelte.js`.
- Auth is cookie-based sessions with argon2 password hashing. API keys and OAuth (for MCP
  clients, `backend/src/oauth.rs`) use bearer tokens; the MCP server is `backend/src/mcp.rs`.
- Sharing: every project has members; all mutations broadcast to project members over SSE.

## Roadmap

- SaaS offering with fair, affordable subscriptions
- Native iOS/Android apps
- Labels, filters, and reminders
