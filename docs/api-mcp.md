# TooDue API and MCP

TooDue exposes a remote MCP server that AI agents connect to with browser OAuth, plus a REST API authenticated with account API keys.

- Public setup guide: [`docs/mcp.html`](mcp.html) (published at https://docs.toodue.com/mcp.html)
- REST reference: [`docs/openapi.yaml`](openapi.yaml) (published at https://docs.toodue.com)

## Remote MCP server (recommended)

URL: `https://app.toodue.com/mcp` (self-hosted: `<PUBLIC_URL>/mcp`; shown in **Settings → AI agents**).

Add the URL to any MCP client that supports remote/HTTP servers. On first use the client discovers TooDue's OAuth server, registers itself, and opens the browser to `/connect`, where the user logs in, picks permissions, and approves. Examples:

```sh
# Claude Code
claude mcp add --transport http toodue https://app.toodue.com/mcp
```

```json
// Cursor (~/.cursor/mcp.json)
{ "mcpServers": { "toodue": { "url": "https://app.toodue.com/mcp" } } }
```

Clients that only speak stdio can bridge with `npx -y mcp-remote https://app.toodue.com/mcp`. See `mcp.html` for Claude, ChatGPT, VS Code, and Codex.

### Scopes

| Scope    | Grants                                                                    |
| -------- | ------------------------------------------------------------------------- |
| `read`   | Always granted. View projects, tasks, sub-tasks, comments.                |
| `write`  | Create, update, complete, and comment on tasks; create projects.          |
| `delete` | Delete tasks.                                                             |

`tools/list` only returns tools the connection's scopes allow, and `tools/call` enforces them.

### Tools

| Tool                   | Scope  |
| ---------------------- | ------ |
| `toodue_me`            | read   |
| `toodue_list_projects` | read   |
| `toodue_get_project`   | read   |
| `toodue_agenda`        | read   |
| `toodue_list_tasks`    | read   |
| `toodue_search_tasks`  | read   |
| `toodue_get_task`      | read   |
| `toodue_create_task`   | write  |
| `toodue_update_task`   | write  |
| `toodue_complete_task` | write  |
| `toodue_add_comment`   | write  |
| `toodue_create_project`| write  |
| `toodue_delete_task`   | delete |

Tools call the same handlers as the app, so membership checks, validation, real-time SSE updates, and Google Calendar sync all apply.

### Endpoints

| Endpoint                                         | Notes                                                        |
| ------------------------------------------------ | ------------------------------------------------------------ |
| `POST /mcp`                                      | Streamable HTTP, stateless, JSON responses. `GET` → 405.     |
| `GET /.well-known/oauth-protected-resource[/mcp]`| RFC 9728 metadata; linked from `WWW-Authenticate` on 401.    |
| `GET /.well-known/oauth-authorization-server`    | RFC 8414 metadata.                                           |
| `POST /oauth/register`                           | RFC 7591 dynamic registration (public clients).              |
| `GET /oauth/authorize`                           | Code flow, PKCE S256 required; redirects to SPA `/connect`.  |
| `POST /oauth/token`                              | `authorization_code`, `refresh_token` (rotating).            |
| `POST /oauth/revoke`                             | RFC 7009.                                                    |

App-only (session cookie) endpoints backing the UI: `GET/POST /api/oauth/requests/:id` (consent), `GET /api/oauth/connections`, `DELETE /api/oauth/connections/:id`, `GET /api/mcp/info`.

Tokens: access tokens (`tdue_at_…`) last 1 hour; refresh tokens (`tdue_rt_…`) last 90 days and rotate on use. Only SHA-256 hashes are stored. OAuth tokens are accepted only at `/mcp`. Set `PUBLIC_URL` in production so metadata URLs are correct behind a proxy (otherwise they're derived from `X-Forwarded-Proto`/`X-Forwarded-Host`/`Host`).

API keys also work at `/mcp` as `Authorization: Bearer tdue_…` and grant all scopes.

## REST API with API keys

In the app: **Settings → API keys → Create**. Copy the key immediately; it is only shown once.

```sh
curl -H "Authorization: Bearer $TOODUE_API_KEY" https://app.toodue.com/api/me
```

See `openapi.yaml` for the canonical endpoints. The legacy `/api/ai/*` aliases remain for compatibility.

## Local stdio MCP script (legacy)

`mcp/toodue-mcp.mjs` is a dependency-free stdio wrapper around the REST API for clients without remote MCP support. Prefer the remote server or `mcp-remote`.

```sh
TOODUE_API_URL=https://app.toodue.com TOODUE_API_KEY=tdue_your_key_here node mcp/toodue-mcp.mjs
```
