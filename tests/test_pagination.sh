#!/bin/bash
# Test script for MCP pagination functionality

MCP_BIN="./target/release/mcp-lore"

echo "=== Testing MCP Pagination ==="
echo

# Test with a single session sending multiple requests
echo "Testing pagination with page=0 and page=1..."
echo

(
  echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}'
  echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["Search"],"maxResults":2,"page":0}}}'
  echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["Search"],"maxResults":2,"page":1}}}'
) | $MCP_BIN 2>&1 | while IFS= read -r line; do
  if echo "$line" | grep -q '"id":2'; then
    echo "=== Page 0 (first 2 results) ==="
    echo "$line" | jq -r '.result.content[0].text' | jq '{totalCount, hasMore, matchCount: (.matches | length), firstMatch: .matches[0].path}'
    echo
  elif echo "$line" | grep -q '"id":3'; then
    echo "=== Page 1 (next 2 results) ==="
    echo "$line" | jq -r '.result.content[0].text' | jq '{totalCount, hasMore, matchCount: (.matches | length), firstMatch: .matches[0].path}'
    echo
  fi
done

echo "=== Test Complete ==="