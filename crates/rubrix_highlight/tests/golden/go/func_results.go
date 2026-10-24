package main

var f = func(x int) Option[int] { return nil }

type H func(int) List[string]

type G func(a, b int) (Pair[int, string], error)

func (s *Stack[T]) Push(v T) {}

func (s Stack) Len() int { return 0 }

func Map[T, U any](xs []T) []U { return nil }

func use() {
	g := func(n int) Result[int] { return Result[int]{} }
	_ = g
}
