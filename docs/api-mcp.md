# TooDue API and MCP

TooDue exposes an AI-friendly REST API authenticated with account API keys, plus a stdio MCP server that wraps the API as tools.

## Create an API key

In the app: **Settings → API keys → Create**. Copy the key immediately; it is only shown once.

API keys are sent as Bearer tokens:

```sh
curl -H "Authorization: Bearer $TOODUE_API_KEY" https://app.toodue.com/api/ai/me
```

## REST endpoints

Base URL: `https://app.toodue.com/api/ai`

- `GET /me`
- `GET /projects`
- `POST /projects` `{ "name": "Project", "parent_id": 123 }`
- `GET /projects/:id`
- `GET /tasks?project_id=123&completed=false`
- `GET /tasks/search?q=invoice&limit=25`
- `GET /tasks/:id`
- `POST /tasks` `{ "project_id": 123, "name": "Task", "due_date": "2026-07-03", "priority": 4 }`
- `PATCH /tasks/:id` `{ "name": "New name", "completed": true }`
- `DELETE /tasks/:id`

Dates are `YYYY-MM-DD`; times are `HH:MM`; priority is `1`–`4`.

## MCP server

The MCP server is a dependency-free Node stdio script:

```sh
TOODUE_API_URL=https://app.toodue.com \
TOODUE_API_KEY=tdue_your_key_here \
node /path/to/toodue/mcp/toodue-mcp.mjs
```

Claude Desktop example:

```json
{
  "mcpServers": {
    "toodue": {
      "command": "node",
      "args": ["/Users/dustin/sd/src/toodue-github/mcp/toodue-mcp.mjs"],
      "env": {
        "TOODUE_API_URL": "https://app.toodue.com",
        "TOODUE_API_KEY": "tdue_your_key_here"
      }
    }
  }
}
```

Tools exposed:

- `toodue_me`
- `toodue_list_projects`
- `toodue_create_project`
- `toodue_list_tasks`
- `toodue_search_tasks`
- `toodue_create_task`
- `toodue_update_task`
- `toodue_complete_task`
- `toodue_delete_task`
