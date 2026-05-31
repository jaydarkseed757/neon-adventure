mod world;
mod player;
mod parser;
mod commands;
mod npcs;
mod mobs;
mod title;
mod ambient;
mod save;
mod ui;
mod gfx;
mod input;
mod net;
mod net_daemons;
mod app;

use macroquad::prelude::*;

fn window_conf() -> Conf {
    Conf {
        window_title: "Neon Descent".to_string(),
        window_width:  1000,
        window_height: 680,
        window_resizable: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Neon Descent v{}", env!("CARGO_PKG_VERSION"));
        println!("by J. Collins");
        println!();
        println!("Usage: neon_descent");
        std::process::exit(0);
    }

    let mut app = app::App::new().await;

    loop {
        app.frame().await;
        // Cap title screen at ~30fps: sleep past one VSync period so the next
        // next_frame() catches every other tick rather than every one.
        if matches!(app.state, app::AppState::Title) {
            std::thread::sleep(std::time::Duration::from_millis(18));
        }
        next_frame().await;
    }
}
