#!/usr/bin/env bash
cat <<EOF
if then $x
plain ${y:-z} and $(date) here
a \$escaped `cmd` done
EOF
echo after $x
