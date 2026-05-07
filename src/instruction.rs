use crate::{
    types::{Cell, Immediate},
};

#[derive(Debug, Clone)]
pub enum NullaryOp {
    Nop,
    Rebase,
    Cond,
}

#[derive(Debug, Clone)]
pub enum UnaryOpCell {
    Not,
    Read,
    ReadReverse,
    Pop,
    // Tail, // Tail call not needed right now.
}

#[derive(Debug, Clone)]
pub enum UnaryOpImm {
    Push,
}

#[derive(Debug, Clone)]
pub enum BinaryOp {
    // Arithmetic instructions
    Add,
    Mul,
    Div,
    // Bitwise instructions
    And,
    Or,
    Xor,
    // Shifting
    ShiftLeftLogical,
    ShiftRightLogical,
    ShiftRightArithmetic,
    // Comparison ; Good enough for early stage of development
    SetEqual,
    SetNotEqual,
    SetLessThan,
    SetLessThanOrEqual,
    SetGreaterThan,
    SetGreaterThanOrEqual,
}

#[derive(Debug, Clone)]
pub enum FunctionOp {
    FunctionDefine,
    FunctionCall,
}

#[derive(Debug, Clone)]
pub enum Instruction {
    AluNullary(NullaryOp),
    AluUnaryImm(UnaryOpImm, Immediate),
    AluUnaryCell(UnaryOpCell, Cell),
    AluBinary(BinaryOp, Cell, Cell),
    Block(Vec<Instruction>),
    AluFunction(FunctionOp, String),
}

// impl<'a> Instruction {
//     pub fn eval(&'a self, machine: &mut Machine<'a>) -> Result<(), CoreError> {
//         use Instruction::*;
//
//         match self {
//             AluNullary(nullop) => nullop.eval(machine, ())?,
//             AluUnaryImm(unop_imm, imm) => unop_imm.eval(machine, *imm)?,
//             AluUnaryCell(unop_reg, reg) => unop_reg.eval(machine, *reg)?,
//             AluBinary(binop, reg1, reg2) => binop.eval(machine, (*reg1, *reg2))?,
//             Block(instructions) => {
//                 /* NOTE:
//                  * Since it is likely that more pops than pushes occur, we must
//                  * save the ENTIRE state of cells, copying it twice.
//                  */
//
//                 let mut block_machine = machine.sub_machine(instructions);
//                 block_machine.new_block();
//
//                 let block_result = block_machine.run()?;
//
//                 // WARN: What if this block returns "void"? Add this to checker.
//                 if let Some(val) = block_result {
//                     machine.push(*val);
//                 }
//
//                 machine.set_base(
//                     block_machine
//                         .base_pop()
//                         .ok_or(CoreError::RebaseError)?
//                         .clone(),
//                 );
//             }
//             AluFunction(function_op, name) => {
//                 function_op.eval(machine, name.clone())?;
//             }
//         }
//
//         Ok(())
//     }
//
//     pub fn check(&'a self, verificator: &mut Verificator) -> Result<(), VerificatorError> {
//         use Instruction::*;
//
//         match self {
//             AluNullary(nullop) => nullop.check(verificator, ())?,
//             AluUnaryImm(unop_imm, imm) => unop_imm.check(verificator, *imm)?,
//             AluUnaryCell(unop_reg, reg) => unop_reg.check(verificator, *reg)?,
//             AluBinary(binop, reg1, reg2) => binop.check(verificator, (*reg1, *reg2))?,
//             Block(instructions) => {
//                 let mut block_verificator = Verificator::from(verificator.clone());
//                 block_verificator.program = instructions;
//                 block_verificator.pc = 0;
//
//                 // eprintln!("Block verificator state:");
//                 // eprintln!("\tProgram: {:?}", block_verificator.program);
//                 // eprintln!("\tCells: {:?}", block_verificator.cell_count);
//                 // eprintln!("\tCells from original: {:?}", verificator.cell_count);
//                 // eprintln!("\tBlock cells: {:?}", block_verificator.block_cells);
//                 // eprintln!("\tFunctions: {:?}", block_verificator.function_data);
//                 // eprintln!("\tpc: {:?}", block_verificator.pc);
//
//                 block_verificator.verify()?
//             }
//             AluFunction(function_op, name) => {
//                 function_op.check(verificator, name.clone())?;
//             }
//         }
//
//         Ok(())
//     }
// }
