mod app;
mod card;
mod constants;
mod deck;
mod renderer;

use app::Playmat;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Cardmat",
        options,
        Box::new(|_cc| Box::new(Playmat::default())),
    )
}
