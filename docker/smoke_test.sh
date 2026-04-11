#!/usr/bin/env bash
set -euo pipefail

PASS=0
FAIL=0

check() {
    local name="$1"
    local cmd="$2"
    local expect="$3"
    local actual
    actual=$(eval "$cmd" 2>&1) || true
    if echo "$actual" | grep -q "$expect"; then
        echo "  [PASS] $name"
        PASS=$((PASS + 1))
    else
        echo "  [FAIL] $name"
        echo "         expected: '$expect'"
        echo "         got:      '$actual'"
        FAIL=$((FAIL + 1))
    fi
}

echo "=== forge smoke test ==="
echo ""

# 1. バージョン確認
check "version" "forge --version" "forge"

# 2. Hello World
cat > /tmp/hello.fg <<'EOF'
fn main() {
    println("Hello, ForgeScript!")
}
EOF
check "hello world" "forge run /tmp/hello.fg" "Hello, ForgeScript!"

# 3. forge build
cat > /tmp/build_test.fg <<'EOF'
fn main() {
    let x = 42
    println(x)
}
EOF
forge build /tmp/build_test.fg -o /tmp/forge_out 2>/dev/null
check "build" "/tmp/forge_out" "42"

# 4. HTTP（インターネット到達可能時のみ）
if curl -sf --max-time 3 https://httpbin.org/get > /dev/null 2>&1; then
    cat > /tmp/http_test.fg <<'EOF'
use forge/http.{ get }
fn main() {
    let res = get("https://httpbin.org/get").send()
    println(res.status)
    println(res.ok)
}
EOF
    check "http get" "forge run /tmp/http_test.fg" "200"
else
    echo "  [SKIP] http get (no internet access)"
fi

echo ""
echo "=== result: ${PASS} passed, ${FAIL} failed ==="

[ "$FAIL" -eq 0 ]
