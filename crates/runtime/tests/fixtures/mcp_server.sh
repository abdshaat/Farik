# A stdio MCP server in `sh`, one JSON-RPC message per line, for Farik's tests. Its tools are
# `search`, which answers `fixture-found: <its arguments>`; `env`, whose description, and whose
# answer when called, is what the server sees of its environment and its working folder;
# `delete_repo`; and `repo.delete`, a name Claude Code would rewrite. A notification (no id) is
# read and not answered.
seen() {
  printf 'PWD=%s HOME=%s API_KEY=%s ANTHROPIC_API_KEY=%s CLAUDE_CODE_OAUTH_TOKEN=%s' "$(pwd)" "${HOME:+set}" "${API_KEY-}" "${ANTHROPIC_API_KEY-}" "${CLAUDE_CODE_OAUTH_TOKEN-}"
}
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"initialize"'*)
      version=$(printf '%s' "$line" | sed -n 's/.*"protocolVersion":"\([^"]*\)".*/\1/p')
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"%s","capabilities":{"tools":{}},"serverInfo":{"name":"fixture","version":"1"}}}\n' "$id" "$version"
      ;;
    *'"method":"tools/list"'*)
      seen=$(seen)
      schema='{"type":"object","properties":{"query":{"type":"string"}}}'
      printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"search","description":"Searches.","inputSchema":%s},{"name":"env","description":"%s","inputSchema":%s},{"name":"delete_repo","description":"Deletes the repository.","inputSchema":%s},{"name":"repo.delete","description":"Deletes a repository.","inputSchema":%s}]}}\n' "$id" "$schema" "$seen" "$schema" "$schema" "$schema"
      ;;
    *'"method":"tools/call"'*)
      case "$line" in
        *'"name":"env"'*)
          printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"%s"}],"isError":false}}\n' "$id" "$(seen)"
          ;;
        *)
          arguments=$(printf '%s' "$line" | sed -n 's/.*"arguments":\({[^}]*}\).*/\1/p' | sed 's/["\\]//g')
          printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"fixture-found: %s"}],"isError":false}}\n' "$id" "$arguments"
          ;;
      esac
      ;;
    *'"method":"'*)
      if [ -n "$id" ]; then
        printf '{"jsonrpc":"2.0","id":%s,"result":{}}\n' "$id"
      fi
      ;;
  esac
done
