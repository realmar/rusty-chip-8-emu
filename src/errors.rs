use thiserror::Error;

#[derive(Error, Debug)]
pub enum Errors {
    #[error("Rom does not contain any data")]
    RomEmpty,

    #[error("ROM size too big {size} max is {max}")]
    RomTooBig {
        size: usize,
        max: usize,
    },

    #[error("Cannot load ROM {name} error: {error}")]
    RomLoadFailed {
        name: String,
        error: std::io::Error,
    },

    #[error("Stack is empty cannot pop frame")]
    StackEmpty,
}
