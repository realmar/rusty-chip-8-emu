#![feature(const_int_pow)]
#![cfg_attr(test, feature(proc_macro_hygiene))]

mod emulator;
mod runner;
mod vm;

use flexi_logger::{LogSpecBuilder, Logger};
use log::{error, info, LevelFilter};

use ggez::event;
use ggez::ContextBuilder;

use emulator::Emulator;
use vm::config::Config;
use vm::constants::*;

fn main() {
    let config = Config::load().unwrap();
    let log_init_result = Logger::with(
        LogSpecBuilder::new()
            .default(config.log_level)
            .module("gfx_device_gl", LevelFilter::Warn)
            .module("ggez", LevelFilter::Warn)
            .build(),
    )
    .start();

    if let Err(err) = log_init_result {
        println!("ERROR initializing logger: {}", err);
    }

    let screen_scaling = config.screen_scaling;
    let mut window_mode = ggez::conf::WindowMode::default();
    window_mode.width = SCREEN_SIZE_X as f32 * screen_scaling;
    window_mode.height = SCREEN_SIZE_Y as f32 * screen_scaling;

    let mut window_setup = ggez::conf::WindowSetup::default();
    window_setup.title = String::from("CHIP8 Emulator");

    let (mut ctx, mut event_loop) = ContextBuilder::new("rusty-chip8-emu", "Anastassios Martakos")
        .window_mode(window_mode)
        .window_setup(window_setup)
        .build()
        .expect("Failed to create engine context");

    let mut emulator = Emulator::new(&mut ctx, config);

    match event::run(&mut ctx, &mut event_loop, &mut emulator) {
        Ok(_) => info!("Engine shutdown"),
        Err(e) => error!("ERROR in engine loop: {}", e),
    }
}
