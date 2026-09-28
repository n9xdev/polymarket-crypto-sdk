#!/usr/bin/env bash
# Print the first TCP port in [start, end] that is not listening locally.
set -euo pipefail
start="${1:-3000}"
end="${2:-3099}"

port_in_use() {
  local p="$1"
  if command -v ss >/dev/null 2>&1; then
    ss -tln 2>/dev/null | grep -qE ":${p}([[:space:]]|$)"
    return $?
  fi
  (echo >/dev/tcp/127.0.0.1/"$p") 2>/dev/null
}

for ((p = start; p <= end; p++)); do
  if ! port_in_use "$p"; then
    echo "$p"
    exit 0
  fi
done

echo "no free port in ${start}-${end}" >&2
exit 1
