echo $HOME $1 $@ $* $# $? $$ $! ${HOME} ${1} ${10}
echo ${var:-x} ${var:=y} ${var:+z} ${var:?err} ${var#pre} ${var##pre} ${var%suf} ${var%%suf}
echo ${var/a/b} ${var//a/b} ${var^^} ${var,,} ${!ref} ${#var}
echo ${arr[0]} ${arr[@]} ${arr[*]} ${#arr[@]} ${arr[$i]:-d}
echo ${var:-${other:-"deep $x"}} "${var:-a b}"
echo "price: $5" "cost $" $ lone
name=value other+=more "q=$x" arr=(a b $c)
declare -a list=(
  one
  two
)
