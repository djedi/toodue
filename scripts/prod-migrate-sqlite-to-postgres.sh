#!/usr/bin/env bash
set -euo pipefail

# Run this on the production host after the Postgres container exists and the
# TooDue app is stopped. Defaults match docker-compose.prod.yml.
#
# Example:
#   SQLITE_DB=/var/lib/docker/volumes/toodue_toodue-data/_data/toodue.db \
#   POSTGRES_CONTAINER=toodue-db ./scripts/prod-migrate-sqlite-to-postgres.sh

SQLITE_DB=${SQLITE_DB:-/var/lib/docker/volumes/toodue_toodue-data/_data/toodue.db}
POSTGRES_CONTAINER=${POSTGRES_CONTAINER:-toodue-db}
POSTGRES_USER=${POSTGRES_USER:-toodue}
POSTGRES_DB=${POSTGRES_DB:-toodue}
BACKUP_DIR=${BACKUP_DIR:-/root/toodue-backups}
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
STAMP=$(date -u +%Y%m%dT%H%M%SZ)

if [[ ! -f "$SQLITE_DB" ]]; then
  echo "SQLite DB not found: $SQLITE_DB" >&2
  exit 1
fi
if ! docker ps --format '{{.Names}}' | grep -qx "$POSTGRES_CONTAINER"; then
  echo "Postgres container is not running: $POSTGRES_CONTAINER" >&2
  exit 1
fi

mkdir -p "$BACKUP_DIR"
BACKUP_DB="$BACKUP_DIR/toodue-sqlite-$STAMP.db"

# Make a consistent SQLite backup even if WAL files exist. Prefer sqlite3 when
# available; otherwise stop the app before running this and copy the DB file.
if command -v sqlite3 >/dev/null 2>&1; then
  sqlite3 "$SQLITE_DB" "PRAGMA wal_checkpoint(FULL); VACUUM INTO '$BACKUP_DB';"
else
  cp "$SQLITE_DB" "$BACKUP_DB"
fi
chmod 600 "$BACKUP_DB"
echo "SQLite backup: $BACKUP_DB"

python3 "$SCRIPT_DIR/migrate-sqlite-to-postgres.py" "$BACKUP_DB" \
  | docker exec -i "$POSTGRES_CONTAINER" psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -v ON_ERROR_STOP=1

echo "Migration loaded into Postgres container $POSTGRES_CONTAINER/$POSTGRES_DB"
