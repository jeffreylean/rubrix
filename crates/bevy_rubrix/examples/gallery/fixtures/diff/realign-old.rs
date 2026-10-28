fn main() {
    let started = Instant::now();
    let total = prices.iter().sum::<u32>();
    println!("took {:?}", started.elapsed());
    report(total);
}
