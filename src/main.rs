#![feature(const_int_pow)]
#![cfg_attr(test, feature(proc_macro_hygiene))]

mod errors;
mod emulator;
mod runner;
mod vm;

use flexi_logger::{LogSpecBuilder, Logger};
use log::{error, info, LevelFilter};

use ggez::{
    conf::{WindowMode, WindowSetup},
    event::{self, EventHandler},
    graphics::{self, Text, Align},
    Context, ContextBuilder, GameResult,
};
use winit::EventsLoop;

use emulator::Emulator;
use vm::config::Config;
use vm::constants::*;

struct ErrorWindow {
    message: String,
}

impl ErrorWindow {
    const WIDTH: f32 = 800.;
    const HEIGHT: f32 = 680.;

    fn new(message: String) -> ErrorWindow {
        ErrorWindow { message }
    }
}

impl EventHandler for ErrorWindow {
    fn update(&mut self, _: &mut ggez::Context) -> GameResult<()> {
        Ok(())
    }

    fn draw(&mut self, ctx: &mut ggez::Context) -> GameResult<()> {
        let mut text = Text::new(format!(
            "ERROR on application start:\n\n{}",
            self.message.as_str()
        ));
        text.set_bounds([ErrorWindow::WIDTH, ErrorWindow::HEIGHT], Align::Left);

        graphics::clear(ctx, graphics::BLACK);
        graphics::draw(ctx, &text, graphics::DrawParam::new())?;

        graphics::present(ctx)?;

        Ok(())
    }
}

fn main() {
    match Config::load() {
        Ok(config) => {
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

            let (mut ctx, event_loop) = create_context(
                {
                    let screen_scaling = config.screen_scaling;
                    let mut mode = WindowMode::default();
                    mode.width = SCREEN_SIZE_X as f32 * screen_scaling;
                    mode.height = SCREEN_SIZE_Y as f32 * screen_scaling;

                    mode
                },
                {
                    let mut setup = WindowSetup::default();
                    setup.title = String::from("CHIP8 Emulator");

                    setup
                },
            );

            match Emulator::new(&mut ctx, config) {
                Ok(emulator) => run(ctx, event_loop, emulator),
                Err(err) => {
                    run(ctx, event_loop, ErrorWindow::new(format!("{}", err)));
                }
            };
        }
        Err(msg) => run_error_window(format!("Cannot load config: {}", msg)),
    }
}

fn run_error_window(message: String) {
    let (ctx, event_loop) = create_context(
        {
            let mut mode = WindowMode::default();
            mode.width = ErrorWindow::WIDTH;
            mode.height = ErrorWindow::HEIGHT;

            mode
        },
        {
            let mut setup = WindowSetup::default();
            setup.title = String::from("Error");

            setup
        },
    );

    run(ctx, event_loop, ErrorWindow::new(message));
}

fn create_context(window_mode: WindowMode, window_setup: WindowSetup) -> (Context, EventsLoop) {
    ContextBuilder::new("rusty-chip8-emu", "Anastassios Martakos")
        .window_mode(window_mode)
        .window_setup(window_setup)
        .build()
        .expect("Failed to create engine context")
}

fn run<TEngine: EventHandler>(mut ctx: Context, mut event_loop: EventsLoop, mut engine: TEngine) {
    match event::run(&mut ctx, &mut event_loop, &mut engine) {
        Ok(_) => info!("Engine shutdown"),
        Err(e) => error!("ERROR in engine loop: {}", e),
    };
}
