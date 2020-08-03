#[derive(Debug, PartialEq)]
#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
pub(super) enum OpCode {
    Unknown,

    Raw_Call                { nnn: u16 },

    Disp_Clear,
    Disp                    { x: usize, y: usize, n: u8 },

    Flow_Return,
    Flow_Jump               { nnn: u16 },
    Flow_Call               { nnn: u16 },
    Flow_Jump_Offset        { nnn: u16 },

    Cond_Eq_Const           { x: usize, nn: u8 },
    Cond_Neq_Const          { x: usize, nn: u8 },
    Cond_Eq_Reg             { x: usize, y: usize },
    Cond_Neq_Reg            { x: usize, y: usize },

    Const_Set_Reg           { x: usize, nn: u8 },
    Const_Add_Reg           { x: usize, nn: u8 },

    Assign                  { x: usize, y: usize },

    BitOp_Or                { x: usize, y: usize },
    BitOp_And               { x: usize, y: usize },
    BitOp_Xor               { x: usize, y: usize },
    BitOp_Shift_Right       { x: usize, y: usize },
    BitOp_Shift_Left        { x: usize, y: usize },

    Math_Add                { x: usize, y: usize },
    Math_Minus              { x: usize, y: usize },
    Math_Minus_Reverse      { x: usize, y: usize },

    MEM_Set_I               { nnn: u16 },
    MEM_Add_I               { x: usize },
    MEM_Set_Sprite_I        { x: usize },
    MEM_Reg_Dump            { x: usize },
    MEM_Reg_Load            { x: usize },

    Rand                    { x: usize, nn: u8 },

    BCD                     { x: usize },

    Timer_Delay_Get         { x: usize },
    Timer_Delay_Set         { x: usize },

    Sound_Set               { x: usize },

    KeyOp_Skip_Pressed      { x: usize },
    KeyOp_Skip_Not_Pressed  { x: usize },
    KeyOp_Await             { x: usize },
}
