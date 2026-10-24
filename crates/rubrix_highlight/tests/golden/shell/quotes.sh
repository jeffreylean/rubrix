echo 'single' "double" 'it'\''s' $'ansi\n\'q'
echo "multi
line $x string" tail
echo 'multi
line' tail
echo "escape \" \$ \\ \n done"
echo $'ansi
multi' end
echo one \
  two \
  three
echo a\ b \$c
echo "unterminated
$(echo
