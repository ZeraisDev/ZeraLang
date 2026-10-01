#!/usr/bin/env bash
# AOT parity check.
#
# The tree-walking VM is the reference implementation, so every fixture in
# tests/aot/ has to print byte-identical output under `--vm` and under the AOT
# (compile to .o, link against the Rust runtime, run).
#
# A fixture can override the reference with `//! expect:` lines for the cases
# where the VM itself is the broken backend — those are reported as
# "vm-better/aot-only" style results instead of being silently skipped.
#
# Usage: scripts/aot_parity.sh [name-filter]

set -uo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$root/target/release/zera_lang"
RUNTIME="$root/target/release/libzera_lang.a"
FILTER="${1:-}"

if [[ ! -x "$BIN" || ! -f "$RUNTIME" ]]; then
  echo "Building release binaries..."
  (cd "$root" && cargo build --release) || exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# The C-FFI fixture needs a shared library; @FFI_LIB@ is substituted into its
# source below so the test does not hardcode a machine-specific path.
if ! cc -dynamiclib "$root/tests/aot/ffi_helper.c" -o "$work/libffi_helper.dylib" 2>"$work/ffi_build.log"; then
  echo "warning: could not build the C-FFI helper; 26_ffi will fail:"
  sed 's/^/  /' "$work/ffi_build.log"
fi

pass=0; fail=0; skip=0
declare -a failing=()

# Compile, link and run one fixture through the AOT. stdout+stderr of the run is
# the result; a build failure is reported as such rather than as empty output.
aot_run() {
  local src="$1" dir="$work/$(basename "$src" .zera)"
  mkdir -p "$dir"
  if [[ "$src" == *@FFI_LIB@* ]] || grep -q '@FFI_LIB@' "$src"; then
    local subst="$dir/$(basename "$src")"
    sed "s|@FFI_LIB@|$work/libffi_helper.dylib|" "$src" >"$subst"
    src="$subst"
  fi
  # --aot writes zeralang_output.o into the working directory, so run from $dir.
  if ! (cd "$dir" && "$BIN" --aot "$src" >build.log 2>&1); then
    echo "COMPILE-FAIL: $(grep -v '^AOT: ' "$dir/build.log" | head -2 | tr '\n' ' ')"
    return
  fi
  if ! (cd "$dir" && cc zeralang_output.o "$RUNTIME" -o prog >>build.log 2>&1); then
    echo "LINK-FAIL: $(grep -v 'platform load command' "$dir/build.log" | tail -2 | tr '\n' ' ')"
    return
  fi
  # A Zera program's exit status is its top-level `return` value, so a non-zero
  # status is normal. Only a signal or a runtime panic counts as a failure.
  "$dir/prog" >"$dir/run.log" 2>&1
  rc=$?
  if (( rc >= 128 )) || grep -q 'panicked\|fatal runtime error' "$dir/run.log"; then
    echo "RUN-FAIL (rc=$rc): $(tail -3 "$dir/run.log" | tr '\n' '|')"
    return
  fi
  cat "$dir/run.log"
}

for src in "$root"/tests/aot/*.zera; do
  name="$(basename "$src" .zera)"
  [[ -n "$FILTER" && "$name" != *"$FILTER"* ]] && { skip=$((skip+1)); continue; }

  expected_vm="$("$BIN" --vm "$src" 2>&1)"
  got_aot="$(aot_run "$src")"

  # `//! expect:` blocks override the VM oracle.
  if grep -q '^//! expect: ' "$src"; then
    want="$(sed -n 's,^//! expect: ,,p' "$src")"
    ref="$want"
    mode="aot-only"
  else
    ref="$expected_vm"
    mode="vm"
  fi

  if [[ "$got_aot" == "$ref" ]]; then
    pass=$((pass+1))
    printf '  ok   %-24s (%s)\n' "$name" "$mode"
  else
    fail=$((fail+1)); failing+=("$name")
    printf '  FAIL %-24s (%s)\n' "$name" "$mode"
    printf '       expected: %s\n' "$(echo "$ref" | head -4 | tr '\n' '|')"
    printf '       aot:      %s\n' "$(echo "$got_aot" | head -4 | tr '\n' '|')"
  fi
done

echo "-----------------------------------------"
echo "AOT parity: pass=$pass fail=$fail"
[[ ${#failing[@]} -gt 0 ]] && echo "failing: ${failing[*]}"
[[ $fail -eq 0 ]]
