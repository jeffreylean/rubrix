ls > out 2>&1 < in >> log &> all
cmd >&2 2>/dev/null |& tee x
diff <(sort a) <(sort b) >(gzip)
a && b || c; d & e | f
case x in a) one ;;& b) two ;& esac
exec 3<&- 4>&-
