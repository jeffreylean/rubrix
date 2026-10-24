package main

type (
	Alias = int
	Other string
	Box[T any] struct {
		Value T
		Count int
	}
	Handler func(int) error
	Reader  interface {
		Read(p []byte) (int, error)
	}
)

type After int

var (
	Name string
)
