fn f<'a>(x: &'a str) -> char { 'a' }
let c = '\n'; let q = '\''; let u = '\u{1F600}';
let e = '🎉'; let b = b'x';
fn g<'long_name, 'b: 'long_name>() {}
'outer: loop { break 'outer; }
let s: &'static str = "x";
