use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Instant;

use anyhow::Result;
use log::{error, info, warn};

use super::vm::{
    audio::Audio,
    config::Config,
    debugger::{Debugger, DebuggerCommand},
    display::{Display, VmDisplay, Snapshot},
    input::Input,
    Vm,
};
use crate::errors::Errors;

pub struct Runner {
    display: Arc<Mutex<dyn Display>>,
    audio: Arc<Mutex<Audio>>,
    alive: Arc<AtomicBool>,

    debug_break: Arc<AtomicBool>,
    debug_sender: Sender<DebuggerCommand>,

    handle: Option<JoinHandle<()>>,
}

impl Runner {
    pub fn new(config: &Config, input: Arc<Mutex<dyn Input>>) -> Result<Runner> {
        let rom_bytes = match fs::read(&config.rom) {
            Ok(bytes) => bytes,
            Err(err) => {
                return Err(Errors::RomLoadFailed {
                    name: config.rom.clone(),
                    error: err,
                }
                .into())
            }
        };

        let display = Arc::new(Mutex::new(VmDisplay::new()));
        let audio = Arc::new(Mutex::new(Audio::new()));
        let alive = Arc::new(AtomicBool::new(true));

        let (tx, rx) = channel::<DebuggerCommand>();
        let debug_break = Arc::new(AtomicBool::new(false));

        let debugger = Debugger::new(config, debug_break.clone(), rx);

        let thread_alive = alive.clone();
        match Vm::new(
            config,
            &rom_bytes,
            display.clone(),
            input.clone(),
            audio.clone(),
            debugger,
        ) {
            Ok(mut vm) => {
                info!("Starting VM ...");

                let handle = thread::spawn(move || {
                    let mut delta = 0u128;
                    while thread_alive.load(Ordering::SeqCst) {
                        let t0 = Instant::now();

                        if let Err(msg) = vm.tick(delta) {
                            error!("ERROR in VM execution: {}", msg);
                        }

                        let dur = Instant::now() - t0;
                        delta = dur.as_nanos();
                    }
                });

                Ok(Runner {
                    display,
                    audio,
                    alive,
                    debug_break,
                    debug_sender: tx,
                    handle: Some(handle),
                })
            }
            Err(msg) => Err(msg),
        }
    }

    pub fn get_display_snapshot(&self) -> Snapshot {
        let display = self.display.lock().unwrap();
        display.get_snapshot()
    }

    pub fn is_playing_sound(&self) -> bool {
        let audio = self.audio.lock().unwrap();
        audio.is_playing()
    }

    pub fn toggle_debugger_break(&mut self) {
        let x = self.debug_break.load(Ordering::SeqCst);
        self.debug_break.store(!x, Ordering::SeqCst);
    }

    pub fn send_debugger_command(&mut self, command: DebuggerCommand) {
        if let Err(err) = self.debug_sender.send(command) {
            warn!("Failed to send debugger command: {}", err);
        }
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        info!("Shutting down VM ...");

        self.alive.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap();
        }
    }
}
