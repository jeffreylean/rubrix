package main

import (
	"fmt"
	str "strings"
)

func main() {
	fmt.Println(len("a"), cap(s))
}

func add(a, b int) int { return a + b }

func (r *Receiver) Method(x int) (int, error) {
	return r.field, nil
}

func (Receiver) NoName() {}

func Generic[T any, K comparable](m map[K]T) []T {
	out := make([]T, 0, len(m))
	for _, v := range m {
		out = append(out, v)
	}
	return out
}

func (l *List[T]) Push(v T) {}

var anon = func(a int) string { return str.Repeat("x", a) }

var fn func(int) (string, error)

func variadic(args ...interface{}) {
	defer func() { recover() }()
	go worker(args...)
	panic(fmt.Sprintf("%v", args))
}

func result() (n int, err error) {
	f := func() int { return 1 }
	n = f()
	return
}
