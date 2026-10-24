cat <<A <<'B' > out
first $x
A
second $x
B
cat <<"E O" | wc -l
body
E O
cat <<\END
$y
END
cat <<<"here string" # not a heredoc
echo $((1 << 2)) $(( x<<3 ))
x=$(cat <<EOF
inside $(echo nested)
EOF
)
cat <<EOF
EOFX
not the end
EOF
