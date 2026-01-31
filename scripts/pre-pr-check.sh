#!/bin/bash
# Pre-PR checklist - Run this before creating a pull request
# This ensures CI will pass and prevents wasted time

set -e

echo "🔍 Pre-PR Checklist"
echo "===================="
echo ""

# Color codes
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

FAILED=0

# Step 1: Check formatting
echo "1️⃣  Checking code formatting (cargo fmt)..."
if cargo fmt --all -- --check; then
    echo -e "${GREEN}✅ Formatting check passed${NC}"
else
    echo -e "${RED}❌ Formatting check failed${NC}"
    echo "   Run: cargo fmt --all"
    FAILED=1
fi
echo ""

# Step 2: Run Clippy
echo "2️⃣  Running Clippy linter..."
if cargo clippy --all-targets --all-features -- -D warnings; then
    echo -e "${GREEN}✅ Clippy passed${NC}"
else
    echo -e "${RED}❌ Clippy failed${NC}"
    echo "   Fix the warnings above"
    FAILED=1
fi
echo ""

# Step 3: Run local tests (no database required)
echo "3️⃣  Running local tests (no database)..."
if cargo test --lib && cargo test --test fetcher_integration_test; then
    echo -e "${GREEN}✅ Local tests passed${NC}"
else
    echo -e "${RED}❌ Local tests failed${NC}"
    FAILED=1
fi
echo ""

# Step 4: Build check
echo "4️⃣  Checking build..."
if cargo build --release; then
    echo -e "${GREEN}✅ Build successful${NC}"
else
    echo -e "${RED}❌ Build failed${NC}"
    FAILED=1
fi
echo ""

# Summary
echo "===================="
if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}✅ All pre-PR checks passed!${NC}"
    echo ""
    echo "You're ready to create a PR:"
    echo "  git push"
    echo "  gh pr create"
    echo ""
else
    echo -e "${RED}❌ Some checks failed. Fix the issues above before creating a PR.${NC}"
    exit 1
fi
