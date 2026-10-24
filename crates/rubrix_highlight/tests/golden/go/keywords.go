package main

import (
	"fmt"
	"os"
)

const Pi = 3.14
const MAX_SIZE = 10

var ErrNotFound = errors.New("not found")

func main() {
	var x = nil
	ok := true && !false
	for i := 0; i < 10; i++ {
		if i%2 == 0 {
			continue
		} else {
			break
		}
	}
	switch x {
	case 1:
		fallthrough
	default:
	}
	select {
	case v := <-ch:
		_ = v
	case ch <- 1:
	}
	for range ch {
	}
	goto end
end:
	ch := make(chan int, 1)
	m := map[string][]int{"a": {1, 2}}
	delete(m, "a")
	n := new(int)
	_, _ = n, ok
	defer fmt.Println(iota)
	os.Exit(1)
}
