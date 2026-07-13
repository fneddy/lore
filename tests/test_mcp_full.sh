#!/bin/bash
# Comprehensive MCP functionality test script

# set -e  # Commented out to see all test results

MCP_BIN="./target/release/mcp-lore"
PASSED=0
FAILED=0

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "=========================================="
echo "  MCP Lore Comprehensive Test Suite"
echo "=========================================="
echo

# Helper function to send MCP request and check response
test_tool() {
    local test_name="$1"
    local request="$2"
    local expected_pattern="$3"
    
    echo -n "Testing: $test_name... "
    
    # Send request and capture response
    local response=$(echo "$request" | $MCP_BIN 2>&1 | grep -v "^$" | tail -1)
    
    # Check if response matches expected pattern
    if echo "$response" | grep -q "$expected_pattern"; then
        echo -e "${GREEN}✓ PASSED${NC}"
        ((PASSED++))
        return 0
    else
        echo -e "${RED}✗ FAILED${NC}"
        echo "  Expected pattern: $expected_pattern"
        echo "  Got: ${response:0:200}..."
        ((FAILED++))
        return 1
    fi
}

# Test 1: Initialize
echo "=== Test 1: Initialization ==="
INIT_REQUEST='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}'
test_tool "Initialize MCP server" "$INIT_REQUEST" '"protocolVersion":"2024-11-05"'
echo

# Test 2: Query-project with defaults
echo "=== Test 2: Query-project (default parameters) ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["Search"]}}}'
) | $MCP_BIN 2>&1 | grep '"id":2' > /tmp/test_query_default.json
if jq -e '.result.content[0].text | fromjson | .totalCount > 0 and .hasMore and (.matches | length) == 100' /tmp/test_query_default.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Default query returns 100 results with pagination"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Default query response invalid"
    ((FAILED++))
fi
echo

# Test 3: Query-project with pagination
echo "=== Test 3: Query-project (pagination) ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["async"],"maxResults":10,"page":0}}}'
  echo '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["async"],"maxResults":10,"page":1}}}'
) | $MCP_BIN 2>&1 > /tmp/test_pagination.txt

# Check page 0
if grep '"id":3' /tmp/test_pagination.txt | jq -e '.result.content[0].text | fromjson | (.matches | length) == 10' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Page 0 returns 10 results"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Page 0 response invalid"
    ((FAILED++))
fi

# Check page 1
if grep '"id":4' /tmp/test_pagination.txt | jq -e '.result.content[0].text | fromjson | (.matches | length) == 10' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Page 1 returns 10 results"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Page 1 response invalid"
    ((FAILED++))
fi
echo

# Test 4: Query-system
echo "=== Test 4: Query-system ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"query-system","arguments":{"query":["socket"],"maxResults":5}}}'
) | $MCP_BIN 2>&1 | grep '"id":5' > /tmp/test_query_system.json
if jq -e '.result.content[0].text | fromjson | .totalCount >= 0 and (.matches | length) <= 5' /tmp/test_query_system.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Query-system returns valid results"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Query-system response invalid"
    ((FAILED++))
fi
echo

# Test 5: List-modules
echo "=== Test 5: List-modules ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"list-modules","arguments":{}}}'
) | $MCP_BIN 2>&1 | grep '"id":6' > /tmp/test_list_modules.json
if jq -e '.result.content[0].text | fromjson | type == "array"' /tmp/test_list_modules.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - List-modules returns array"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - List-modules response invalid"
    ((FAILED++))
fi
echo

# Test 6: Show (metadata only)
echo "=== Test 6: Show (metadata only) ==="
TEST_FILE="target/doc/lore/index.html"
if [ -f "$TEST_FILE" ]; then
    (
      echo "$INIT_REQUEST"
      echo "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"tools/call\",\"params\":{\"name\":\"show\",\"arguments\":{\"path\":\"$TEST_FILE\",\"metadataOnly\":true}}}"
    ) | $MCP_BIN 2>&1 | grep '"id":7' > /tmp/test_show_metadata.json
    if jq -e '.result.content[0].text | fromjson | .sizeBytes > 0' /tmp/test_show_metadata.json > /dev/null 2>&1; then
        echo -e "${GREEN}✓ PASSED${NC} - Show metadata returns file size"
        ((PASSED++))
    else
        echo -e "${RED}✗ FAILED${NC} - Show metadata response invalid"
        ((FAILED++))
    fi
else
    echo -e "${YELLOW}⊘ SKIPPED${NC} - Test file not found: $TEST_FILE"
fi
echo

# Test 7: Show (content extraction)
echo "=== Test 7: Show (content extraction) ==="
if [ -f "$TEST_FILE" ]; then
    (
      echo "$INIT_REQUEST"
      echo "{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"tools/call\",\"params\":{\"name\":\"show\",\"arguments\":{\"path\":\"$TEST_FILE\",\"start\":0,\"length\":1000}}}"
    ) | $MCP_BIN 2>&1 | grep '"id":8' > /tmp/test_show_content.json
    if jq -e '.result.content[0].text | fromjson | .content | length > 0' /tmp/test_show_content.json > /dev/null 2>&1; then
        echo -e "${GREEN}✓ PASSED${NC} - Show content extraction works"
        ((PASSED++))
    else
        echo -e "${RED}✗ FAILED${NC} - Show content response invalid"
        ((FAILED++))
    fi
else
    echo -e "${YELLOW}⊘ SKIPPED${NC} - Test file not found: $TEST_FILE"
fi
echo

# Test 8: Outline
echo "=== Test 8: Outline ==="
if [ -f "$TEST_FILE" ]; then
    (
      echo "$INIT_REQUEST"
      echo "{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"tools/call\",\"params\":{\"name\":\"outline\",\"arguments\":{\"path\":\"$TEST_FILE\"}}}"
    ) | $MCP_BIN 2>&1 | grep '"id":9' > /tmp/test_outline.json
    if jq -e '.result.content[0].text | fromjson | type == "array"' /tmp/test_outline.json > /dev/null 2>&1; then
        echo -e "${GREEN}✓ PASSED${NC} - Outline returns array of headings"
        ((PASSED++))
    else
        echo -e "${RED}✗ FAILED${NC} - Outline response invalid"
        ((FAILED++))
    fi
else
    echo -e "${YELLOW}⊘ SKIPPED${NC} - Test file not found: $TEST_FILE"
fi
echo

# Test 9: Query with filters
echo "=== Test 9: Query with filters ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["Search"],"maxResults":5,"excludeExtensions":["txt","md"]}}}'
) | $MCP_BIN 2>&1 | grep '"id":10' > /tmp/test_query_filters.json
if jq -e '.result.content[0].text | fromjson | .matches | length <= 5' /tmp/test_query_filters.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Query with excludeExtensions works"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Query with excludeExtensions response invalid"
    ((FAILED++))
fi
echo

# Test 10: Response structure validation
echo "=== Test 10: Response structure validation ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["async"],"maxResults":1}}}'
) | $MCP_BIN 2>&1 | grep '"id":11' > /tmp/test_structure.json

# Check that offsets field is NOT present (should be removed)
if jq -e '.result.content[0].text | fromjson | .matches[0] | has("offsets") | not' /tmp/test_structure.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Offsets field correctly removed from response"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Offsets field still present in response"
    ((FAILED++))
fi

# Check required fields are present
if jq -e '.result.content[0].text | fromjson | .matches[0] | has("path") and has("filesize") and has("matched_terms")' /tmp/test_structure.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Required fields present in match"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Required fields missing from match"
    ((FAILED++))
fi
echo

# Test 11: Query with excludePathPatterns
echo "=== Test 11: Query with excludePathPatterns ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["async"],"maxResults":5,"excludePathPatterns":["test","example"]}}}'
) | $MCP_BIN 2>&1 | grep '"id":12' > /tmp/test_exclude_paths.json
if jq -e '.result.content[0].text | fromjson | .matches | length <= 5' /tmp/test_exclude_paths.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Query with excludePathPatterns works"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Query with excludePathPatterns response invalid"
    ((FAILED++))
fi
echo

# Test 12: Query with excludeMatch
echo "=== Test 12: Query with excludeMatch ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":13,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["async"],"maxResults":5,"excludeMatch":["deprecated","internal"]}}}'
) | $MCP_BIN 2>&1 | grep '"id":13' > /tmp/test_exclude_match.json
if jq -e '.result.content[0].text | fromjson | .matches | length <= 5' /tmp/test_exclude_match.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Query with excludeMatch works"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Query with excludeMatch response invalid"
    ((FAILED++))
fi
echo

# Test 13: Query with includePath
echo "=== Test 13: Query with includePath ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":14,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["async"],"maxResults":5,"includePath":true}}}'
) | $MCP_BIN 2>&1 | grep '"id":14' > /tmp/test_include_path.json
if jq -e '.result.content[0].text | fromjson | .matches | length <= 5' /tmp/test_include_path.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Query with includePath works"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Query with includePath response invalid"
    ((FAILED++))
fi
echo

# Test 14: Query with context parameter
echo "=== Test 14: Query with context parameter ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":15,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["async"],"maxResults":5,"context":5}}}'
) | $MCP_BIN 2>&1 | grep '"id":15' > /tmp/test_context.json
if jq -e '.result.content[0].text | fromjson | .matches | length <= 5' /tmp/test_context.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Query with context parameter works"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Query with context parameter response invalid"
    ((FAILED++))
fi
echo

# Test 15: Query with custom paths
echo "=== Test 15: Query with custom paths ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":16,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["async"],"maxResults":5,"paths":["target/doc"]}}}'
) | $MCP_BIN 2>&1 | grep '"id":16' > /tmp/test_custom_paths.json
if jq -e '.result.content[0].text | fromjson | .matches | length <= 5' /tmp/test_custom_paths.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Query with custom paths works"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Query with custom paths response invalid"
    ((FAILED++))
fi
echo

# Test 16: Query with multiple terms (OR logic)
echo "=== Test 16: Query with multiple terms (OR logic) ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":17,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["async","tokio","futures"],"maxResults":10}}}'
) | $MCP_BIN 2>&1 | grep '"id":17' > /tmp/test_multiple_terms.json
if jq -e '.result.content[0].text | fromjson | .totalCount > 0 and (.matches | length) <= 10' /tmp/test_multiple_terms.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Query with multiple terms (OR logic) works"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Query with multiple terms response invalid"
    ((FAILED++))
fi
echo

# Test 17: Query with all parameters combined
echo "=== Test 17: Query with all parameters combined ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":18,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["async"],"maxResults":5,"page":0,"context":3,"includePath":true,"excludePathPatterns":["test"],"excludeMatch":["deprecated"],"excludeExtensions":["txt"]}}}'
) | $MCP_BIN 2>&1 | grep '"id":18' > /tmp/test_all_params.json
if jq -e '.result.content[0].text | fromjson | .matches | length <= 5' /tmp/test_all_params.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Query with all parameters combined works"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Query with all parameters combined response invalid"
    ((FAILED++))
fi
echo

# Test 18: Edge case - query as string instead of array (should return error)
echo "=== Test 18: Edge case - query as string instead of array ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":19,"method":"tools/call","params":{"name":"query-project","arguments":{"query":"async","maxResults":5}}}'
) | $MCP_BIN 2>&1 | grep '"id":19' > /tmp/test_query_string.json
if jq -e '.result.isError == true' /tmp/test_query_string.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Query as string correctly returns error (isError=true)"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Query as string should return error but didn't"
    ((FAILED++))
fi
echo

# Test 19: Edge case - empty query array (returns all files)
echo "=== Test 19: Edge case - empty query array ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":20,"method":"tools/call","params":{"name":"query-project","arguments":{"query":[],"maxResults":5}}}'
) | $MCP_BIN 2>&1 | grep '"id":20' > /tmp/test_empty_query.json
if jq -e '.result.content[0].text | fromjson | .totalCount > 0 and (.matches | length) <= 5' /tmp/test_empty_query.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Empty query array returns all files (no filter)"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Empty query array not handled correctly"
    ((FAILED++))
fi
echo

# Test 20: Edge case - maxResults = 0
echo "=== Test 20: Edge case - maxResults = 0 ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":21,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["async"],"maxResults":0}}}'
) | $MCP_BIN 2>&1 | grep '"id":21' > /tmp/test_max_zero.json
if jq -e '.result.content[0].text | fromjson | (.matches | length) == 0' /tmp/test_max_zero.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - maxResults=0 returns empty matches"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - maxResults=0 not handled correctly"
    ((FAILED++))
fi
echo

# Test 21: Edge case - very large page number
echo "=== Test 21: Edge case - very large page number ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":22,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["async"],"maxResults":10,"page":99999}}}'
) | $MCP_BIN 2>&1 | grep '"id":22' > /tmp/test_large_page.json
if jq -e '.result.content[0].text | fromjson | (.matches | length) == 0 and .hasMore == false' /tmp/test_large_page.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Large page number returns empty results with hasMore=false"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Large page number not handled correctly"
    ((FAILED++))
fi
echo

# Test 22: Validation - query term with spaces (should be rejected)
echo "=== Test 22: Validation - query term with spaces ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":23,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["async tokio"],"maxResults":5}}}'
) | $MCP_BIN 2>&1 | grep '"id":23' > /tmp/test_spaces_rejected.json
if jq -e '.result.content[0].text | test("contains spaces")' /tmp/test_spaces_rejected.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Query term with spaces correctly rejected"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Query term with spaces should be rejected"
    ((FAILED++))
fi
echo

# Test 23: Validation - query term with spaces allowed when flag set
echo "=== Test 23: Validation - query term with spaces (allowSpacesInTerms=true) ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":24,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["async tokio"],"maxResults":5,"allowSpacesInTerms":true}}}'
) | $MCP_BIN 2>&1 | grep '"id":24' > /tmp/test_spaces_allowed.json
if jq -e '.result.content[0].text | fromjson | .totalCount >= 0' /tmp/test_spaces_allowed.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Query term with spaces allowed when flag set"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Query term with spaces should be allowed with flag"
    ((FAILED++))
fi
echo

# Test 24: Validation - properly split terms work correctly
echo "=== Test 24: Validation - properly split terms ==="
(
  echo "$INIT_REQUEST"
  echo '{"jsonrpc":"2.0","id":25,"method":"tools/call","params":{"name":"query-project","arguments":{"query":["async","tokio"],"maxResults":5}}}'
) | $MCP_BIN 2>&1 | grep '"id":25' > /tmp/test_split_terms.json
if jq -e '.result.content[0].text | fromjson | .totalCount > 0 and (.matches[0].matched_terms | length) > 0' /tmp/test_split_terms.json > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC} - Properly split terms work correctly"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC} - Properly split terms should work"
    ((FAILED++))
fi
echo

# Summary
echo "=========================================="
echo "  Test Summary"
echo "=========================================="
echo -e "Total tests: $((PASSED + FAILED))"
echo -e "${GREEN}Passed: $PASSED${NC}"
echo -e "${RED}Failed: $FAILED${NC}"
echo

# Cleanup
rm -f /tmp/test_*.json /tmp/test_*.txt

# Exit with appropriate code
if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
fi