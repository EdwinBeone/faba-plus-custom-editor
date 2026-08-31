#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cloud_dir="$(cd -- "$script_dir/.." && pwd)"
backup_dir="$cloud_dir/backups"
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"

mkdir -p "$backup_dir"
umask 077
docker_command=(docker)
if ! docker info >/dev/null 2>&1; then
  docker_command=(sudo docker)
fi

"${docker_command[@]}" compose --project-directory "$cloud_dir" --env-file "$cloud_dir/.env" exec -T postgres \
  pg_dump --format=custom --no-owner --username=faba faba > "$backup_dir/faba-$timestamp.dump"
echo "$backup_dir/faba-$timestamp.dump"
"${docker_command[@]}" compose --project-directory "$cloud_dir" --env-file "$cloud_dir/.env" exec -T api \
  tar -C /data -czf - audio > "$backup_dir/faba-audio-$timestamp.tar.gz"
echo "$backup_dir/faba-audio-$timestamp.tar.gz"
