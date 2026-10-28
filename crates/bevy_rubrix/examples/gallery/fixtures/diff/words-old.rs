fn total(prices: &[u32]) -> u32 {
    let mut sum = 0;
    for price in prices {
        sum += price;
    }
    sum
}
