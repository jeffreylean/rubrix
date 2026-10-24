package main

import "fmt"

var query = `
func main() {
	// not a comment
}
`

var oneLine = `raw \n stays "raw"`

var joined = `first
second` + `third`

var empty = ``

const sql = `SELECT *
  FROM t
 WHERE a = 'b'`

func main() {
	fmt.Println(`inline`, "interpreted", `after`)
}
