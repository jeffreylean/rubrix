package main

var (
	a = 'x'
	b = '\n'
	c = '\''
	d = '\\'
	e = '\x41'
	f = '\u00e9'
	g = '\U0001F600'
	h = '\101'
	i = '世'
	j = '😀'
	k = '"'
	l = "'"
)

func runes() []rune {
	return []rune{'a', 'b', '\t'}
}
