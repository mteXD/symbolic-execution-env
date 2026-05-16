use std::{cell::RefCell, rc::Rc};

use log::{debug, error, info, warn};
use virtual_machine::{
    instruction::{
        BinaryOp, Instruction::{self, *}, IntrinsicOp, UnaryOpCell, UnaryOpImm
    }, logging, machine::executor::Executor, macros::add_instr, types::{self, Immediate}
};

fn main() {
    logging::init();
    info!("SYMBOLIC EXECUTION ENVIRONMENT");

    let program = vec![
        add_instr!(Push, 123),
        add_instr!(io Print, 0), // Should print 123
    ];

    let program = vec![
        add_instr!(io Input, 0),
        add_instr!(io Print, 0),
    ];

    let new_input: Rc<RefCell<Vec<Immediate>>> = Rc::new(RefCell::new(vec![42]));

    let mut machine = Executor::new(&program);
    machine.redirect_input(types::Input::Buffer(new_input.clone()));
    machine.eval().unwrap();
    println!()
}
