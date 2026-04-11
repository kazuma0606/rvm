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

# 2. Hello World（トップレベル文を直接実行）
cat > /tmp/hello.fg <<'EOF'
println("Hello, ForgeScript!")
EOF
check "hello world" "forge run /tmp/hello.fg" "Hello, ForgeScript!"

# 3. forge build（cargo が存在する場合のみ実行）
cat > /tmp/build_test.fg <<'EOF'
let x = 42
println(x)
EOF
if command -v cargo > /dev/null 2>&1; then
    echo "  [INFO] forge build を実行中（初回は数分かかる場合があります）..."
    if timeout 300 forge build /tmp/build_test.fg -o /tmp/forge_out > /tmp/build_out.txt 2>&1; then
        check "build" "/tmp/forge_out" "42"
    else
        build_exit=$?
        if [ "$build_exit" -eq 124 ]; then
            echo "  [SKIP] build (timeout after 300s — cargo compile cache なし)"
        else
            echo "  [FAIL] build (exit $build_exit)"
            cat /tmp/build_out.txt | tail -5
            FAIL=$((FAIL + 1))
        fi
    fi
else
    echo "  [SKIP] build (cargo not installed — pre-built binary environment)"
fi

# 4. HTTP（インターネット到達可能時のみ）
if curl -sf --max-time 3 https://httpbin.org/get > /dev/null 2>&1; then
    cat > /tmp/http_test.fg <<'EOF'
use forge/http.{ get }
let res = get("https://httpbin.org/get").send()
println(res.status)
println(res.ok)
EOF
    check "http get" "forge run /tmp/http_test.fg" "200"
else
    echo "  [SKIP] http get (no internet access)"
fi

# 5. forge mcp start / status / stop
check "mcp start"  "forge mcp start"  "起動"
check "mcp status" "forge mcp status" "running"
check "mcp stop"   "forge mcp stop"   "停止"
check "mcp status after stop" "forge mcp status" "not running"

echo ""
echo "=== result: ${PASS} passed, ${FAIL} failed ==="

[ "$FAIL" -eq 0 ]
