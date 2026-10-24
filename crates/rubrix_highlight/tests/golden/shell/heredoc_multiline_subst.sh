cat <<EOF
$(echo
hi)
EOF
case $a in
  x) echo one ;;
esac
echo after
cat <<-EOF
	`echo
	tick`
	${v:-
	x}
	EOF
echo done
cat <<'EOF'
$(echo
hi)
EOF
cat <<"EOF"
$(echo
hi)
EOF
cat <<\EOF
$(echo
hi)
EOF
if true; then echo end; fi
cat <<A <<B
$(echo
a)
A
$(echo
b)
B
echo last
