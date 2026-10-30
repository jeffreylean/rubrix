#![allow(
    dead_code,
    reason = "each test binary uses its own part of this module"
)]

pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, n: u32) -> u32 {
        (self.next() % u64::from(n)) as u32
    }

    pub fn chance(&mut self, percent: u32) -> bool {
        self.below(100) < percent
    }
}

fn parse_seed(var: Option<&str>) -> u64 {
    var.map_or(1, |seed| {
        seed.parse().unwrap_or_else(|_| {
            panic!(
                "RUBRIX_SEED must be an unsigned integer, got \
                 {seed:?}"
            )
        })
    })
}

pub fn seed() -> u64 {
    let var = std::env::var_os("RUBRIX_SEED");
    parse_seed(
        var.as_ref().map(|seed| seed.to_string_lossy()).as_deref(),
    )
}

#[test]
fn seed_defaults_to_one() {
    assert_eq!(parse_seed(None), 1);
}

#[test]
fn seed_reads_the_variable() {
    assert_eq!(parse_seed(Some("7")), 7);
}

#[test]
#[should_panic(expected = "RUBRIX_SEED")]
fn unparsable_seed_fails() {
    parse_seed(Some("seven"));
}
