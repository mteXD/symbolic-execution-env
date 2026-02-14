/*
 * A virtual machine that's a hybrid between a register-based and stack-based
 * architecture.
 *
 * Registers (referred to as "Cells") are temporarily represented as i64 value.
 * These "registers" are referred to as "cells" in the code.
 *
 * It has a push instruction - it places a value to the next available cell.
 * This is similar to Single Static Assignment (SSA) form in compilers.
 *
 * Pop doesn't have to exist for reading purposes, as we can read directly from
 * available cell.
 *
 * However, pop can be used to free up cells when needed.
 */

use std::{collections::HashMap, fmt::Debug};

pub type Cell = u16;
pub type Immediate = i64;

#[derive(Debug, Clone)]
pub enum MachineError {
    StackUnderflow,
    InvalidCell,
    DivisionByZero,
    NoSavedCells,
    RebaseError,
    NoRebasedCells,
    FunctionRedefinition,
    FunctionUndefined,
    FunctionCallError,
    InstructionError(String),
    OtherError(String),
}

// TODO: Solve this comment (delete or uncomment if derive is not good enough).
// impl Debug for MachineError {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         use MachineError::*;
//
//         let text = match self {
//             StackUnderflow => "Stack Underflow",
//             InvalidCell => "Invalid Cell",
//             DivisionByZero => "Division By Zero",
//             NoSavedCells => "No Saved Cells",
//             RebaseError => "Could Not Rebase",
//             NoRebasedCells => "No Rebased Cells",
//             FunctionRedefinition => "Function Redefinition",
//             FunctionUndefined => "Function Undefined",
//             FunctionCallError => "Function Call Error",
//         };
//         write!(f, "{}", text)
//     }
// }

#[derive(Debug, Clone)]
pub enum NullaryOp {
    Nop,
    Rebase,
}

#[derive(Debug, Clone)]
pub enum UnaryOpCell {
    Not,
    Read,
    ReadReverse,
    Tail, // Tail-call a function.
}

#[derive(Debug, Clone)]
pub enum UnaryOpImm {
    Push,
    Pop,
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

impl<'a> Instruction {
    fn eval(&'a self, machine: &mut Machine<'a>) -> Result<(), MachineError> {
        use Instruction::*;

        if let Some(val) = &machine.function_data.new_function_declared {
            if machine.function_data.function_table.contains_key(val) {
                return Err(MachineError::FunctionRedefinition);
            }

            machine
                .function_data
                .function_table
                .insert(val.clone(), self.clone());

            machine.function_data.new_function_declared = None;

            return Ok(());
        }

        eprintln!("Executing instruction: {:#?}", self);
        eprintln!("Current cells: {:#?}\n", machine.cells);

        match self {
            AluNullary(nullop) => nullop.eval(machine, ())?,
            AluUnaryImm(unop_imm, imm) => unop_imm.eval(machine, *imm)?,
            AluUnaryCell(unop_reg, reg) => unop_reg.eval(machine, *reg)?,
            AluBinary(binop, reg1, reg2) => binop.eval(machine, (*reg1, *reg2))?,
            Block(instructions) => {
                /* NOTE:
                 * Since it is likely that more pops than pushes occur, we must
                 * save the ENTIRE state of cells, copying it twice.
                 */

                let mut block_machine = Machine::from(instructions);
                block_machine.cells = machine.cells.clone();
                block_machine.base_stack.push(machine.base);
                block_machine.base = block_machine.cells.len();

                let block_result = block_machine.run()?;

                if let Some(val) = block_result {
                    machine.push(*val)?;
                }

                machine.base = block_machine
                    .base_stack
                    .pop()
                    .ok_or(MachineError::RebaseError)?
                    .clone();
            }
            AluFunction(function_op, name) => {
                function_op.eval(machine, name.clone())?;
            }
        }

        Ok(())
    }
}

pub trait Operator {
    type ArgType;

    fn eval(&self, machine: &mut Machine, arg: Self::ArgType) -> Result<(), MachineError>;
}

impl Operator for NullaryOp {
    type ArgType = ();

    fn eval(&self, machine: &mut Machine, _: Self::ArgType) -> Result<(), MachineError> {
        match self {
            NullaryOp::Nop => {}
            NullaryOp::Rebase => {
                machine.rebase()?;
            }
        }
        Ok(())
    }
}
impl Operator for UnaryOpCell {
    type ArgType = Cell;

    fn eval(&self, machine: &mut Machine, arg: Self::ArgType) -> Result<(), MachineError> {
        use UnaryOpCell::*;

        match self {
            Not => {
                let val = !*machine.read(arg)?;
                machine.push(val)?;
            }
            Read => {
                let val = *machine.read(arg)?;
                machine.push(val)?;
            }
            ReadReverse => {
                // like python's negative indexing.
                let index = u16::try_from(machine.cells.len())
                    .ok()
                    .and_then(|len| len.checked_sub(1))
                    .and_then(|len| len.checked_sub(arg))
                    .ok_or(MachineError::InvalidCell)?;
                let val = *machine.read(index)?;
                machine.push(val)?;
            }
            Tail => todo!(), // TODO: Implement tail call
        }
        Ok(())
    }
}
impl Operator for UnaryOpImm {
    type ArgType = Immediate;

    fn eval(&self, machine: &mut Machine, arg: Self::ArgType) -> Result<(), MachineError> {
        use UnaryOpImm::*;

        match self {
            Push => {
                machine.push(arg)?;
            }
            Pop => {
                machine.multi_pop(arg)?;
            }
        }
        Ok(())
    }
}
impl Operator for BinaryOp {
    type ArgType = (Cell, Cell);

    fn eval(&self, machine: &mut Machine, arg: Self::ArgType) -> Result<(), MachineError> {
        use BinaryOp::*;
        fn from_bool<T: From<bool>>(value: bool) -> T {
            value.into()
        }

        let (reg1, reg2) = arg;

        let a = machine.read(reg1)?;
        let b = machine.read(reg2)?;

        let calculated_value = match self {
            Add => a + b,
            Mul => a * b,
            Div => a.checked_div(*b).ok_or(MachineError::DivisionByZero)?,
            And => a & b,
            Or => a | b,
            Xor => a ^ b,
            ShiftLeftLogical => a << b,
            ShiftRightLogical => ((*a as u64) >> b) as i64,
            ShiftRightArithmetic => a >> b,
            SetEqual => from_bool(a == b),
            SetNotEqual => from_bool(a != b),
            SetLessThan => from_bool(a < b),
            SetLessThanOrEqual => from_bool(a <= b),
            SetGreaterThan => from_bool(a > b),
            SetGreaterThanOrEqual => from_bool(a >= b),
        };

        machine.push(calculated_value)?;

        Ok(())
    }
}

impl Operator for FunctionOp {
    type ArgType = String;

    fn eval(&self, machine: &mut Machine, arg: Self::ArgType) -> Result<(), MachineError> {
        use FunctionOp::*;

        match self {
            FunctionDefine => {
                if machine.function_data.function_table.contains_key(&arg) {
                    return Err(MachineError::FunctionRedefinition);
                }
                machine.function_data.new_function_declared = Some(arg);
            }
            FunctionCall => {
                let instructions = machine
                    .function_data
                    .function_table
                    .get(&arg)
                    .ok_or(MachineError::FunctionUndefined)?;

                let program = vec![instructions.clone()];

                let mut function_machine = Machine::from(&program);
                function_machine.cells = machine.cells.clone();

                eprintln!("Calling function '{}'", arg);
                eprintln!(" with cells: {:?}", function_machine.cells);
                eprintln!("Function instructions: {:?}", instructions);
                let function_result = function_machine.run()?;

                if let Some(val) = function_result {
                    machine.push(*val)?;
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct FunctionData {
    function_table: HashMap<String, Instruction>,
    new_function_declared: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Machine<'a> {
    cells: Vec<i64>,
    program: Option<&'a [Instruction]>,
    base: usize,
    base_stack: Vec<usize>,
    function_data: FunctionData,
}

impl<'a> Machine<'a> {
    pub fn new() -> Self {
        Machine {
            cells: Vec::new(),
            program: None,
            base: 0,
            base_stack: Vec::new(),
            function_data: FunctionData::default(),
        }
    }

    pub fn load_program(&mut self, program: &'a Vec<Instruction>) {
        self.program = Some(program);
    }

    fn push(&mut self, value: i64) -> Result<(), MachineError> {
        self.cells.push(value);
        Ok(())
    }

    fn pop(&mut self) -> Option<i64> {
        self.cells.pop()
    }

    fn multi_pop(&mut self, n: Immediate) -> Result<(), MachineError> {
        if n < 0 {
            return Err(MachineError::InvalidCell);
        }

        for _ in 0..n {
            self.pop().ok_or(MachineError::StackUnderflow)?; // Discard the popped value
        }
        Ok(())
    }

    fn read(&self, reg: Cell) -> Result<&i64, MachineError> {
        match self.cells.get::<usize>(reg.into()) {
            Some(value) => Ok(value),
            None => Err(MachineError::InvalidCell),
        }
    }

    // TODO: Delete or uncomment
    // fn save_cells(&mut self) -> Result<(), MachineError> {
    //     self.saved_cells.push(self.cells.clone()); // WARN: Clone
    //     Ok(())
    // }
    //
    // fn restore_cells(&mut self) -> Result<(), MachineError> {
    //     if let Some(saved) = self.saved_cells.pop() {
    //         self.cells = saved;
    //         Ok(())
    //     } else {
    //         Err(MachineError::NoSavedCells)
    //     }
    // }

    fn rebase(&mut self) -> Result<(), MachineError> {
        if self.base > self.cells.len() {
            return Err(MachineError::RebaseError);
        }

        self.cells = self.cells.split_off(self.base);

        Ok(())
    }

    pub fn run(&mut self) -> Result<Option<&i64>, MachineError> {
        self.program
            .ok_or(MachineError::OtherError("No program loaded".to_string()))?
            .iter()
            .try_for_each(|instr| {
                instr.eval(self).map_err(|e| {
                    eprintln!(
                        "Error executing instruction {:?}: {:?} | cells: {:?}",
                        instr, e, self.cells
                    );
                    e
                })
            })?;

        Ok(self.cells.last())
    }
}

impl<'a> From<&'a Vec<Instruction>> for Machine<'a> {
    fn from(value: &'a Vec<Instruction>) -> Self {
        let mut machine = Machine::new();
        machine.load_program(value);
        machine
    }
}

pub mod macros {
    #[macro_export]
    macro_rules! add_instr {
        ($op:ident) => {
            AluNullary(NullaryOp::$op)
        };
        ($op:ident, $a:expr) => {
            // for immediate
            AluUnaryImm(UnaryOpImm::$op, $a)
        };
        (R $op:ident, $a:expr) => {
            // for register
            AluUnaryCell(UnaryOpCell::$op, $a)
        };
        ($op:ident, $a:expr, $b:expr) => {
            AluBinary(BinaryOp::$op, $a, $b)
        };
        (fun $op:ident, $name:expr) => {
            AluFunction(FunctionOp::$op, $name)
        };
    }

    #[macro_export]
    macro_rules! make_block {
        ($($instr:expr),+) => { // Variadic arguments, at least one
            Block(vec![ $( $instr ),* ])
        };
    }

    pub use add_instr;
    pub use make_block;
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use Instruction::*;

    macro_rules! test_binop {
        ($name:ident, $a:expr, $b:expr, $op:ident => $expected:expr) => {
            #[test]
            fn $name() {
                let program = vec![
                    add_instr!(Push, $a),
                    add_instr!(Push, $b),
                    add_instr!($op, 0, 1),
                ];
                let mut machine = Machine::new();
                machine.load_program(&program);
                let last = machine.run().unwrap();
                assert_eq!(last, Some(&$expected));
            }
        };
    }

    #[test]
    fn test_push_pop() {
        let program = vec![
            add_instr!(Push, 1),
            add_instr!(Push, 2),
            add_instr!(Push, 3),
            add_instr!(Push, 4),
            add_instr!(Push, 5),
        ];
        let mut machine = Machine::new();
        machine.load_program(&program);
        let _ = machine.run().unwrap();
        assert_eq!(machine.cells[0], 1);
        assert_eq!(machine.cells[1], 2);
        assert_eq!(machine.cells[2], 3);
        assert_eq!(machine.cells[3], 4);
        assert_eq!(machine.cells[4], 5);

        let prog = vec![add_instr!(Pop, -1)];
        machine.program = Some(&prog);
        let result = machine.run();
        assert!(matches!(result, Err(MachineError::InvalidCell)));

        let prog = vec![add_instr!(Pop, 1)];
        machine.program = Some(&prog);
        let val = machine.run().unwrap();
        assert_eq!(val, Some(&4));

        let prog = vec![add_instr!(Pop, 2)];
        machine.program = Some(&prog);
        let val = machine.run().unwrap();
        assert_eq!(val, Some(&2));

        let prog = vec![add_instr!(Pop, 2)];
        machine.program = Some(&prog);
        let val = machine.run().unwrap();
        assert_eq!(val, None);

        let prog = vec![add_instr!(Pop, 1)];
        machine.program = Some(&prog);
        let result = machine.run();
        assert!(matches!(result, Err(MachineError::StackUnderflow)));
    }

    #[test]
    fn test_read() {
        let program = vec![
            add_instr!(Push, 100),
            add_instr!(Push, 200),
            add_instr!(R Read, 0),
        ];
        let mut machine = Machine::new();
        machine.load_program(&program);
        let last = machine.run().unwrap();
        assert_eq!(last, Some(&100));
        assert_eq!(machine.cells[0], 100);
        assert_eq!(machine.cells[1], 200);
    }

    #[test]
    fn test_read_reverse() {
        let program = vec![
            add_instr!(Push, 10),
            add_instr!(Push, 20),
            add_instr!(Push, 30),
            add_instr!(R ReadReverse, 1), // Should read 20
        ];
        let mut machine = Machine::new();
        machine.load_program(&program);
        let last = machine.run().unwrap();
        assert_eq!(last, Some(&20));
        assert_eq!(machine.cells[0], 10);
        assert_eq!(machine.cells[1], 20);
        assert_eq!(machine.cells[2], 30);
    }

    test_binop!(test_add, 10, 20, Add => 30);
    test_binop!(test_add_neg, 10, -30, Add => -20);
    test_binop!(test_mul, 10, 20, Mul => 200);
    test_binop!(test_div, 20, 5, Div => 4);

    #[test]
    fn test_div_bad() {
        let program = vec![
            add_instr!(Push, 10),
            add_instr!(Push, 0),
            add_instr!(Div, 0, 1),
        ];
        let mut machine = Machine::new();
        machine.load_program(&program);
        let last = machine.run();
        assert!(matches!(last, Err(MachineError::DivisionByZero)));
    }

    test_binop!(test_and, 0b1100, 0b1010, And => 0b1000);
    test_binop!(test_or, 0b1100, 0b1010, Or => 0b1110);
    test_binop!(test_xor, 0b1100, 0b1010, Xor => 0b0110);

    #[test]
    fn test_not() {
        let program = vec![add_instr!(Push, 0b1100), add_instr!(R Not, 0)];
        let mut machine = Machine::new();
        machine.load_program(&program);
        let last = machine.run().unwrap();
        assert_eq!(last, Some(&(!0b1100)));
    }

    test_binop!(test_slt, 10, 20, SetLessThan => 1);
    test_binop!(test_sgt, 20, 10, SetGreaterThan => 1);
    test_binop!(test_seq, 10, 10, SetEqual => 1);
    test_binop!(test_sne, 10, 20, SetNotEqual => 1);
    test_binop!(test_sle, 10, 10, SetLessThanOrEqual => 1);
    test_binop!(test_sge, 20, 10, SetGreaterThanOrEqual => 1);

    test_binop!(test_sll, 0b0001, 2, ShiftLeftLogical => 0b0100);
    test_binop!(test_srl, 0b0100, 2, ShiftRightLogical => 0b0001);
    test_binop!(test_sra, -8, 2, ShiftRightArithmetic => -2);

    #[test]
    fn nop() {
        let program = vec![add_instr!(Nop)];
        let mut machine = Machine::new();
        machine.load_program(&program);
        let last = machine.run().unwrap();
        assert_eq!(last, None);
    }

    #[test]
    fn math_with_read() {
        let program = vec![
            add_instr!(Push, 50),
            add_instr!(Push, 70),
            add_instr!(Push, 10),
            add_instr!(Add, 0, 1), // 50 + 70 = 120
            add_instr!(Div, 3, 2), // 120 / 10 = 12
        ];
        let mut machine = Machine::new();
        machine.load_program(&program);
        let last = machine.run().unwrap();
        assert_eq!(last, Some(&12));
    }

    mod block_tests {
        use super::*;

        #[test]
        fn test_no_arg() {
            let program = vec![
                add_instr!(Push, 10),
                add_instr!(Push, 20),
                add_instr!(Add, 0, 1),
                make_block!(
                    add_instr!(Push, 2),   // This push should be deleted after block ends
                    add_instr!(Mul, 2, 3) // This is the last push, the result, and it should remain after block ends
                ),
                add_instr!(Add, 2, 3),
            ];

            let mut machine = Machine::new();
            machine.load_program(&program);
            let last = machine.run().unwrap();
            assert_eq!(last, Some(&90)); // (10 + 20) + ((10 + 20) * 2) = 90

            assert_eq!(machine.cells[0], 10);
            assert_eq!(machine.cells[1], 20);
            assert_eq!(machine.cells[2], 30); // Result of first addition
            assert_eq!(machine.cells[3], 60); // Result of multiplication inside block
            assert_eq!(machine.cells[4], 90); // Final result
            assert!(matches!(machine.cells.get(5), None)); // Ensure no extra cells exist
            assert_eq!(machine.cells.len(), 5); // Ensure all expected cells are present
        }

        #[test]
        fn test_nested() {
            let program = vec![
                add_instr!(Push, 3),
                make_block!(
                    add_instr!(Push, 4),
                    make_block!(
                        add_instr!(Push, 5),
                        add_instr!(Mul, 1, 2) // 4 * 5 = 20
                    ),
                    add_instr!(Add, 0, 2) // 3 + 20 = 23
                ),
            ];
            let mut machine = Machine::new();
            machine.load_program(&program);
            let last = machine.run().unwrap();
            assert_eq!(last, Some(&23));
            assert_eq!(machine.cells[0], 3);
            assert_eq!(machine.cells[1], 23);
        }

        #[test]
        fn test_square_fn() {
            let square_block = make_block!(
                add_instr!(R ReadReverse, 0),
                add_instr!(R ReadReverse, 0),
                add_instr!(Rebase),
                add_instr!(Mul, 0, 1) // Multiply input by 2
            );

            let program = vec![
                add_instr!(Push, 2),
                square_block.clone(),
                square_block.clone(),
            ];

            let mut machine = Machine::new();
            machine.load_program(&program);
            let last = machine.run().unwrap();
            assert_eq!(last, Some(&16));
        }

        #[test]
        fn test_with_pop() {
            let block = make_block!(
                add_instr!(Pop, 2) // Pop the 20, leaving only 30
            );

            let program = vec![
                add_instr!(Push, 3),
                add_instr!(Push, 5),
                block,
                add_instr!(Mul, 0, 1), // 3 * 5 = 15
            ];

            let mut machine = Machine::new();
            machine.load_program(&program);
            let last = machine.run().unwrap();
            assert_eq!(last, Some(&15));
        }

        #[test]
        fn test_nested_rebase_1() {
            let program = vec![
                add_instr!(Push, 2),
                make_block!(
                    add_instr!(Push, 3),
                    add_instr!(Rebase),
                    make_block!(
                        add_instr!(Push, 4),
                        add_instr!(Mul, 0, 1) // 3 * 4 = 12
                    ),
                    add_instr!(Add, 0, 1) // 3 + 12 = 14
                ),
            ];
            let mut machine = Machine::new();
            machine.load_program(&program);
            let last = machine.run().unwrap();
            assert_eq!(last, Some(&15));
            assert_eq!(machine.cells[0], 2);
            assert_eq!(machine.cells[1], 15);
        }

        #[test]
        fn test_nested_rebase_2() {
            let program = vec![
                add_instr!(Push, 2),
                make_block!(
                    add_instr!(Push, 3),
                    add_instr!(Rebase),
                    make_block!(
                        add_instr!(R ReadReverse, 0),
                        add_instr!(Push, 4),
                        add_instr!(Rebase),
                        add_instr!(Mul, 0, 1) // 3 * 4 = 12
                    ),
                    add_instr!(Add, 0, 1) // 3 + 12 = 14
                ),
            ];
            let mut machine = Machine::new();
            machine.load_program(&program);
            let last = machine.run().unwrap();
            assert_eq!(last, Some(&15));
            assert_eq!(machine.cells[0], 2);
            assert_eq!(machine.cells[1], 15);
        }

        #[test]
        fn test_square_add_42() {
            let program = vec![
                add_instr!(Push, 5), // Argument
                make_block!(
                    add_instr!(R ReadReverse, 0), // Read x . . . r0 <- x
                    add_instr!(Rebase),
                    add_instr!(Mul, 0, 0), // x ^ 2 . . . r1 <- r0 ^ 2
                    add_instr!(Push, 42),  // r2 <- 42
                    add_instr!(Mul, 0, 2), // x * 42 . . . r3 <- r0 * r2
                    add_instr!(Add, 1, 3)  // x^2 + 42x . . . r4 <- r1 + r3
                ),
            ];

            let mut machine = Machine::new();
            machine.load_program(&program);
            let last = machine.run().unwrap();
            assert_eq!(last, Some(&235));
            assert_eq!(machine.cells[0], 5);
            assert_eq!(machine.cells[1], 235);
            assert_eq!(machine.cells.len(), 2);
        }
    }

    mod function_tests {
        use super::*;

        #[test]
        fn test_simple_function() {
            let program = vec![
                add_instr!(fun FunctionDefine, String::from("square")),
                make_block!(
                    add_instr!(R ReadReverse, 0),
                    add_instr!(Rebase),
                    add_instr!(Mul, 0, 0) // Multiply input by 2
                ),
                add_instr!(Push, 3),
                add_instr!(fun FunctionCall, String::from("square")),
            ];

            let mut machine = Machine::new();
            machine.load_program(&program);
            let last = machine.run().unwrap();
            assert_eq!(last, Some(&9));
        }

        #[test]
        fn test_sequential_definitions() {
            let program = vec![
                add_instr!(fun FunctionDefine, String::from("square")),
                add_instr!(fun FunctionDefine, String::from("cube")),
                add_instr!(fun FunctionDefine, String::from("nothing")),
                add_instr!(Push, 2),
                add_instr!(fun FunctionCall, String::from("square")),
                add_instr!(fun FunctionCall, String::from("brr")),
            ];

            // BUG: This test currently fails because we are taking whatever
            // the next instruction is as the function body. The intended behavior
            // is similar to the one of a label in assembly, so we need
            // something like a PC counter, so that we can slide across
            // FunctionDefine instructions until we hit something else.
            // TODO: Implement the PC counter.

            let mut machine = Machine::new();
            machine.load_program(&program);
            let _ = machine.run().unwrap();

            // assert_eq!(machine.cells[0], 2);
        }
    }
}
