use asr::{async_main, future::next_tick};

pub mod autosplitter;
pub mod game;

use autosplitter::AutoSplitter;
use game::{GameAutoSplitter, risk_of_rain, risk_of_rain_2, risk_of_rain_returns};

async_main!(stable);

async fn main() {
    let mut autosplitter = AutoSplitter::new();

    let mut ror1 = risk_of_rain::Game::new();
    let mut ror2 = risk_of_rain_2::Game::new();
    let mut rorr = risk_of_rain_returns::Game::new();

    loop {
        if let Some(process) = ror1.attach_any() {
            process.until_closes(ror1.attached(&process, &mut autosplitter)).await;
        } else if let Some(process) = ror2.attach_any() {
            process.until_closes(ror2.attached(&process, &mut autosplitter)).await;
        } else if let Some(process) = rorr.attach_any() {
            process.until_closes(rorr.attached(&process, &mut autosplitter)).await;
        } else {
            autosplitter.update_loop(None);
        }
        next_tick().await;
    }
}

