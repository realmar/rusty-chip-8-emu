use std::vec::Vec;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;

#[cfg(test)]
use mocktopus::macros::*;

use log::{trace, warn};

use rand;

use super::display::{Display, DisplayState, RawScreen};
use super::audio::Audio;
use super::input::Input;
use super::config::Config;
use super::debugger::{Debugger,DebuggerCommand};

use super::constants::*;
use super::opcodes::*;
use super::timer::Timer;

#[cfg_attr(test, mockable)]
fn get_random() -> u8 {
    rand::random::<u8>()
}

type VmRegisters = [u8; REGISTER_COUNT];
type VmStack = Vec<StackFrame>;
type VmMemory = [u8; MEMORY_SIZE];

#[derive(Clone)]
#[allow(non_snake_case)]
struct VmFrame {
    registers:      VmRegisters,
    stack:          VmStack,
    memory:         VmMemory,
    PC:             u16,
    I:              u16,

    delay_timer:    Timer,
    sound_timer:    Timer,

    screen:         RawScreen,
}

impl VmFrame {
    fn new() -> VmFrame {
        VmFrame {
            registers: [0u8; REGISTER_COUNT],
            stack: Vec::with_capacity(16),
            memory: [0u8; MEMORY_SIZE],
            PC: PC_START,
            I: 0,

            delay_timer: Timer::new(),
            sound_timer: Timer::new(),

            screen: [0; SCREEN_SIZE],
        }
    }
}

#[derive(Debug, Clone)]
struct StackFrame {
    return_address: u16,
}

pub struct Vm {
    display:        Arc<Mutex<dyn Display>>,
    input:          Arc<Mutex<dyn Input>>,
    audio:          Arc<Mutex<Audio>>,

    debugger: Debugger,

    tick_timer:     u128,
    tick_duration:  u128,

    frames: Vec<VmFrame>,
    frame_pointer: usize,
}

impl Vm {
    pub fn new(
        config: &Config,
        rom: &Vec<u8>,
        display: Arc<Mutex<dyn Display>>,
        input: Arc<Mutex<dyn Input>>,
        audio: Arc<Mutex<Audio>>,
        debugger: Debugger) -> Result<Vm, String> {
        let result;

        if rom.len() == 0 {
            result = Err(String::from("ROM is empty"));
        } else if rom.len() > ROM_SIZE {
            result = Err(format!("ROM size too big {} max is {}", rom.len(), ROM_SIZE));
        } else {
            let mut memory = [0u8; MEMORY_SIZE];
            let rom_slice = &mut memory[VM_RESERVED_BEGIN..rom.len() + VM_RESERVED_BEGIN];
            rom_slice.copy_from_slice(rom.as_slice());

            for n in 0..FONTS.len() {
                memory[n] = FONTS[n];
            }

            let mut frames = Vec::with_capacity(match debugger.enabled {
                true => 1024 * 1024,
                false => 1,
            });
            let mut frame = VmFrame::new();
            frame.memory = memory;
            frames.push(frame);

            let vm = Vm {
                display,
                input,
                audio,

                debugger,

                tick_timer: 0,
                tick_duration: {
                    if config.hz == 0 {
                        0
                    } else {
                        let nano_1_sec = u128::pow(10, 9);
                        let multiplicator = config.hz as f64 / VM_ORIGINAL_HZ as f64;

                        let ticks_per_seconds = VM_ORIGINAL_HZ;
                        let tick_duration_original = nano_1_sec / ticks_per_seconds;

                        (tick_duration_original as f64 / multiplicator) as u128
                    }
                },

                frames,
                frame_pointer: 0,
            };

            result = Ok(vm);
        }

        result
    }

    // delta in nanoseconds
    pub fn tick(&mut self, delta: u128) -> Result<(), String> {
        let mut result = Ok(());

        if self.tick_timer > self.tick_duration {
            self.tick_timer = 0;

            let execute_cycle = match self.debugger.enabled {
                true => self.process_debugger(),
                false => true,
            };

            if execute_cycle {
                let mut frame = self.next_frame();

                let timer_delta = match self.tick_duration {
                    0 => delta,
                    _ => self.tick_duration,
                };

                if frame.delay_timer.get() > 0 {
                    let dt = frame.delay_timer.get_mut();
                    *dt = dt.saturating_sub(timer_delta);
                }

                if frame.sound_timer.get() > 0 {
                    let st = frame.sound_timer.get_mut();
                    *st = st.saturating_sub(timer_delta);

                    if frame.sound_timer.get() == 0 {
                        let mut audio = self.audio.lock().unwrap();
                        audio.playing = false;
                    }
                }

                let raw_opcode = self.fetch(&frame);
                let opcode = self.decode(raw_opcode);

                result = self.execute(&mut frame, opcode);

                self.update_stack(frame);
            }
        } else {
            self.tick_timer += delta;
        }

        result
    }

    fn process_debugger(&mut self) -> bool {
        fn print_debug(s: &Vm, command: &DebuggerCommand) {
            let frame = s.get_current_frame();
            let opcode = s.decode(s.fetch(frame));

            println!("Debugger: {:width$} {:?}", command.to_string(), opcode, width=8);
        }

        if self.debugger.enable_break.load(Ordering::SeqCst) {
            let mut result = false;

            while let Ok(command) = self.debugger.consumer.try_recv() {
                match command {
                    DebuggerCommand::Next =>
                        if self.frame_pointer < self.frames.len() - 1 {
                            self.frame_pointer += 1;

                            print_debug(self, &command);
                        } else {
                            result = true;
                        }
                    DebuggerCommand::Previous =>
                        if self.frame_pointer > 0 {
                            self.frame_pointer -= 1;

                            print_debug(self, &command);
                        },
                    DebuggerCommand::PrintRegisters => {
                        let frame = self.get_current_frame();

                        frame.registers
                            .iter()
                            .enumerate()
                            .for_each(|(i, x)| println!("V[{:#02X}] = {}", i, x));

                        println!("PC = {}", frame.PC);
                        println!("I = {}", frame.I);
                    },
                    DebuggerCommand::PrintStack => {
                        let frame = self.get_current_frame();

                        if frame.stack.len() == 0 {
                            println!("Stack is empty");
                        } else {
                            frame.stack
                                .iter()
                                .enumerate()
                                .rev()
                                .for_each(|(i, x)| println!("Frame #{}: {:?}", i, x));
                        }
                    },
                    DebuggerCommand::PrintTimers => {
                        let frame = self.get_current_frame();

                        println!("Delay Timer: Scaled: {} Raw: {}", frame.delay_timer.get_scaled(), frame.delay_timer.get());
                        println!("Sound Timer: Scaled: {} Raw: {}", frame.sound_timer.get_scaled(), frame.sound_timer.get());
                    }
                }
            };

            self.display.lock().unwrap().set_screen(&self.get_current_frame().screen);

            result
        } else {
            true
        }
    }

    fn get_current_frame(&self) -> &VmFrame {
        self.frames.get(self.frame_pointer).unwrap()
    }

    fn next_frame(&self) -> VmFrame {
        self.get_current_frame().clone()
    }

    fn update_stack(&mut self, mut frame: VmFrame) {
        if self.debugger.enabled {
            {
                frame.screen = self.display.lock().unwrap().get_screen().clone();
            }

            if self.frame_pointer + 1 == self.frames.len() {
                self.frames.push(frame);
            } else {
                self.frames[self.frame_pointer] = frame.clone();
                self.frames[self.frame_pointer + 1] = frame;
            }

            self.frame_pointer += 1;
        } else {
            self.frames[self.frame_pointer] = frame;
        }
    }

    fn fetch(&self, frame: &VmFrame) -> u16 {
        let slice = &frame.memory[frame.PC as usize..(frame.PC + 2) as usize];
        u16::from_be_bytes([slice[0], slice[1]])
    }

    fn decode(&self, code: u16) -> OpCode {
        //  1    2    3    4
        // 1111 1111 1111 1111
        //
        // 1111 0000 0000 0000  1  0xF000
        // 0000 1111 0000 0000  2  0xF00
        // 0000 0000 1111 0000  3  0xF0
        // 0000 0000 0000 1111  4  0xF
        //
        // 1111 0000 0000 0000  OP  0xF000
        // 0000 1111 0000 0000  X   0xF00
        // 0000 0000 1111 0000  Y   0xF0
        //
        // 0000 0000 0000 1111  N  0xF
        // 0000 0000 1111 1111  NN  0xFF
        // 0000 1111 1111 1111  NNN 0xFFF

        let nibble_1 = 0xF000;
        let nibble_2 = 0xF00;
        let nibble_3 = 0x0F0;
        let nibble_4 = 0x00F;

        let op_bitmask  = nibble_1;
        let x_bitmask   = nibble_2;
        let y_bitmask   = nibble_3;
        let n_bitmask   = nibble_4;
        let nn_bitmask  = nibble_3 | nibble_4;
        let nnn_bitmask = nibble_2 | nibble_3 | nibble_4;

        let op  = (code & op_bitmask) >> (3 * 4);
        let x   = ((code & x_bitmask) >> (2 * 4)) as usize;
        let y   = ((code & y_bitmask) >> (1 * 4)) as usize;
        let n   = (code & n_bitmask)   as u8;
        let nn  = (code & nn_bitmask)  as u8;
        let nnn = (code & nnn_bitmask) as u16;

        match op {
            0x0 => match nnn {
                0x0E0 => OpCode::Disp_Clear,
                0x0EE => OpCode::Flow_Return,
                _     => OpCode::Raw_Call { nnn: nnn },
            },
            0x1 => OpCode::Flow_Jump { nnn: nnn },
            0x2 => OpCode::Flow_Call { nnn: nnn },
            0x3 => OpCode::Cond_Eq_Const { x: x, nn: nn },
            0x4 => OpCode::Cond_Neq_Const { x: x, nn: nn },
            0x5 => OpCode::Cond_Eq_Reg { x: x, y: y },
            0x6 => OpCode::Const_Set_Reg { x: x, nn: nn },
            0x7 => OpCode::Const_Add_Reg { x: x, nn: nn },
            0x8 => {
                let sub_code = code & nibble_4;
                match sub_code {
                    0x0 => OpCode::Assign { x: x, y: y },
                    0x1 => OpCode::BitOp_Or { x: x, y: y },
                    0x2 => OpCode::BitOp_And { x: x, y: y },
                    0x3 => OpCode::BitOp_Xor { x: x, y: y },
                    0x4 => OpCode::Math_Add { x: x, y: y },
                    0x5 => OpCode::Math_Minus { x: x, y: y },
                    0x6 => OpCode::BitOp_Shift_Right { x: x, y: y },
                    0x7 => OpCode::Math_Minus_Reverse { x: x, y: y },
                    0xE => OpCode::BitOp_Shift_Left { x: x, y: y },
                    _   => {
                        warn!("unknown OpCode {}", code);
                        OpCode::Unknown
                    }
                }
            }
            0x9 => OpCode::Cond_Neq_Reg { x: x, y: y },
            0xA => OpCode::MEM_Set_I { nnn: nnn },
            0xB => OpCode::Flow_Jump_Offset { nnn: nnn },
            0xC => OpCode::Rand { x: x, nn: nn },
            0xD => OpCode::Disp { x: x, y: y, n: n },
            0xE => {
                let sub_code = code & (nibble_3 | nibble_4);
                match sub_code {
                    0x9E => OpCode::KeyOp_Skip_Pressed { x: x },
                    0xA1 => OpCode::KeyOp_Skip_Not_Pressed { x: x },
                    _    => {
                        warn!("unknown OpCode {}", code);
                        OpCode::Unknown
                    }
                }
            }
            0xF => {
                let sub_code = code & (nibble_3 | nibble_4);
                match sub_code {
                    0x07 => OpCode::Timer_Delay_Get { x: x },
                    0x0A => OpCode::KeyOp_Await { x: x },
                    0x15 => OpCode::Timer_Delay_Set { x: x },
                    0x18 => OpCode::Sound_Set { x: x },
                    0x1E => OpCode::MEM_Add_I { x: x },
                    0x29 => OpCode::MEM_Set_Sprite_I { x: x },
                    0x33 => OpCode::BCD { x: x },
                    0x55 => OpCode::MEM_Reg_Dump { x: x },
                    0x65 => OpCode::MEM_Reg_Load { x: x },
                    _    => {
                        warn!("unknown OpCode {}", code);
                        OpCode::Unknown
                    }
                }
            }
            _   => {
                warn!("unknown OpCode prefix {} OpCode {}", op, code);
                OpCode::Unknown
            }
        }
    }

    fn increment_pc(&mut self, frame: &mut VmFrame) {
        frame.PC += PC_INCREMENT;
    }

    fn set_vf_flag(&mut self, frame: &mut VmFrame, value: u8) {
        frame.registers[0xF] = value;
    }

    fn execute(&mut self, frame: &mut VmFrame, code: OpCode) -> Result<(), String> {
        let mut result = Ok(());

        trace!("Executing {:?}", code);
        let mut inc_pc = true;

        match code {
            OpCode::Disp_Clear                      => self.op_clear(),
            OpCode::Disp { x, y, n }                => self.op_draw(frame, frame.registers[x], frame.registers[y], n),

            OpCode::Flow_Call { nnn }               => { self.op_call(frame, nnn); inc_pc = false },
            OpCode::Flow_Return                     => { result = self.op_return(frame); },
            OpCode::Flow_Jump { nnn }               => { frame.PC = nnn; inc_pc = false },
            OpCode::Flow_Jump_Offset { nnn }        => { frame.PC = frame.registers[0] as u16 + nnn; inc_pc = false },

            OpCode::Cond_Eq_Const { x, nn }         => if frame.registers[x] == nn { self.increment_pc(frame) }
            OpCode::Cond_Neq_Const { x, nn }        => if frame.registers[x] != nn { self.increment_pc(frame) }
            OpCode::Cond_Eq_Reg { x, y }            => if frame.registers[x] == frame.registers[y] { self.increment_pc(frame) }
            OpCode::Cond_Neq_Reg { x, y }           => if frame.registers[x] != frame.registers[y] { self.increment_pc(frame) }

            OpCode::Const_Set_Reg { x, nn }         => frame.registers[x] = nn,
            OpCode::Const_Add_Reg { x, nn }         => frame.registers[x] = frame.registers[x].wrapping_add(nn),

            OpCode::Assign { x, y }                 => frame.registers[x] = frame.registers[y],

            OpCode::BitOp_Or { x, y }               => frame.registers[x] |= frame.registers[y],
            OpCode::BitOp_And { x, y }              => frame.registers[x] &= frame.registers[y],
            OpCode::BitOp_Xor { x, y }              => frame.registers[x] ^= frame.registers[y],
            OpCode::BitOp_Shift_Right { x, .. }     => self.op_right_shift(frame, x, x),
            OpCode::BitOp_Shift_Left { x, .. }      => self.op_left_shift(frame, x, x),

            OpCode::Math_Add { x, y }               => self.op_math_add(frame, x, y, x),
            OpCode::Math_Minus { x, y }             => self.op_math_minus(frame, x, y, x),
            OpCode::Math_Minus_Reverse { x, y }     => self.op_math_minus(frame, y, x, x),

            OpCode::KeyOp_Await { x }               => { self.op_await_key(frame, x); inc_pc = false },
            OpCode::KeyOp_Skip_Pressed { x }        => self.op_key_pressed(frame, frame.registers[x]),
            OpCode::KeyOp_Skip_Not_Pressed { x }    => self.op_key_not_pressed(frame, frame.registers[x]),

            OpCode::Rand { x, nn }                  => self.op_rand(frame, x, nn),

            OpCode::BCD { x }                       => self.op_bcd(frame, frame.registers[x]),

            OpCode::Timer_Delay_Get { x }           => frame.registers[x] = frame.delay_timer.get_scaled(),
            OpCode::Timer_Delay_Set { x }           => frame.delay_timer.set_scaled(frame.registers[x]),

            OpCode::Sound_Set { x }                 => self.op_sound_set(frame, frame.registers[x]),

            OpCode::MEM_Set_I { nnn }               => frame.I = nnn,
            OpCode::MEM_Add_I { x }                 => self.op_mem_add_i(frame, frame.registers[x] as u16),
            OpCode::MEM_Reg_Dump { x }              => self.op_dump(frame, x),
            OpCode::MEM_Reg_Load { x }              => self.op_load(frame, x),
            OpCode::MEM_Set_Sprite_I { x }          => frame.I = (frame.registers[x] as usize * FONT_SYMBOL_SIZE) as u16,
            _                                       => warn!("{:?} not implemented", code),
        };

        if inc_pc {
            self.increment_pc(frame);
        }

        result
    }

    fn op_sound_set(&mut self, frame: &mut VmFrame, value: u8) {
        {
            let mut audio = self.audio.lock().unwrap();
            audio.playing = true;
        }

        frame.sound_timer.set_scaled(value);
    }

    fn op_await_key(&mut self, frame: &mut VmFrame, reg: usize) {
        let result = {
            let input = self.input.lock().unwrap();
            input.get_pressed_key()
        };

        if let Some(key) = result {
            frame.registers[reg] = key;
            self.increment_pc(frame);
        }
    }

    fn op_key_pressed(&mut self, frame: &mut VmFrame, key: u8) {
        self.op_key_conditional_jump(frame, key, true);
    }

    fn op_key_not_pressed(&mut self, frame: &mut VmFrame, key: u8) {
        self.op_key_conditional_jump(frame, key, false);
    }

    fn op_key_conditional_jump(&mut self, frame: &mut VmFrame, key: u8, jump_if_pressed: bool) {
        let is_pressed;
        {
            let input = self.input.lock().unwrap();
            is_pressed = input.is_pressed(key);
        }

        if is_pressed == jump_if_pressed {
            self.increment_pc(frame)
        }
    }

    fn op_rand(&mut self, frame: &mut VmFrame, reg: usize, mask: u8) {
        let number: u8 = get_random();
        frame.registers[reg] = number & mask;
    }

    fn op_right_shift(&mut self, frame: &mut VmFrame, reg: usize, store_reg: usize) {
        self.set_vf_flag(frame, frame.registers[reg] & 0x1);
        frame.registers[store_reg] = frame.registers[reg] >> 1;
    }

    fn op_left_shift(&mut self, frame: &mut VmFrame, reg: usize, store_reg: usize) {
        self.set_vf_flag(frame, frame.registers[reg] >> 7);
        frame.registers[store_reg] = frame.registers[reg] << 1;
    }

    fn op_math_add(&mut self, frame: &mut VmFrame, reg1: usize, reg2: usize, store_reg: usize) {
        self.op_math(frame, reg1, reg2, store_reg,
            |a, b| a.overflowing_add(b),
            |has_carry| match has_carry {
                true => 1u8,
                false => 0u8,
            });
    }

    fn op_math_minus(&mut self, frame: &mut VmFrame, reg1: usize, reg2: usize, store_reg: usize) {
        self.op_math(frame, reg1, reg2, store_reg,
            |a, b| a.overflowing_sub(b),
            |has_borrow| match has_borrow {
                true => 0u8,
                false => 1u8,
            });
    }

    fn op_math(&mut self, frame: &mut VmFrame, reg1: usize, reg2: usize, store_reg: usize, operation: fn(u8, u8) -> (u8, bool), get_carry_value: fn(bool) -> u8) {
        let a = frame.registers[reg1];
        let b = frame.registers[reg2];

        let (result, has_overflow) = operation(a, b);

        frame.registers[store_reg] = result;

        self.set_vf_flag(frame, get_carry_value(has_overflow));
    }

    fn op_clear(&mut self) {
        let mut display = self.display.lock().unwrap();
        display.clear();
    }

    fn op_draw(&mut self, frame: &mut VmFrame, x: u8, y: u8, height: u8) {
        let size = 8 * height;
        let data = &frame.memory[frame.I as usize..(frame.I + size as u16) as usize];

        let result;
        {
            let mut display = self.display.lock().unwrap();
            result = display.draw_sprite(x as usize, y as usize, height, data);
        }

        self.set_vf_flag(frame, match result {
            DisplayState::Changed => 1,
            DisplayState::Unchanged => 0,
        });
    }

    fn op_call(&mut self, frame: &mut VmFrame, address: u16) {
        frame.stack.push(StackFrame { return_address: frame.PC });
        frame.PC = address;
    }

    fn op_return(&mut self, vm_frame: &mut VmFrame) -> Result<(), String> {
        match vm_frame.stack.pop() {
            Some(frame) => { vm_frame.PC = frame.return_address; Ok(()) }
            None => Err(String::from("Unable to pop stack because stack is empty. Fatal Error.")),
        }
    }

    fn op_bcd(&mut self, frame: &mut VmFrame, data: u8) {
        let hundreds = data / 100;
        let tens = (data / 10 ) % 10;
        let ones = (data % 100) % 10;

        frame.memory[(frame.I + 0) as usize] = hundreds;
        frame.memory[(frame.I + 1) as usize] = tens;
        frame.memory[(frame.I + 2) as usize] = ones;
    }

    fn op_mem_add_i(&mut self, frame: &mut VmFrame, data: u16) {
        // frame.I = frame.I.wrapping_add(data)
        let (result, has_overflow) = frame.I.overflowing_add(data);
        frame.I = result;
        self.set_vf_flag(frame, match has_overflow {
            true => 1,
            false => 0,
        });
    }

    fn op_dump(&mut self, frame: &mut VmFrame, offset: usize) {
        for n in 0..offset + 1 {
            frame.memory[frame.I as usize + n] = frame.registers[n];
        }

        frame.I += offset as u16 + 1;
    }

    fn op_load(&mut self, frame: &mut VmFrame, offset: usize) {
        for n in 0..offset + 1 {
            frame.registers[n] = frame.memory[frame.I as usize + n];
        }

        frame.I += offset as u16 + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::display::MockDisplay;
    use super::input::MockInput;
    use mockall::*;
    use mocktopus::mocking::*;
    use test_case::test_case;

    #[allow(dead_code)]
    struct TestData {
        tx: mpsc::Sender::<DebuggerCommand>,
        vm: Vm,
        frame: VmFrame,

        display:   Arc<Mutex<MockDisplay>>,
        input:     Arc<Mutex<MockInput>>,
    }

    fn new() -> TestData {
        let config = Config::default();
        let (tx, rx) = mpsc::channel::<DebuggerCommand>();

        let display = Arc::new(Mutex::new(MockDisplay::new()));
        let input = Arc::new(Mutex::new(MockInput::new()));

        TestData {
            tx,
            vm: Vm::new(
                &config,
                &vec![0, 0],
                display.clone(),
                input.clone(),
                Arc::new(Mutex::new(Audio::new())),
                Debugger::new(&config, Arc::new(AtomicBool::new(false)), rx))
            .unwrap(),
            frame: VmFrame::new(),
            display,
            input,
        }
    }

    #[test_case(0x_0123_u16, OpCode::Raw_Call                { nnn: 0x123 }              )]
    #[test_case(0x_00E0_u16, OpCode::Disp_Clear                                          )]
    #[test_case(0x_00EE_u16, OpCode::Flow_Return                                         )]
    #[test_case(0x_1ABC_u16, OpCode::Flow_Jump               { nnn: 0xABC }              )]
    #[test_case(0x_2ABC_u16, OpCode::Flow_Call               { nnn: 0xABC }              )]
    #[test_case(0x_3123_u16, OpCode::Cond_Eq_Const           { x: 0x1, nn: 0x23 }        )]
    #[test_case(0x_4123_u16, OpCode::Cond_Neq_Const          { x: 0x1, nn: 0x23 }        )]
    #[test_case(0x_5120_u16, OpCode::Cond_Eq_Reg             { x: 0x1, y: 0x2 }          )]
    #[test_case(0x_6123_u16, OpCode::Const_Set_Reg           { x: 0x1, nn: 0x23 }        )]
    #[test_case(0x_7123_u16, OpCode::Const_Add_Reg           { x: 0x1, nn: 0x23 }        )]
    #[test_case(0x_8120_u16, OpCode::Assign                  { x: 0x1, y: 0x2 }          )]
    #[test_case(0x_8121_u16, OpCode::BitOp_Or                { x: 0x1, y: 0x2 }          )]
    #[test_case(0x_8122_u16, OpCode::BitOp_And               { x: 0x1, y: 0x2 }          )]
    #[test_case(0x_8123_u16, OpCode::BitOp_Xor               { x: 0x1, y: 0x2 }          )]
    #[test_case(0x_8124_u16, OpCode::Math_Add                { x: 0x1, y: 0x2 }          )]
    #[test_case(0x_8125_u16, OpCode::Math_Minus              { x: 0x1, y: 0x2 }          )]
    #[test_case(0x_8126_u16, OpCode::BitOp_Shift_Right       { x: 0x1, y: 0x2 }          )]
    #[test_case(0x_8127_u16, OpCode::Math_Minus_Reverse      { x: 0x1, y: 0x2 }          )]
    #[test_case(0x_812E_u16, OpCode::BitOp_Shift_Left        { x: 0x1, y: 0x2 }          )]
    #[test_case(0x_9120_u16, OpCode::Cond_Neq_Reg            { x: 0x1, y: 0x2 }          )]
    #[test_case(0x_A123_u16, OpCode::MEM_Set_I               { nnn: 0x123 }              )]
    #[test_case(0x_B123_u16, OpCode::Flow_Jump_Offset        { nnn: 0x123 }              )]
    #[test_case(0x_C123_u16, OpCode::Rand                    { x: 0x1, nn: 0x23 }        )]
    #[test_case(0x_D123_u16, OpCode::Disp                    { x: 0x1, y: 0x2, n: 3 }    )]
    #[test_case(0x_E19E_u16, OpCode::KeyOp_Skip_Pressed      { x: 0x1 }                  )]
    #[test_case(0x_E1A1_u16, OpCode::KeyOp_Skip_Not_Pressed  { x: 0x1 }                  )]
    #[test_case(0x_F107_u16, OpCode::Timer_Delay_Get         { x: 0x1 }                  )]
    #[test_case(0x_F10A_u16, OpCode::KeyOp_Await             { x: 0x1 }                  )]
    #[test_case(0x_F115_u16, OpCode::Timer_Delay_Set         { x: 0x1 }                  )]
    #[test_case(0x_F118_u16, OpCode::Sound_Set               { x: 0x1 }                  )]
    #[test_case(0x_F11E_u16, OpCode::MEM_Add_I               { x: 0x1 }                  )]
    #[test_case(0x_F129_u16, OpCode::MEM_Set_Sprite_I        { x: 0x1 }                  )]
    #[test_case(0x_F133_u16, OpCode::BCD                     { x: 0x1 }                  )]
    #[test_case(0x_F155_u16, OpCode::MEM_Reg_Dump            { x: 0x1 }                  )]
    #[test_case(0x_F165_u16, OpCode::MEM_Reg_Load            { x: 0x1 }                  )]
    fn decode(code: u16, expected: OpCode) {
        let vm = new().vm;

        let actual = vm.decode(code);

        assert_eq!(actual, expected);
    }

    #[test]
    fn op_disp_clear() {
        let mut d = new();

        {
            let mut screen = d.display.lock().unwrap();
            screen
                .expect_clear()
                .times(1)
                .return_const(());
        }

        d.vm.execute(&mut d.frame, OpCode::Disp_Clear).unwrap();
    }

    #[test]
    fn op_flow_call_return() {
        let mut d = new();
        d.frame.PC = 789;

        d.vm.execute(&mut d.frame, OpCode::Flow_Call { nnn: 123 }).unwrap();

        assert_eq!(d.frame.PC, 123);
        assert_eq!(d.frame.stack.len(), 1);
        assert_eq!(d.frame.stack[0].return_address, 789);

        d.vm.execute(&mut d.frame, OpCode::Flow_Return).unwrap();

        assert_eq!(d.frame.PC, 789 + 2);
        assert_eq!(d.frame.stack.len(), 0);
    }

    #[test]
    fn op_jump() {
        let mut d = new();

        d.vm.execute(&mut d.frame, OpCode::Flow_Jump { nnn: 123 }).unwrap();

        assert_eq!(d.frame.PC, 123);
    }

    // Cond_Eq
    #[test_case(12, OpCode::Cond_Eq_Const { x: 2, nn: 12 }, PC_START + 4 ; "const eq do jump")]
    #[test_case(12, OpCode::Cond_Eq_Const { x: 2, nn: 8  }, PC_START + 2 ; "const eq do not jump")]
    // Cond_Neq
    #[test_case(12, OpCode::Cond_Neq_Const { x: 2, nn: 12 }, PC_START + 2 ; "const neq do jump")]
    #[test_case(12, OpCode::Cond_Neq_Const { x: 2, nn: 8  }, PC_START + 4 ; "const neq do not jump")]
    fn op_cond_const(reg_nn: u8, opcode: OpCode, pc: u16) {
        let mut d = new();
        d.frame.registers[2] = reg_nn;

        d.vm.execute(&mut d.frame, opcode).unwrap();

        assert_eq!(d.frame.PC, pc);
    }

    // Cond_Eq
    #[test_case(2, 2, OpCode::Cond_Eq_Reg { x: 0, y: 1 }, PC_START + 4 ; "eq do jump")]
    #[test_case(2, 8, OpCode::Cond_Eq_Reg { x: 0, y: 1 }, PC_START + 2 ; "eq do not jump")]
    // Cond_Neq
    #[test_case(2, 2, OpCode::Cond_Neq_Reg { x: 0, y: 1 }, PC_START + 2 ; "neq do jump")]
    #[test_case(2, 8, OpCode::Cond_Neq_Reg { x: 0, y: 1 }, PC_START + 4 ; "neq do not jump")]
    fn op_cond_reg(val1: u8, val2: u8, opcode: OpCode, pc: u16) {
        let mut d = new();
        d.frame.registers[0] = val1;
        d.frame.registers[1] = val2;

        d.vm.execute(&mut d.frame, opcode).unwrap();

        assert_eq!(d.frame.PC, pc);
    }

    #[test]
    fn op_const_set_reg() {
        let mut d = new();

        d.vm.execute(&mut d.frame, OpCode::Const_Set_Reg { x: 0, nn: 12 }).unwrap();

        assert_eq!(d.frame.registers[0], 12);
    }

    #[test]
    fn op_const_add_reg() {
        let mut d = new();
        d.frame.registers[0] = 8;

        d.vm.execute(&mut d.frame, OpCode::Const_Add_Reg { x: 0, nn: 16 }).unwrap();

        assert_eq!(d.frame.registers[0], 8 + 16);
    }

    #[test]
    fn op_assign() {
        let mut d = new();
        d.frame.registers[1] = 8;

        assert_eq!(d.frame.registers[0], 0);

        d.vm.execute(&mut d.frame, OpCode::Assign { x: 0, y: 1 }).unwrap();

        assert_eq!(d.frame.registers[0], 8);
    }

    // BitOpt
    #[test_case(0xA, 0xB, 0xA | 0xB, None, OpCode::BitOp_Or { x: 0, y: 1 } ; "BitOp_Or")]
    #[test_case(0xA, 0xB, 0xA & 0xB, None, OpCode::BitOp_And { x: 0, y: 1 } ; "BitOp_And")]
    #[test_case(0xA, 0xB, 0xA ^ 0xB, None, OpCode::BitOp_Xor { x: 0, y: 1 } ; "BitOp_Xor")]

    #[test_case(0xFF, 0xFF, 0xFF >> 1, Some(1), OpCode::BitOp_Shift_Right { x: 0, y: 1 } ; "BitOp_Shift_Right 0xFF")]
    #[test_case(0xA,  0xA,  0xA  >> 1, Some(0), OpCode::BitOp_Shift_Right { x: 0, y: 1 } ; "BitOp_Shift_Right 0xA")]
    #[test_case(0xFF, 0xFF, 0xFF << 1, Some(1), OpCode::BitOp_Shift_Left { x: 0, y: 1 } ; "BitOp_Shift_Left 0xFF")]
    #[test_case(0xB,  0xB,  0xB  << 1, Some(0), OpCode::BitOp_Shift_Left { x: 0, y: 1 } ; "BitOp_Shift_Left 0xB")]

    // Math
    #[test_case(2, 8, 2 + 8, Some(0), OpCode::Math_Add { x: 0, y: 1 } ; "Math_Add no carry")]
    #[test_case(255, 8, 7,   Some(1), OpCode::Math_Add { x: 0, y: 1 } ; "Math_Add carry")]

    #[test_case(8, 2, 8 - 2, Some(1), OpCode::Math_Minus { x: 0, y: 1 } ; "Math_Minus no borrow")]
    #[test_case(2, 8, 250,   Some(0), OpCode::Math_Minus { x: 0, y: 1 } ; "Math_Minus borrow")]

    #[test_case(2, 8, 8 - 2, Some(1), OpCode::Math_Minus_Reverse { x: 0, y: 1 } ; "Math_Minus_Reverse no borrow")]
    #[test_case(8, 2, 250,   Some(0), OpCode::Math_Minus_Reverse { x: 0, y: 1 } ; "Math_Minus_Reverse borrow")]
    fn op_bitop_math(val1: u8, val2: u8, result: u8, vf: Option<u8>, opcode: OpCode) {
        let mut d = new();
        d.frame.registers[0] = val1;
        d.frame.registers[1] = val2;

        d.vm.execute(&mut d.frame, opcode).unwrap();

        assert_eq!(d.frame.registers[0], result);

        if let Some(v) = vf {
            assert_eq!(d.frame.registers[0xF], v);
        }
    }

    #[test_case(None, PC_START ; "nothing pressed")]
    #[test_case(Some(0), PC_START + 2 ; "key pressed")]
    fn op_await_key(key: Option<u8>, pc: u16) {
        let mut d = new();

        {
            let mut input = d.input.lock().unwrap();
            input.expect_get_pressed_key()
                .times(1)
                .return_const(key);
        }

        d.vm.execute(&mut d.frame, OpCode::KeyOp_Await { x: 0 }).unwrap();

        assert_eq!(d.frame.PC, pc);
    }

    #[test_case(0, false, PC_START + 2, OpCode::KeyOp_Skip_Pressed { x: 0 } ; "KeyOp_Skip_Pressed noskip")]
    #[test_case(0, true,  PC_START + 4, OpCode::KeyOp_Skip_Pressed { x: 0 } ; "KeyOp_Skip_Pressed skip")]
    #[test_case(0, false, PC_START + 4, OpCode::KeyOp_Skip_Not_Pressed { x: 0 } ; "KeyOp_Skip_Not_Pressed skip")]
    #[test_case(0, true,  PC_START + 2, OpCode::KeyOp_Skip_Not_Pressed { x: 0 } ; "KeyOp_Skip_Not_Pressed noskip")]
    fn op_key_press_skip(key: u8, is_pressed: bool, pc: u16, opcode: OpCode) {
        let mut d = new();

        d.frame.registers[0] = key;
        {
            let mut input = d.input.lock().unwrap();
            input.expect_is_pressed()
                .with(predicate::eq(key))
                .times(1)
                .return_const(is_pressed);
        }

        d.vm.execute(&mut d.frame, opcode).unwrap();

        assert_eq!(d.frame.PC, pc);
    }

    #[test]
    fn op_rand() {
        let mut d = new();

        get_random.mock_safe(|| MockResult::Return(28));

        d.vm.execute(&mut d.frame, OpCode::Rand { x: 0 , nn: 0xAB }).unwrap();

        assert_ne!(d.frame.registers[0], 28);
    }

    #[test_case(255, 0b0010, 0b0101, 0b0101)]
    #[test_case(0,   0b0000, 0b0000, 0b0000)]
    #[test_case(1,   0b0000, 0b0000, 0b0001)]
    #[test_case(23,  0b0000, 0b0010, 0b0011)]
    #[test_case(100, 0b0001, 0b0000, 0b0000)]
    #[test_case(38,  0b0000, 0b0011, 0b1000)]
    fn op_bcd(data: u8, hunderts: u8, tens: u8, ones: u8) {
        let mut d = new();
        d.frame.registers[0] = data;

        d.vm.execute(&mut d.frame, OpCode::BCD { x: 0 }).unwrap();

        assert_eq!(d.frame.memory[(d.frame.I + 0) as usize], hunderts);
        assert_eq!(d.frame.memory[(d.frame.I + 1) as usize], tens);
        assert_eq!(d.frame.memory[(d.frame.I + 2) as usize], ones);
    }

    #[test]
    fn op_timer_delay_get() {
        let mut d = new();
        d.frame.delay_timer.set_scaled(8);

        d.vm.execute(&mut d.frame, OpCode::Timer_Delay_Get { x: 0 }).unwrap();

        assert_eq!(d.frame.registers[0], 8);
    }

    #[test]
    fn op_timer_delay_set() {
        let mut d = new();
        d.frame.registers[0] = 8;

        d.vm.execute(&mut d.frame, OpCode::Timer_Delay_Set { x: 0 }).unwrap();

        assert_eq!(d.frame.delay_timer.get_scaled(), 8);
    }

    #[test]
    fn op_sound_set() {
        let mut d = new();
        d.frame.registers[0] = 8;

        d.vm.execute(&mut d.frame, OpCode::Sound_Set { x: 0 }).unwrap();

        assert_eq!(d.frame.sound_timer.get_scaled(), 8);
        assert_eq!(d.vm.audio.lock().unwrap().playing, true);
    }

    #[test]
    fn op_mem_set_i() {
        let mut d = new();

        d.vm.execute(&mut d.frame, OpCode::MEM_Set_I { nnn: 123 }).unwrap();

        assert_eq!(d.frame.I, 123);
    }

    #[test]
    fn op_mem_add_i() {
        let mut d = new();
        d.frame.I = 8;
        d.frame.registers[0] = 123;

        d.vm.execute(&mut d.frame, OpCode::MEM_Add_I { x: 0 }).unwrap();

        assert_eq!(d.frame.I, 8 + 123);
    }

    #[test_case(0)]
    #[test_case(1)]
    #[test_case(8)]
    #[test_case(12)]
    #[test_case(14)]
    #[test_case(REGISTER_COUNT - 1)]
    fn op_mem_reg_dump(vx: usize) {
        let mut d = new();
        for n in 1..REGISTER_COUNT + 1 {
            d.frame.registers[n - 1] = n as u8;
        }

        d.vm.execute(&mut d.frame, OpCode::MEM_Reg_Dump { x: vx }).unwrap();

        assert_eq!(d.frame.I, vx as u16 + 1);

        d.frame.I = 0;

        for n in 0..vx + 1 {
            assert_eq!(d.frame.memory[d.frame.I as usize + n], d.frame.registers[n])
        }
    }

    #[test_case(0)]
    #[test_case(1)]
    #[test_case(8)]
    #[test_case(12)]
    #[test_case(14)]
    #[test_case(REGISTER_COUNT - 1)]
    fn op_mem_reg_load(vx: usize) {
        let mut d = new();

        for n in 1..REGISTER_COUNT + 1 {
            d.frame.memory[d.frame.I as usize + n - 1] = n as u8;
        }

        d.vm.execute(&mut d.frame, OpCode::MEM_Reg_Load { x: vx }).unwrap();

        assert_eq!(d.frame.I, vx as u16 + 1);

        d.frame.I = 0;

        for n in 0..vx + 1 {
            assert_eq!(d.frame.registers[n], d.frame.memory[d.frame.I as usize + n]);
        }
    }

    #[test_case(0x0, 0x0 * FONT_SYMBOL_SIZE as u16)]
    #[test_case(0x1, 0x1 * FONT_SYMBOL_SIZE as u16)]
    #[test_case(0x2, 0x2 * FONT_SYMBOL_SIZE as u16)]
    #[test_case(0x3, 0x3 * FONT_SYMBOL_SIZE as u16)]
    #[test_case(0x4, 0x4 * FONT_SYMBOL_SIZE as u16)]
    #[test_case(0x5, 0x5 * FONT_SYMBOL_SIZE as u16)]
    #[test_case(0x6, 0x6 * FONT_SYMBOL_SIZE as u16)]
    #[test_case(0x7, 0x7 * FONT_SYMBOL_SIZE as u16)]
    #[test_case(0x8, 0x8 * FONT_SYMBOL_SIZE as u16)]
    #[test_case(0x9, 0x9 * FONT_SYMBOL_SIZE as u16)]
    #[test_case(0xB, 0xB * FONT_SYMBOL_SIZE as u16)]
    #[test_case(0xC, 0xC * FONT_SYMBOL_SIZE as u16)]
    #[test_case(0xD, 0xD * FONT_SYMBOL_SIZE as u16)]
    #[test_case(0xE, 0xE * FONT_SYMBOL_SIZE as u16)]
    #[test_case(0xF, 0xF * FONT_SYMBOL_SIZE as u16)]
    fn op_mem_set_sprite_i(symbol: u8, address: u16) {
        let mut d = new();
        d.frame.registers[0] = symbol;

        d.vm.execute(&mut d.frame, OpCode::MEM_Set_Sprite_I { x: 0 }).unwrap();

        assert_eq!(d.frame.I, address);
    }
}
