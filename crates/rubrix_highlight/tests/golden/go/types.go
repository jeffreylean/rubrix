package main

type Celsius float64

type Alias = string

type (
	Point struct{ X, Y int }
	Shape interface {
		Area() float64
		fmt.Stringer
	}
)

type Number interface {
	~int | ~int64 | ~float64
}

type Pair[K comparable, V any] struct {
	Key K
	Val V
}

type Handler func(w http.ResponseWriter, r *http.Request)

var (
	i   int
	i8  int8
	u   uint
	u64 uint64
	up  uintptr
	f32 float32
	c   complex128
	by  byte
	ru  rune
	s   string
	e   error
	b   bool
	a   any
)

func switchType(x interface{}) {
	switch v := x.(type) {
	case int:
		_ = v
	case string, []byte:
	default:
	}
}
