use log::info;
use virtual_machine::{
    add_instr,
    instruction::{Instruction::*, IntrinsicOp, UnaryOpImm},
    logging,
    machine::executor::Executor,
    types::IoBuffer,
};

fn main() {
    logging::init();
    info!("SYMBOLIC EXECUTION ENVIRONMENT");

    let _program: Vec<virtual_machine::instruction::Instruction> = vec![
        add_instr!(Push, 123),
        add_instr!(io Print, 0), // Should print 123
    ];

    let program = vec![add_instr!(io Input), add_instr!(io Print, 0)];

    Executor::new(program)
        .redirect_input(IoBuffer::new(vec![42]).into())
        .exec()
        .unwrap();
    println!()
}
