use std::path::PathBuf;
use std::sync::Arc;

use bevy::app::{App, First};
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Res, ResMut};
use bevy::ecs::world::World;
use bevy::ui::{percent, px};
use bevy_fynix::tokens::Tone;
use bevy_fynix::views::{FrameProps, column, label, row};
use bevy_fynix::{ViewExt, resource};
use bevy_rubrix::{
    CodeDoc, CodeLine, CodeSource, CodeStream, Lang, code_view,
};

use super::code::{LARGE_RS, read, stress};
use super::{PAGES, control, wrapping};
use crate::{PageView, Selected};

const RATES: [(usize, &str); 4] = [
    (1, "1 B"),
    (64, "64 B"),
    (4 << 10, "4 KiB"),
    (64 << 10, "64 KiB"),
];

const FIRST_RATE: usize = 64;

/// The file the page streams, and how far it has sent it.
#[derive(Resource)]
struct Streaming {
    name: String,
    text: Arc<str>,
    sent: usize,
    rate: usize,
    paused: bool,
    stream: CodeStream,
    source: CodeSource,
}

impl Streaming {
    fn new(name: &str, text: Arc<str>, rate: usize) -> Streaming {
        let first = text.lines().next().unwrap_or_default();
        let stream = CodeSource::streaming(Lang::detect(name, first));
        Streaming {
            name: name.to_owned(),
            text,
            sent: 0,
            rate,
            paused: false,
            source: stream.source(),
            stream,
        }
    }

    /// Starts again from the first byte, at the same rate, and
    /// paused if it was.
    fn restart(&mut self) {
        let paused = self.paused;
        *self =
            Streaming::new(&self.name, self.text.clone(), self.rate);
        self.paused = paused;
    }

    fn rate_name(&self) -> &'static str {
        RATES
            .iter()
            .find(|(rate, _)| *rate == self.rate)
            .map_or("?", |(_, name)| name)
    }
}

impl Default for Streaming {
    fn default() -> Self {
        Streaming::new("large.rs.txt", LARGE_RS.into(), FIRST_RATE)
    }
}

/// Row text roots spawned, for the `stream` log topic.
#[derive(Resource, Default)]
struct Respawned {
    spawned: usize,
    logged: usize,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Streaming>()
        .init_resource::<Respawned>()
        .add_observer(
            |_: On<Add<CodeLine>>,
             mut respawned: ResMut<Respawned>| {
                respawned.spawned += 1;
            },
        )
        .add_systems(First, send);
}

/// Sends the next chunk while the page shows.
fn send(selected: Res<Selected>, mut streaming: ResMut<Streaming>) {
    let shown = PAGES[selected.0].name == "stream";
    let left = streaming.text.len() - streaming.sent;
    if !shown || streaming.paused || left == 0 {
        return;
    }
    let streaming = &mut *streaming;
    let mut end = streaming.sent + streaming.rate.min(left);
    while !streaming.text.is_char_boundary(end) {
        end += 1;
    }
    streaming.stream.push(&streaming.text[streaming.sent..end]);
    streaming.sent = end;
    if end == streaming.text.len() {
        streaming.stream.finish();
    }
}

pub fn build() -> PageView {
    column((
        label("Stream").size(20.0),
        controls(),
        label(resource::<Streaming, _>(|streaming| {
            format!(
                "{}, {} of {} bytes at {} a frame",
                streaming.name,
                streaming.sent,
                streaming.text.len(),
                streaming.rate_name()
            )
        }))
        .tone(Tone::Dim),
        code_view(resource::<Streaming, _>(|streaming| {
            streaming.source.clone()
        })),
    ))
    .height(percent(100.0))
    .min_height(px(0.0))
    .gap(8.0)
    .boxed()
}

fn controls() -> PageView {
    let rates = RATES.map(|(rate, name)| {
        control(name, move |world| {
            world.resource_mut::<Streaming>().rate = rate;
        })
    });
    let [one, sixty_four, four_k, sixty_four_k] = rates;
    let buttons = row((
        one,
        sixty_four,
        four_k,
        sixty_four_k,
        control(
            resource::<Streaming, _>(|streaming| {
                if streaming.paused { "resume" } else { "pause" }
                    .to_owned()
            }),
            |world| {
                let mut streaming = world.resource_mut::<Streaming>();
                streaming.paused = !streaming.paused;
            },
        ),
        control("restart", |world| {
            world.resource_mut::<Streaming>().restart();
        }),
        control("100k lines", |world| {
            let rate = world.resource::<Streaming>().rate;
            world.insert_resource(Streaming::new(
                "large.rs.txt repeated",
                stress().into(),
                rate,
            ));
        }),
    ))
    .gap(8.0);
    wrapping(buttons.boxed())
}

pub fn open(
    world: &mut World,
    paths: &[PathBuf],
) -> Result<(), String> {
    let (name, text) = read("stream", paths)?;
    let rate = world.resource::<Streaming>().rate;
    world.insert_resource(Streaming::new(&name, text.into(), rate));
    Ok(())
}

/// `stream committed <n> provisional "<line>" respawned <n> sent
/// <bytes>/<bytes>`: the first code view's committed lines and its
/// unfinished last line, or `provisional none`, the row text roots
/// spawned since the last `stream` log, and the bytes sent.
pub fn log(world: &mut World) -> String {
    let doc = world.query::<&CodeDoc>().iter(world).next();
    let (committed, provisional) = doc.map_or((0, None), |doc| {
        let committed = doc.committed();
        (committed, doc.line(committed).map(str::to_owned))
    });
    let provisional = provisional.map_or_else(
        || "none".to_owned(),
        |line| format!("{line:?}"),
    );
    let streaming = world.resource::<Streaming>();
    let sent = format!("{}/{}", streaming.sent, streaming.text.len());
    let mut respawned = world.resource_mut::<Respawned>();
    let since = respawned.spawned - respawned.logged;
    respawned.logged = respawned.spawned;
    format!(
        "stream committed {committed} provisional {provisional} \
         respawned {since} sent {sent}"
    )
}
