package main

type T struct { A int `json:"a"`; B *Node }

type Node struct {
	Next *Node
	Val  int
	Tags map[Key][]Item
	pkg.Qualified
}

var Exported = 1

var Global Config

func use(x T, ch chan Event, rest ...Arg) {
	_ = x.A
	_ = x.Next.Val
	y := Exported
	c := Config{Name: "n"}
	m := map[Key]Value{}
	var s []Item
	var p *Point
	p2 := &Point{}
	fmt.Println(Exported, MaxSize, y, c, m, s, p, p2)
	_ = obj.(Shape)
	z := a * Scale
	check(Exported, Config)
	p3 := new(Point)
}
