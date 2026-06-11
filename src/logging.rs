use std::sync::Once;

use log::error;

pub static mut INDENT_AMNT: usize = 0;
static INIT: Once = Once::new();

pub fn init() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Trace)
        .init();
}

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
    for _ in 0..unsafe { INDENT_AMNT } {
        print!("\t");
    }
}

pub fn init_logger() {
    INIT.call_once(|| {
        let res = env_logger::builder().is_test(true).try_init();
        if let Err(e) = res {
            error!("Failed to initialize logger: {}", e);
        }
    });
}
