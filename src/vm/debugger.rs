use super::config::Config;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;
use strum_macros::Display;

#[derive(Display, Debug)]
pub enum DebuggerCommand {
    Next,
    Previous,

    PrintRegisters,
    PrintStack,
    PrintTimers,
}

pub struct Debugger {
    pub(super) enabled: bool,
    pub(super) enable_break: Arc<AtomicBool>,
    pub(super) consumer: mpsc::Receiver<DebuggerCommand>,
}

impl Debugger {
    pub fn new(
        config: &Config,
        enable_break: Arc<AtomicBool>,
        consumer: mpsc::Receiver<DebuggerCommand>,
    ) -> Debugger {
        Debugger {
            enabled: config.debugger.enable,
            enable_break,
            consumer,
        }
    }
}
