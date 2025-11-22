#!/bin/bash

# Test script for sandbox executor
BASE_URL="${SANDBOX_URL:-http://localhost:8090}"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

echo "🧪 Testing Sandbox Executor at $BASE_URL"
echo ""

# Test 1: Health check
echo -e "${BLUE}Test 1: Health Check${NC}"
response=$(curl -s "$BASE_URL/api/v1/health")
if echo "$response" | jq -e '.status == "healthy"' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Health check passed${NC}"
else
    echo -e "${RED}✗ Health check failed${NC}"
    echo "$response"
    exit 1
fi
echo ""

# Test 2: Get config
echo -e "${BLUE}Test 2: Get Configuration${NC}"
response=$(curl -s "$BASE_URL/api/v1/config")
if echo "$response" | jq -e '.supported_languages' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Config retrieval passed${NC}"
    echo "Supported languages: $(echo "$response" | jq -r '.supported_languages | join(", ")')"
else
    echo -e "${RED}✗ Config retrieval failed${NC}"
    echo "$response"
fi
echo ""

# Test 3: Python execution
echo -e "${BLUE}Test 3: Python Code Execution${NC}"
response=$(curl -s -X POST "$BASE_URL/api/v1/execute" \
    -H "Content-Type: application/json" \
    -d '{
        "code": "print(\"Hello from Python!\")\nprint(2 + 2)",
        "language": "python",
        "timeout": 30
    }')

if echo "$response" | jq -e '.status == "success"' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Python execution passed${NC}"
    echo "Execution ID: $(echo "$response" | jq -r '.execution_id')"
    echo "Output: $(echo "$response" | jq -r '.stdout')"
    echo "Time: $(echo "$response" | jq -r '.execution_time_ms')ms"
else
    echo -e "${RED}✗ Python execution failed${NC}"
    echo "$response"
fi
echo ""

# Test 4: JavaScript execution
echo -e "${BLUE}Test 4: JavaScript Code Execution${NC}"
response=$(curl -s -X POST "$BASE_URL/api/v1/execute" \
    -H "Content-Type: application/json" \
    -d '{
        "code": "console.log(\"Hello from Node.js!\");\nconsole.log(3 + 3);",
        "language": "javascript",
        "timeout": 30
    }')

if echo "$response" | jq -e '.status == "success"' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ JavaScript execution passed${NC}"
    echo "Output: $(echo "$response" | jq -r '.stdout')"
else
    echo -e "${RED}✗ JavaScript execution failed${NC}"
    echo "$response"
fi
echo ""

# Test 5: Shell execution
echo -e "${BLUE}Test 5: Shell Script Execution${NC}"
response=$(curl -s -X POST "$BASE_URL/api/v1/execute" \
    -H "Content-Type: application/json" \
    -d '{
        "code": "echo \"Hello from Shell!\"\ndate\nuname -a",
        "language": "shell",
        "timeout": 30
    }')

if echo "$response" | jq -e '.status == "success"' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Shell execution passed${NC}"
    echo "Output: $(echo "$response" | jq -r '.stdout' | head -1)"
else
    echo -e "${RED}✗ Shell execution failed${NC}"
    echo "$response"
fi
echo ""

# Test 6: Error handling (syntax error)
echo -e "${BLUE}Test 6: Error Handling${NC}"
response=$(curl -s -X POST "$BASE_URL/api/v1/execute" \
    -H "Content-Type: application/json" \
    -d '{
        "code": "print(invalid syntax here",
        "language": "python",
        "timeout": 30
    }')

if echo "$response" | jq -e '.stderr' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Error handling passed${NC}"
    echo "Error captured in stderr"
else
    echo -e "${RED}✗ Error handling failed${NC}"
    echo "$response"
fi
echo ""

# Test 7: Get stats
echo -e "${BLUE}Test 7: Get Statistics${NC}"
response=$(curl -s "$BASE_URL/api/v1/stats")
if echo "$response" | jq -e '.total_executions' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Stats retrieval passed${NC}"
    echo "Total executions: $(echo "$response" | jq -r '.total_executions')"
    echo "Success rate: $(echo "$response" | jq -r '.success_rate')%"
else
    echo -e "${RED}✗ Stats retrieval failed${NC}"
    echo "$response"
fi
echo ""

echo -e "${GREEN}🎉 All tests completed!${NC}"

