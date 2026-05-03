#!/bin/bash
# ABOUTME: Verifies Pkgly HTTP access logs include client and user-agent fields.
# ABOUTME: Sends a real request and inspects the shared rolling file log volume.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

print_section "HTTP Access Logs"

wait_for_server 60

CLIENT_ADDRESS="198.51.100.77, 203.0.113.88"
USER_AGENT_VALUE="pkgly-access-log-test/1.0"
REQUEST_PATH="/api/info"
LOG_ROOT="${PKGLY_LOG_ROOT:-/pkgly-storage}"

find_access_log_entry() {
    local log_files=()
    mapfile -t log_files < <(find "$LOG_ROOT" -type f -name "pkgly.log*" 2>/dev/null)

    if [ "${#log_files[@]}" -eq 0 ]; then
        return 1
    fi

    grep -h -F "HTTP access" "${log_files[@]}" \
        | grep -F "client.address" \
        | grep -F "$CLIENT_ADDRESS" \
        | grep -F "user_agent.original" \
        | grep -F "$USER_AGENT_VALUE" \
        | tail -n 1
}

print_test "request succeeds with forwarded headers"
status=$(curl -sS -o /dev/null -w "%{http_code}" \
    -H "X-Forwarded-For: ${CLIENT_ADDRESS}" \
    -H "User-Agent: ${USER_AGENT_VALUE}" \
    "${PKGLY_URL}${REQUEST_PATH}")

if assert_http_status "200" "$status"; then
    pass
else
    fail "Expected ${REQUEST_PATH} to return 200, got ${status}"
fi

print_test "file access log includes client address and user agent"
entry=""
for _ in {1..30}; do
    if entry=$(find_access_log_entry); then
        break
    fi
    sleep 1
done

if [ -n "$entry" ]; then
    pass
else
    record_output "$(find "$LOG_ROOT" -maxdepth 2 -type f -name "pkgly.log*" -print 2>/dev/null)"
    fail "Expected access log entry with client.address=${CLIENT_ADDRESS} and user_agent.original=${USER_AGENT_VALUE}"
fi

print_summary
