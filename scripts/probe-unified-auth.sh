#!/usr/bin/env bash
set -euo pipefail

base=${1:-http://127.0.0.1:18087}
work=${2:-$(mktemp -d /tmp/toodue-unified-auth.XXXXXX)}
mkdir -p "$work"
jar="$work/cookies.txt"
email="probe-$(date +%s)-$RANDOM@example.com"
password="correct-horse-battery-staple"

curl_json() {
  curl -fsS "$@"
}

curl_json -c "$jar" \
  -H 'content-type: application/json' \
  -d "{\"name\":\"Probe User\",\"email\":\"$email\",\"password\":\"$password\"}" \
  "$base/api/auth/register" >/dev/null

key=$(
  curl_json -b "$jar" \
    -H 'content-type: application/json' \
    -d '{"name":"Unified auth probe"}' \
    "$base/api/api-keys" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["key"])'
)

projects_status=$(curl -sS -o "$work/projects.json" -w '%{http_code}' \
  -H "authorization: Bearer $key" \
  "$base/api/projects")
if [[ "$projects_status" != "200" ]]; then
  echo "expected bearer auth on /api/projects to return 200, got $projects_status" >&2
  cat "$work/projects.json" >&2 || true
  exit 1
fi

project_id=$(
  python3 -c 'import json,sys; data=json.load(open(sys.argv[1])); print(data[0]["id"])' "$work/projects.json"
)

create_status=$(curl -sS -o "$work/task.json" -w '%{http_code}' \
  -H "authorization: Bearer $key" \
  -H 'content-type: application/json' \
  -d "{\"project_id\":$project_id,\"name\":\"Unified auth task\"}" \
  "$base/api/tasks")
if [[ "$create_status" != "200" ]]; then
  echo "expected bearer auth on /api/tasks to create a task, got $create_status" >&2
  cat "$work/task.json" >&2 || true
  exit 1
fi

search_status=$(curl -sS -o "$work/search.json" -w '%{http_code}' \
  -H "authorization: Bearer $key" \
  "$base/api/tasks/search?q=Unified")
if [[ "$search_status" != "200" ]]; then
  echo "expected bearer auth on /api/tasks/search to return 200, got $search_status" >&2
  cat "$work/search.json" >&2 || true
  exit 1
fi

python3 - <<PY
import json
project = json.load(open('$work/projects.json'))[0]
task = json.load(open('$work/task.json'))
search = json.load(open('$work/search.json'))
assert project['name'] == 'Inbox', project
assert task['name'] == 'Unified auth task', task
assert any(item['id'] == task['id'] for item in search), search
print('unified bearer auth probe passed')
PY
