package main

import "fmt"

// 注释 with ünïcode
var 变量 = "日本語のテキスト"

type Ünïcode struct{ Ünï int }

func 函数(参数 int) int {
	return 参数 + 1
}

func main() {
	s := "😀 emoji 🎉"
	r := '世'
	fmt.Println(s, r, `多行
字符串`)
}
