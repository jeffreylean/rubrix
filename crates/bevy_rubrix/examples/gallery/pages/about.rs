use bevy::ui::{AlignItems, percent, px};
use bevy_fynix::ViewExt;
use bevy_fynix::tokens::Tone;
use bevy_fynix::views::{FrameProps, column, label, row, scroll};

use crate::PageView;

const LABELS: usize = 200;
const ROW_HEIGHT: f32 = 24.0;

pub fn build() -> PageView {
    let rows = (0..LABELS)
        .map(|index| {
            row((label(text(index)).wrap(false),))
                .height(px(ROW_HEIGHT))
                .shrink(0.0)
                .align(AlignItems::Center)
                .boxed()
        })
        .collect::<Vec<_>>();
    column((
        label("About").size(20.0),
        label("The rubrix gallery shows one page per feature.")
            .tone(Tone::Dim),
        scroll(rows).gap(0.0).grow(1.0),
    ))
    .height(percent(100.0))
    .min_height(px(0.0))
    .gap(8.0)
    .boxed()
}

fn text(index: usize) -> String {
    if index == 1 {
        format!("Label {index:03}  héllo wörld · 日本語 · 😀")
    } else {
        format!("Label {index:03}")
    }
}
