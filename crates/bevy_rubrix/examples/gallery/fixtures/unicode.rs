// Tabs, CJK and an emoji 😀 in one file.
fn main() {
	let greeting = "日本語のテキスト";
	let face = "😀";	// a tab before this comment
	if greeting.len() > 3 {
		println!("{greeting} {face}");
	}
}
