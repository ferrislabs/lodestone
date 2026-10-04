#!/bin/sh

set -eu

HTML_DIR=/usr/share/nginx/html
TEMPLATE=/usr/local/share/oxid/config.json
CONFIG_FILE="$HTML_DIR/config.json"

# Only the runtime configuration is rewritten. The assets are placed at build
# time: this process runs unprivileged and has no business rewriting the tree it
# serves.
#
# The served file is written by redirection rather than with `cp` or `sed -i`.
# Both of those create or unlink inside the directory, which is root-owned; a
# redirection truncates the existing file in place and needs only write
# permission on the file itself, which the runtime user has.
#
# Reading from the pristine template keeps every start substituting from
# placeholders rather than from the previous start's values.
escape() {
  printf '%s' "$1" | sed -e 's/[\/&|]/\\&/g'
}

if [ -f "$TEMPLATE" ] && [ -f "$CONFIG_FILE" ]; then
  # shellcheck disable=SC2016
  sed \
    -e "s|\${API_URL}|$(escape "${API_URL:-}")|g" \
    -e "s|\${ISSUER_URL}|$(escape "${ISSUER_URL:-}")|g" \
    -e "s|\${OIDC_CLIENT_ID}|$(escape "${OIDC_CLIENT_ID:-}")|g" \
    -e "s|\${OIDC_SCOPE}|$(escape "${OIDC_SCOPE:-openid profile email}")|g" \
    "$TEMPLATE" > "$CONFIG_FILE"
fi

exec "$@"
