use super::*;
use crate::{
    instruction::{BinaryOp, FunctionOp, Instruction, NullaryOp, UnaryOpCell, UnaryOpImm},
    machine::{CoreError, CoreMachine},
    types::{Cell, Immediate},
};
use ExecutorError::*;
use log::debug;

#[derive(Debug, Clone)]
pub enum ExecutorError {
    DivisionByZero,
    ArithmeticOverflow,
    StackUnderflow,
    NoSavedCells,
    RebaseError,
    NoRebasedCells,
    InvalidCell,
    Core(CoreError),
}

impl From<CoreError> for ExecutorError {
    fn from(error: CoreError) -> Self {
        ExecutorError::Core(error)
    }
}

type Result<T> = std::result::Result<T, ExecutorError>;

pub struct Executor<'a> {
    machine: CoreMachine<'a>,
    cells: Vec<i64>,
    base: usize, // The index in `cells` where the current block/function's cells start.
    base_stack: Vec<usize>,
}

impl<'a> Executor<'a> {
    pub fn new(program: &'a [Instruction]) -> Self {
        Self {
            machine: CoreMachine::new(program),
            cells: Vec::new(),
            base: 0,
            base_stack: Vec::new(),
        }
    }

    fn sub_machine(&self, program: &'a [Instruction]) -> Self {
        // TODO: Optimize
        Self {
            machine: CoreMachine::sub_machine(&self.machine, program),
            cells: self.cells.clone(),
            base: 0,
            base_stack: Vec::new(),
        }
    }

    pub fn sub_machine_block(&self, program: &'a [Instruction]) -> Self {
        let mut sub_machine = self.sub_machine(program);
        sub_machine.base_stack.push(self.base);
        sub_machine.base = self.cells.len();
        sub_machine
    }

    pub fn sub_machine_function(&self, program: &'a [Instruction]) -> Self {
        self.sub_machine(program)
    }

    pub fn new_block(&mut self) {
        self.base_stack.push(self.base);
        self.base = self.cells.len();
    }

    pub fn push(&mut self, value: i64) {
        self.cells.push(value);
    }

    pub fn pop(&mut self) -> Option<i64> {
        self.cells.pop()
    }

    pub fn base_pop(&mut self) -> Option<usize> {
        self.base_stack.pop()
    }

    pub fn set_base(&mut self, new_base: usize) {
        self.base = new_base;
    }

    pub fn cells_len(&self) -> usize {
        self.cells.len()
    }

    pub fn multi_pop(&mut self, n: Cell) -> Result<()> {
        for _ in 0..n {
            self.pop().ok_or(StackUnderflow)?; // Discard the popped value
        }
        Ok(())
    }

    pub fn read(&self, reg: Cell) -> Result<&i64> {
        self.cells.get::<usize>(reg.into()).ok_or(InvalidCell)
    }

    pub fn rebase(&mut self) -> Result<()> {
        if self.base > self.cells.len() {
            return Err(RebaseError);
        }

        self.cells = self.cells.split_off(self.base);

        Ok(())
    }

    pub fn eval(&mut self) -> Result<Option<&i64>> {
        while let Some(instr) = self.machine.next() {
            use Instruction::*;

            match instr {
                AluNullary(instr) => self.eval_alu_nullary(instr),
                AluUnaryImm(instr, imm) => self.eval_alu_unary_imm(instr, *imm),
                AluUnaryCell(instr, cell) => self.eval_alu_unary_cell(instr, *cell),
                AluBinary(instr, arg1, arg2) => self.eval_alu_binary(instr, *arg1, *arg2),
                Block(instrs) => self.eval_block(instrs),
                AluFunction(instr, fun) => self.eval_function(instr, fun),
            }?;

            if let Block(_) = instr {
                debug!("Done with Block");
            } else {
                debug!("Done with {:#?},\ncells: {:#?}", instr, self.cells);
            }
        }

        Ok(self.cells.last())
    }

    fn eval_alu_nullary(&mut self, instr: &NullaryOp) -> Result<()> {
        use NullaryOp::*;

        match instr {
            Nop => {}
            Rebase => self.rebase()?,
            Cond => match self.pop() {
                Some(1) => {}
                Some(_) => {
                    self.machine.next(); // Skip the next instruction
                }
                None => return Err(StackUnderflow),
            },
        }

        Ok(())
    }

    fn eval_alu_unary_imm(&mut self, instr: &UnaryOpImm, arg: Immediate) -> Result<()> {
        use UnaryOpImm::*;

        match instr {
            Push => self.push(arg),
        }

        Ok(())
    }

    fn eval_alu_unary_cell(&mut self, instr: &UnaryOpCell, arg: Cell) -> Result<()> {
        use UnaryOpCell::*;

        match instr {
            Not => {
                let val = !*self.read(arg)?;
                self.push(val);
            }
            Read => {
                let val = *self.read(arg)?;
                self.push(val);
            }
            ReadReverse => {
                // like python's negative indexing.
                let index = u16::try_from(self.cells_len())
                    .ok()
                    .and_then(|len| len.checked_sub(1))
                    .and_then(|len| len.checked_sub(arg))
                    .ok_or(InvalidCell)?;
                let val = *self.read(index)?;
                self.push(val);
            }
            Pop => {
                self.multi_pop(arg)?;
            }
        }

        Ok(())
    }

    fn eval_alu_binary(&mut self, instr: &BinaryOp, arg1: Cell, arg2: Cell) -> Result<()> {
        use BinaryOp::*;
        fn from_bool<T: From<bool>>(value: bool) -> T {
            value.into()
        }

        let a = self.read(arg1)?;
        let b = self.read(arg2)?;

        let calculated_value = match instr {
            Add => a.checked_add(*b).ok_or(ArithmeticOverflow)?,
            Mul => a.checked_mul(*b).ok_or(ArithmeticOverflow)?,
            Div => a.checked_div(*b).ok_or(DivisionByZero)?,
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

        self.push(calculated_value);

        Ok(())
    }

    fn eval_block(&mut self, instrs: &'a [Instruction]) -> Result<()> {
        /* NOTE:
         * Since it is likely that more pops than pushes occur, we must
         * save the ENTIRE state of cells, copying it twice.
         */

        let mut block_self = self.sub_machine(instrs);
        block_self.new_block();

        let block_result = block_self.eval()?;

        // WARN: What if this block returns "void"? Add this to checker.
        if let Some(val) = block_result {
            self.push(*val);
        }

        self.set_base(block_self.base_pop().ok_or(RebaseError)?.clone());

        Ok(())
    }

    fn eval_function(&mut self, instr: &FunctionOp, arg: &str) -> Result<()> {
        use FunctionOp::*;

        match instr {
            FunctionDefine => self.machine.common_function_logic(arg)?,
            FunctionCall => {
                let instr = self.machine.function_get(&arg).map(std::slice::from_ref)?;

                let mut function_self = self.sub_machine(instr);
                let function_result = function_self.eval()?;

                if let Some(val) = function_result {
                    self.push(*val);
                }
            }
        }

        Ok(())
    }
}

impl From<Vec<i64>> for Executor<'_> {
    fn from(value: Vec<i64>) -> Self {
        let mut machine = Self::new(&[]);
        machine.cells = value;
        machine
    }
}

#[cfg(test)]
pub mod executor_tests;
