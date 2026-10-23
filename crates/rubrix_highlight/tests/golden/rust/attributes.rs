#[derive(Debug)] struct A; println!("{x}");
#![no_std]
#[cfg(all(
    feature = "std", // why ] here
    not(test)
))]
#[doc = "a ] b"]
#[rustfmt::skip] fn f() {}
