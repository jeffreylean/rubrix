// Package doc comment.
package main

/* single line */

/*
Multi-line
block comment with `backticks` and "quotes"
*/

var x = 1 /* inline */ + 2

/* a */ /* b */ var y = 3

/**
 * Doc style
 */
func f() {
	/* nested /* is not nested */
	_ = 1 // line comment /* not a block
}

/* ends */ var z = 4 /* opens
and continues
*/ var w = 5
