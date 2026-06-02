#!/usr/bin/env bash
set -euo pipefail

script_name="${0##*/}"

require_env() {
  local name="${1:?env name required}"
  local value="${!name:-}"
  if [[ -z "$value" ]]; then
    printf '%s: %s is required\n' "$script_name" "$name" >&2
    exit 2
  fi
  printf '%s\n' "$value"
}

expand_path() {
  case "$1" in
    "~") printf '%s\n' "$HOME" ;;
    "~/"*) printf '%s/%s\n' "$HOME" "${1#~/}" ;;
    /*) printf '%s\n' "$1" ;;
    *)
      printf '%s: path must be absolute or start with ~, got %s\n' "$script_name" "$1" >&2
      exit 2
      ;;
  esac
}

validate_loopback_host() {
  local value="${1:?host required}"
  local name="${2:?host name required}"
  case "$value" in
    127.0.0.1|localhost) ;;
    *)
      printf '%s: %s must stay on loopback, got %s\n' "$script_name" "$name" "$value" >&2
      exit 2
      ;;
  esac
}

validate_port() {
  local value="${1:?port required}"
  local name="${2:?port name required}"
  if [[ ! "$value" =~ ^[0-9]+$ ]] || (( value < 1 || value > 65535 )); then
    printf '%s: %s must be a TCP port, got %s\n' "$script_name" "$name" "$value" >&2
    exit 2
  fi
}

ssh_target="$(require_env JAILGUN_SSH_TARGET)"
identity_file="$(expand_path "$(require_env JAILGUN_SSH_IDENTITY_FILE)")"
known_hosts="$(expand_path "${JAILGUN_SSH_KNOWN_HOSTS:-$HOME/.ssh/known_hosts}")"
ssh_port="${JAILGUN_SSH_PORT:-22}"
local_host="${JAILGUN_LOCAL_CDP_HOST:-127.0.0.1}"
local_port="${JAILGUN_LOCAL_CDP_PORT:-9224}"
remote_host="${JAILGUN_REMOTE_CDP_HOST:-127.0.0.1}"
remote_port="${JAILGUN_REMOTE_CDP_PORT:-9224}"

validate_loopback_host "$local_host" "JAILGUN_LOCAL_CDP_HOST"
validate_loopback_host "$remote_host" "JAILGUN_REMOTE_CDP_HOST"
validate_port "$ssh_port" "JAILGUN_SSH_PORT"
validate_port "$local_port" "JAILGUN_LOCAL_CDP_PORT"
validate_port "$remote_port" "JAILGUN_REMOTE_CDP_PORT"

if [[ ! -r "$identity_file" ]]; then
  printf '%s: SSH identity file is not readable: %s\n' "$script_name" "$identity_file" >&2
  exit 2
fi

if [[ ! -r "$known_hosts" ]]; then
  printf '%s: SSH known_hosts file is not readable: %s\n' "$script_name" "$known_hosts" >&2
  exit 2
fi

exec /usr/bin/ssh -NT \
  -o LogLevel=ERROR \
  -o BatchMode=yes \
  -o IdentitiesOnly=yes \
  -o StrictHostKeyChecking=yes \
  -o UserKnownHostsFile="$known_hosts" \
  -o ExitOnForwardFailure=yes \
  -o ServerAliveInterval=15 \
  -o ServerAliveCountMax=3 \
  -o ConnectTimeout=10 \
  -i "$identity_file" \
  -p "$ssh_port" \
  -L "$local_host:$local_port:$remote_host:$remote_port" \
  "$ssh_target"
