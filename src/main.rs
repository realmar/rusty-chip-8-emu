#![feature(const_int_pow)]
#![cfg_attr(test, feature(proc_macro_hygiene))]

mod runner;
mod vm;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use flexi_logger::{Logger, LogSpecBuilder};
use log::{LevelFilter, info, error};

use ggez::audio;
use ggez::audio::SoundSource;
use ggez::event::{self, EventHandler};
use ggez::input::keyboard;
use ggez::input::keyboard::{KeyCode, KeyMods};
use ggez::{graphics, Context, ContextBuilder, GameResult};

use runner::Runner;
use vm::config::Config;
use vm::constants::*;
use vm::input::Input;
use vm::DebuggerCommand;

const SCREEN_SCALING: f32 = 20.;

fn main() {
    let config = Config::load().unwrap();
    let log_init_result = Logger::with(
        LogSpecBuilder::new()
            .default(config.log_level)
            .module("gfx_device_gl", LevelFilter::Warn)
            .module("ggez", LevelFilter::Warn)
            .build())
        .start();

    if let Err(err) = log_init_result {
        println!("ERROR initializing logger: {}", err);
    }

    let mut window_mode = ggez::conf::WindowMode::default();
    window_mode.width = SCREEN_SIZE_X as f32 * SCREEN_SCALING;
    window_mode.height = SCREEN_SIZE_Y as f32 * SCREEN_SCALING;

    let mut window_setup = ggez::conf::WindowSetup::default();
    window_setup.title = String::from("CHIP8 Emulator");

    let (mut ctx, mut event_loop) = ContextBuilder::new("rusty-chip8-emu", "Anastassios Martakos")
        .window_mode(window_mode)
        .window_setup(window_setup)
        .build()
        .expect("Failed to create engine context");

    let mut emulator = Emulator::new(&mut ctx, config);

    match event::run(&mut ctx, &mut event_loop, &mut emulator) {
        Ok(_) => info!("Engine shutdown."),
        Err(e) => error!("ERROR in engine loop: {}", e),
    }
}

struct GGEZInput {
    pressed_keys: Vec<u8>,
    mapping: HashMap<KeyCode, u8>,
}

impl GGEZInput {
    fn new(config: &Config) -> GGEZInput {
        let mut mapping = match config.get_rom_keymapping() {
            Some(x) => x,
            None => &config.default_key_mapping,
        }
        .clone();

        for kvp in Config::get_default_key_mapping() {
            if mapping.contains_key(&kvp.0) == false {
                mapping.insert(kvp.0, kvp.1);
            }
        }

        GGEZInput {
            pressed_keys: Vec::with_capacity(16),
            mapping: mapping,
        }
    }

    fn map_keycode(&self, code: &KeyCode) -> Option<u8> {
        match self.mapping.get(code) {
            Some(key) => Some(*key),
            None => None,
        }
    }

    fn update_keys(&mut self, keys: &HashSet<KeyCode>) {
        self.pressed_keys.clear();
        for code in keys.iter() {
            if let Some(key) = self.map_keycode(code) {
                self.pressed_keys.push(key);
            }
        }
    }
}

impl Input for GGEZInput {
    fn is_pressed(&self, key: u8) -> bool {
        self.pressed_keys.contains(&key)
    }

    fn get_pressed_key(&self) -> Option<u8> {
        if self.pressed_keys.len() == 0 {
            None
        } else {
            Some(self.pressed_keys[0])
        }
    }
}

struct Emulator {
    config: Config,
    runner: Runner,
    input: Arc<Mutex<GGEZInput>>,
    beep: audio::Source,
}

impl Emulator {
    pub fn new(_ctx: &mut Context, config: Config) -> Emulator {
        let sound_bytes = match vm::audio::sample(config.beep_frequency) {
            Ok(waveform) => waveform,
            Err(msg) => panic!("ERROR while generating sound waveform: {}", msg),
        };
        let (input, runner) = Emulator::create_runner(&config);
        let beep =
            audio::Source::from_data(_ctx, audio::SoundData::from_bytes(sound_bytes.as_slice()))
                .unwrap();

        Emulator {
            config,
            input,
            runner,
            beep,
        }
    }

    fn create_runner(config: &Config) -> (Arc<Mutex<GGEZInput>>, Runner) {
        let input = Arc::new(Mutex::new(GGEZInput::new(&config)));
        let runner = match Runner::new(&config, input.clone()) {
            Ok(runner) => runner,
            Err(err) => panic!("ERROR creating VM: {}", err),
        };

        (input, runner)
    }

    fn reset(&mut self) {
        let config = Config::load().unwrap();
        let (input, runner) = Emulator::create_runner(&config);

        self.config = config;
        self.input = input;
        self.runner = runner;
    }
}

impl EventHandler for Emulator {
    fn update(&mut self, _ctx: &mut Context) -> GameResult<()> {
        let pressed_keys = keyboard::pressed_keys(&_ctx);

        if self.config.debugger.enable {
            if (keyboard::active_mods(_ctx) & KeyMods::SHIFT) == KeyMods::SHIFT {
                if pressed_keys.contains(&self.config.debugger.key_mapping.step_previous) {
                    self.runner.send_debugger_command(DebuggerCommand::Previous);
                } else if pressed_keys.contains(&self.config.debugger.key_mapping.step_next) {
                    self.runner.send_debugger_command(DebuggerCommand::Next);
                }
            }
        }

        {
            let mut input = self.input.lock().unwrap();
            input.update_keys(pressed_keys);
        }

        if self.runner.is_playing_sound() && self.beep.playing() == false {
            if let Err(msg) = self.beep.play() {
                error!("ERROR playing sound: {}", msg);
            }
        } else if self.beep.playing() && self.runner.is_playing_sound() == false {
            self.beep.stop();
        }

        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
        let mut has_items = false;
        let mut builder = graphics::MeshBuilder::new();

        for y in 0..SCREEN_SIZE_Y {
            for x in 0..SCREEN_SIZE_X {
                let mut curr_pixel = 0;
                let pixel_byte = self.runner.get_pixel(x, y);

                for n in 0..8 {
                    let mask = 1 << n;
                    let is_set = pixel_byte & mask > 0;

                    if is_set {
                        builder.rectangle(
                            graphics::DrawMode::fill(),
                            graphics::Rect::new(
                                (x + curr_pixel) as f32 * SCREEN_SCALING,
                                y as f32 * SCREEN_SCALING,
                                SCREEN_SCALING,
                                SCREEN_SCALING,
                            ),
                            graphics::WHITE,
                        );

                        has_items = true;
                    }

                    curr_pixel += 1;
                }
            }
        }

        graphics::clear(ctx, graphics::BLACK);

        if has_items {
            let result = builder.build(ctx)?;
            graphics::draw(ctx, &result, graphics::DrawParam::new())?;
        }

        graphics::present(ctx)
    }

    fn key_up_event(&mut self, _ctx: &mut Context, _keycode: KeyCode, _keymods: KeyMods) {
        let no_shift = (_keymods & KeyMods::SHIFT) != KeyMods::SHIFT;

        if _keycode == self.config.general_key_mapping.restart_vm {
            self.reset()
        }

        if self.config.debugger.enable {
            if _keycode == self.config.debugger.key_mapping.toggle_break && no_shift {
                self.runner.toggle_debugger_break()
            }

            if _keycode == self.config.debugger.key_mapping.step_previous && no_shift {
                self.runner.send_debugger_command(DebuggerCommand::Previous)
            }

            if _keycode == self.config.debugger.key_mapping.step_next && no_shift {
                self.runner.send_debugger_command(DebuggerCommand::Next)
            }

            if _keycode == self.config.debugger.key_mapping.print_registers && no_shift {
                self.runner
                    .send_debugger_command(DebuggerCommand::PrintRegisters)
            }

            if _keycode == self.config.debugger.key_mapping.print_stack && no_shift {
                self.runner
                    .send_debugger_command(DebuggerCommand::PrintStack)
            }

            if _keycode == self.config.debugger.key_mapping.print_timers && no_shift {
                self.runner
                    .send_debugger_command(DebuggerCommand::PrintTimers)
            }
        }
    }
}
