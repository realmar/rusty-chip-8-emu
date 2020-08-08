use std::sync::{Arc, Mutex};

use log::error;

use ggez::audio;
use ggez::audio::SoundSource;
use ggez::event::EventHandler;
use ggez::input::keyboard;
use ggez::input::keyboard::{KeyCode, KeyMods};
use ggez::{graphics, Context, GameResult};

use super::input::GGEZInput;
use crate::runner::Runner;
use crate::vm::audio as vm_audio;
use crate::vm::config::Config;
use crate::vm::constants::{SCREEN_SIZE_X, SCREEN_SIZE_Y};
use crate::vm::debugger::DebuggerCommand;

pub struct Emulator {
    config: Config,
    screen_scaling: f32,

    runner: Runner,
    input: Arc<Mutex<GGEZInput>>,
    beep: audio::Source,
}

impl Emulator {
    pub fn new(ctx: &mut Context, config: Config) -> Result<Emulator, String> {
        let (input, runner) = Emulator::create_runner(&config)?;

        Ok(Emulator {
            beep: Emulator::create_beep(&config, ctx)?,
            screen_scaling: config.screen_scaling,
            config,
            input,
            runner,
        })
    }

    fn create_runner(config: &Config) -> Result<(Arc<Mutex<GGEZInput>>, Runner), String> {
        let input = Arc::new(Mutex::new(GGEZInput::new(&config)));
        Ok((input.clone(), Runner::new(&config, input.clone())?))
    }

    fn create_beep(config: &Config, ctx: &mut Context) -> Result<audio::Source, String> {
        let sound_bytes = vm_audio::sample(config.beep_frequency)?;
        Ok(audio::Source::from_data(ctx, audio::SoundData::from_bytes(sound_bytes.as_slice())).unwrap())
    }

    fn reset(&mut self, ctx: &mut Context) -> Result<(), String> {
        let config = match Config::load() {
            Ok(c) => c,
            Err(err) => return Err(format!("{}", err)),
        };
        let (input, runner) = Emulator::create_runner(&config)?;

        self.beep = Emulator::create_beep(&config, ctx)?;
        self.config = config;
        self.input = input;
        self.runner = runner;

        Ok(())
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
                                (x + curr_pixel) as f32 * self.screen_scaling,
                                y as f32 * self.screen_scaling,
                                self.screen_scaling,
                                self.screen_scaling,
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
            if let Err(msg) = self.reset(_ctx) {
                error!("ERROR resetting VM: {}", msg);
            }
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
