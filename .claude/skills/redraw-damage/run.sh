#!/usr/bin/env bash
# Model-checks RedrawDamage.tla with TLC. With no arguments it runs every
# check: damage, then rows. Name configs to run only those, for example
# `run.sh damage`. Exits nonzero when any check fails.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
version=v1.7.4
sha256=936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88
cache=${XDG_CACHE_HOME:-$HOME/.cache}
jar=$cache/tla2tools/$version/tla2tools.jar
out=$cache/redraw-damage

if [ ! -f "$jar" ]; then
    mkdir -p "$(dirname "$jar")"
    curl -fsSLo "$jar.part" \
        "https://github.com/tlaplus/tlaplus/releases/download/$version/tla2tools.jar"
    mv "$jar.part" "$jar"
fi
echo "$sha256  $jar" | sha256sum --check --quiet

configs=("$@")
[ ${#configs[@]} -eq 0 ] && configs=(damage rows)

failed=0
for config in "${configs[@]}"; do
    dir=$out/$config
    rm -rf "$dir"
    mkdir -p "$dir"
    log=$dir/tlc.log
    start=$(date +%s)
    (cd "$dir" && java -Xmx4g -XX:+UseParallelGC \
        -Djava.io.tmpdir="$dir" \
        -jar "$jar" -workers 4 -metadir "$dir/states" \
        -config "$here/$config.cfg" "$here/RedrawDamage.tla") \
        >"$log" 2>&1 || true
    took=$(($(date +%s) - start))
    states=$(grep -oE '[0-9,]+ distinct states found' "$log" | tail -1 || true)
    if grep -q '^Model checking completed. No error has been found.' "$log"; then
        echo "PASS $config in ${took}s, $states"
    else
        failed=1
        echo "FAIL $config in ${took}s, $states. Trace in $log:"
        grep -E '^Error|is violated|^Assertion' "$log" || tail -20 "$log"
    fi
done
exit $failed
