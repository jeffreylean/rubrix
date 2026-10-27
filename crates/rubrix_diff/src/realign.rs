use alloc::string::String;
use alloc::vec::Vec;

use crate::model::{Block, FileDiff, Lines};

/// Blocks that would need more line comparisons than this keep their
/// positional pairing.
const MAX_COMPARISONS: u64 = 4096;

/// How much a shifted pairing must beat the positional one, per
/// paired line, before a block is re-split. Near ties keep the
/// diff's own order.
const MIN_GAIN_PER_PAIR: f64 = 0.5;

/// Re-splits each change block whose sides differ in length so that
/// the most similar old and new lines pair, as Pierre's
/// `realignChangeContent.ts` does. The shorter side slides along the
/// longer one; the surplus lines become pure deletions or additions
/// at the block's edges.
///
/// Changes stay inside their block, so stacked rows and every
/// layout's row count stay the same; split rows and word ranges
/// follow the new pairs. Lines are compared without whitespace, by
/// their shared prefix and suffix over the longer length.
pub fn realign(diff: &mut FileDiff) {
    diff.split_changes(|old, new, block, parts| {
        realign_block(old, new, block, parts);
    })
    .expect("realign's parts cover their block");
}

fn realign_block(
    old: &Lines,
    new: &Lines,
    block: Block,
    parts: &mut Vec<(u32, u32)>,
) {
    let (old_lines, new_lines) =
        (block.old_range(), block.new_range());
    let (old_len, new_len) =
        (old_lines.len() as u32, new_lines.len() as u32);
    let pairs = old_len.min(new_len);
    let surplus = old_len.abs_diff(new_len);
    if pairs == 0
        || surplus == 0
        || u64::from(pairs) * u64::from(surplus + 1) > MAX_COMPARISONS
    {
        return;
    }
    let strip = |lines: &Lines, range: core::ops::Range<u32>| {
        range
            .map(|i| {
                lines.line(i).split_whitespace().collect::<String>()
            })
            .collect::<Vec<_>>()
    };
    let (old_text, new_text) =
        (strip(old, old_lines), strip(new, new_lines));
    let old_longer = old_len > new_len;
    let score = |offset: u32| {
        (0..pairs)
            .map(|i| {
                let (o, n) = if old_longer {
                    (i + offset, i)
                } else {
                    (i, i + offset)
                };
                similarity(
                    &old_text[o as usize],
                    &new_text[n as usize],
                )
            })
            .sum::<f64>()
    };
    let mut best =
        (0, score(0) + f64::from(pairs) * MIN_GAIN_PER_PAIR);
    for offset in 1..=surplus {
        let shifted = score(offset);
        if shifted > best.1 {
            best = (offset, shifted);
        }
    }
    let offset = best.0;
    if offset == 0 {
        return;
    }
    let rest = surplus - offset;
    let edge =
        |len: u32| if old_longer { (len, 0) } else { (0, len) };
    parts.extend(
        [edge(offset), (pairs, pairs), edge(rest)]
            .into_iter()
            .filter(|&part| part != (0, 0)),
    );
}

/// 0 to 1: shared prefix plus shared suffix over the longer length.
/// 1 for equal lines, 0 when one is empty and the other is not.
fn similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    (prefix + suffix) as f64 / a.len().max(b.len()) as f64
}
