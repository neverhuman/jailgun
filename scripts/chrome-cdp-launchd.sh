#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tunnel_script="$script_dir/chrome-cdp-tunnel.sh"
script_name="${0##*/}"
label="${JAILGUN_LAUNCHD_LABEL:-com.jailgun.chrome-cdp-tunnel}"
plist_dir="${HOME}/Library/LaunchAgents"
plist_path="${JAILGUN_LAUNCHD_PLIST:-$plist_dir/$label.plist}"
log_dir="${JAILGUN_LAUNCHD_LOG_DIR:-${HOME}/Library/Logs/Jailgun}"
uid="$(id -u)"

xml_escape() {
  local value="${1:-}"
  value="${value//&/&amp;}"
  value="${value//</&lt;}"
  value="${value//>/&gt;}"
  value="${value//\"/&quot;}"
  printf '%s\n' "$value"
}

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

validate_readable_file() {
  local path="${1:?path required}"
  local name="${2:?name required}"
  if [[ ! -r "$path" ]]; then
    printf '%s: %s must be readable, got %s\n' "$script_name" "$name" "$path" >&2
    exit 2
  fi
}

install_plist() {
  local ssh_target
  local identity_file
  local known_hosts

  ssh_target="$(require_env JAILGUN_SSH_TARGET)"
  identity_file="$(expand_path "$(require_env JAILGUN_SSH_IDENTITY_FILE)")"
  known_hosts="$(expand_path "${JAILGUN_SSH_KNOWN_HOSTS:-$HOME/.ssh/known_hosts}")"

  validate_readable_file "$identity_file" "JAILGUN_SSH_IDENTITY_FILE"
  validate_readable_file "$known_hosts" "JAILGUN_SSH_KNOWN_HOSTS"

  mkdir -p "$plist_dir" "$log_dir"

  local tmp_plist
  tmp_plist="$(mktemp "${plist_path}.XXXXXX")"

  cat >"$tmp_plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>$(xml_escape "$label")</string>
  <key>ProgramArguments</key>
  <array>
    <string>/bin/bash</string>
    <string>$(xml_escape "$tunnel_script")</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>JAILGUN_SSH_TARGET</key>
    <string>$(xml_escape "$ssh_target")</string>
    <key>JAILGUN_SSH_IDENTITY_FILE</key>
    <string>$(xml_escape "$identity_file")</string>
    <key>JAILGUN_SSH_KNOWN_HOSTS</key>
    <string>$(xml_escape "$known_hosts")</string>
    <key>JAILGUN_SSH_PORT</key>
    <string>$(xml_escape "${JAILGUN_SSH_PORT:-22}")</string>
    <key>JAILGUN_LOCAL_CDP_HOST</key>
    <string>$(xml_escape "${JAILGUN_LOCAL_CDP_HOST:-127.0.0.1}")</string>
    <key>JAILGUN_LOCAL_CDP_PORT</key>
    <string>$(xml_escape "${JAILGUN_LOCAL_CDP_PORT:-9224}")</string>
    <key>JAILGUN_REMOTE_CDP_HOST</key>
    <string>$(xml_escape "${JAILGUN_REMOTE_CDP_HOST:-127.0.0.1}")</string>
    <key>JAILGUN_REMOTE_CDP_PORT</key>
    <string>$(xml_escape "${JAILGUN_REMOTE_CDP_PORT:-9224}")</string>
  </dict>
  <key>KeepAlive</key>
  <true/>
  <key>RunAtLoad</key>
  <true/>
  <key>ThrottleInterval</key>
  <integer>10</integer>
  <key>StandardOutPath</key>
  <string>$(xml_escape "$log_dir/chrome-cdp-tunnel.out.log")</string>
  <key>StandardErrorPath</key>
  <string>$(xml_escape "$log_dir/chrome-cdp-tunnel.err.log")</string>
</dict>
</plist>
EOF

  mv "$tmp_plist" "$plist_path"
}

case "${1:-install}" in
  install)
    install_plist
    launchctl bootout "gui/$uid" "$plist_path" >/dev/null 2>&1 || true
    launchctl bootstrap "gui/$uid" "$plist_path"
    printf '%s: loaded %s\n' "$script_name" "$plist_path"
    ;;
  uninstall)
    launchctl bootout "gui/$uid" "$plist_path" >/dev/null 2>&1 || true
    rm -f "$plist_path"
    printf '%s: removed %s\n' "$script_name" "$plist_path"
    ;;
  *)
    printf 'usage: %s [install|uninstall]\n' "$script_name" >&2
    exit 2
    ;;
esac
