#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cloud_dir="$(cd -- "$script_dir/.." && pwd)"
env_file="$cloud_dir/.env"

if [[ ! -f "$env_file" ]]; then
  umask 077
  db_password="$(openssl rand -hex 48)"
  {
    echo "POSTGRES_PASSWORD=$db_password"
    echo "FABA_PORT=8787"
    echo "SESSION_DAYS=90"
    echo "MAX_TRACK_BYTES=209715200"
    echo "MAX_ACCOUNT_BYTES=5368709120"
    echo "MAX_TOTAL_BYTES=53687091200"
  } > "$env_file"
  echo "Configuration secrète créée dans $env_file"
fi

docker_command=(docker)
if ! docker info >/dev/null 2>&1; then
  docker_command=(sudo docker)
fi

"${docker_command[@]}" compose --project-directory "$cloud_dir" --env-file "$env_file" up -d --build
"${docker_command[@]}" compose --project-directory "$cloud_dir" --env-file "$env_file" ps
