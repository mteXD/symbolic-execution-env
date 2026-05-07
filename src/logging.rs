use log::{debug, info, warn, error};

pub static mut INDENT_AMNT: usize = 0;

pub fn indent_inc() {
    unsafe {
        INDENT_AMNT += 1;
    }
}

pub fn indent_dec() {
    unsafe {
        INDENT_AMNT -= 1;
    }
}

pub fn indent_prt() {
    for _ in 0..unsafe{INDENT_AMNT} {
        print!("\t");
    }
}
