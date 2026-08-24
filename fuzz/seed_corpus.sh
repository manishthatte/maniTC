#!/usr/bin/env bash
# Build the fuzzing seed corpus from real maniT source.  (F-8)
#
# © Manish Jagdish Thatte
#
# libFuzzer mutates what it is given. Starting from noise, it spends its whole
# budget rediscovering that `fn` is a keyword; starting from the shipped
# examples and the standard library, its first mutations are already valid
# programs and it gets to the semantics in seconds. Regenerating is cheap and
# idempotent, so run it whenever the sources move.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(dirname "$here")"

for target in fuzz_lex fuzz_parse fuzz_analyze fuzz_pipeline; do
    dir="$here/corpus/$target"
    mkdir -p "$dir"
    n=0
    for src in "$root"/examples/*.mt "$root"/stdlib/*.mt; do
        [ -e "$src" ] || continue
        cp -f "$src" "$dir/$(basename "$src")"
        n=$((n + 1))
    done
    echo "$target: $n seeds in $dir"
done
