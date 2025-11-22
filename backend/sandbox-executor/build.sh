#!/bin/bash
set -e

echo "🔨 Building Sandbox Executor..."

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}Step 1: Building runtime container image...${NC}"
docker build -t sandbox-runtime:latest -f Dockerfile.runtime .

if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ Runtime image built successfully${NC}"
else
    echo -e "${RED}✗ Failed to build runtime image${NC}"
    exit 1
fi

echo -e "${BLUE}Step 2: Building sandbox executor service...${NC}"
docker build -t sandbox-executor:latest -f Dockerfile .

if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ Sandbox executor built successfully${NC}"
else
    echo -e "${RED}✗ Failed to build sandbox executor${NC}"
    exit 1
fi

echo -e "${GREEN}🎉 Build completed successfully!${NC}"
echo ""
echo "To run the service:"
echo "  docker-compose up -d"
echo ""
echo "To test the service:"
echo "  ./test.sh"

