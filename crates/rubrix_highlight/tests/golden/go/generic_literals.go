package main

func empty() Option[int] {
	return Option[int]{}
}

func pair() {
	p := Pair[string, []int]{Key: "a"}
	q := Wrapper[Option[int]]{}
	r := &Node[int]{Next: nil}
	for _, v := range Items[1:] {
		_ = Table[v] + Offsets[v]
	}
	if Flags[0] {
		_ = p
	}
	_, _, _ = q, r, Matrix[0][1]
}
