package main

var a = "unterminated string
var b = 'x
var c = '
var d = ''
var e = 'ab'
var f = "bad escape \
var g = 1
var h = `raw never closes
func inside() {
	// still raw
}
