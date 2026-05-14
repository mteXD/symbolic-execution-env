use log::{debug, error, info, warn};
use virtual_machine::{
    logging,
    macros::{add_instr},
    instruction::{
        UnaryOpCell, UnaryOpImm, BinaryOp, IntrinsicOp,
        Instruction::{self, *},
    },
    machine::executor::Executor,
};

fn main() {
    logging::init();
    info!("SYMBOLIC EXECUTION ENVIRONMENT");

    let program = vec![
        add_instr!(Push, 123),
        add_instr!(io Print, 0), // Should print 123
    ];

    let mut machine = Executor::new(&program);
    machine.eval().unwrap();
    println!()
}
