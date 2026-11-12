//! Minimal ferrocord plugin, written with `ferrocord-sdk`.
//!
//! Build with:
//!
//! ```sh
//! cargo build --release --target wasm32-unknown-unknown
//! cp target/wasm32-unknown-unknown/release/hello_plugin.wasm plugin.wasm
//! ```

use ferrocord_sdk::{anyhow, export_plugin, log, Context, Event, Plugin};

struct Hello {
    events: u32,
}

impl Plugin for Hello {
    fn init(_cx: &mut Context) -> anyhow::Result<Self> {
        log::info!("hello from a Rust plugin!");
        Ok(Self { events: 0 })
    }

    fn on_event(&mut self, _cx: &mut Context, event: Event) {
        self.events += 1;
        match event {
            Event::Ready { client_version } => {
                log::info!("got event `ready` (client {client_version:?})");
            }
            Event::MessageCreate { author_id, content, .. } => {
                log::info!("message from {author_id}: {content}");
            }
            other => log::debug!("event #{}: {other:?}", self.events),
        }
    }
}

export_plugin!(Hello);
