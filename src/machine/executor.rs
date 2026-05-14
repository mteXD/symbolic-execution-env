use super::*;
use crate::{
    instruction::{BinaryOp, FunctionOp, Instruction, NullaryOp, UnaryOpCell, UnaryOpImm},
    machine::{CoreError, CoreMachine},
    types::{Cell, CellIndex, Immediate},
};
use Cell::*;
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
    TypeError { expected: Cell, found: Cell },
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
    cells: Vec<Cell>,
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
        // TODO: Optimize cells cloning (use Rc?)
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

    pub fn push(&mut self, value: Cell) {
        self.cells.push(value);
    }

    pub fn pop(&mut self) -> Option<Cell> {
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

    pub fn multi_pop(&mut self, n: CellIndex) -> Result<()> {
        for _ in 0..n {
            self.pop().ok_or(StackUnderflow)?; // Discard the popped value
        }
        Ok(())
    }

    pub fn read(&self, reg: CellIndex) -> Result<&Cell> {
        self.cells.get::<usize>(reg.into()).ok_or(InvalidCell)
    }

    pub fn rebase(&mut self) -> Result<()> {
        if self.base > self.cells.len() {
            return Err(RebaseError);
        }

        self.cells = self.cells.split_off(self.base);

        Ok(())
    }

    pub fn eval(&mut self) -> Result<Option<&Cell>> {
        while let Some(instr) = self.machine.next() {
            use Instruction::*;

            match instr {
                AluNullary(instr) => self.eval_alu_nullary(instr),
                AluUnaryImm(instr, imm) => self.eval_alu_unary_imm(instr, *imm),
                AluUnaryCell(instr, cell) => self.eval_alu_unary_cell(instr, *cell),
                AluBinary(instr, arg1, arg2) => self.eval_alu_binary(instr, *arg1, *arg2),
                Block(instrs) => self.eval_block(instrs),
                AluFunction(instr, fun) => self.eval_function(instr, fun),
                AluIntrinsic(instr) => todo!(),
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
                Some(Integer(0)) => {
                    self.machine.next(); // Skip the next instruction
                }
                Some(Integer(_)) => {}
                Some(_) => {
                    return Err(TypeError {
                        expected: Integer(0),
                        found: self
                            .cells
                            .last()
                            .cloned()
                            .expect("Stack should not be empty here"),
                    });
                }
                None => return Err(StackUnderflow),
            },
        }

        Ok(())
    }

    fn eval_alu_unary_imm(&mut self, instr: &UnaryOpImm, arg: Immediate) -> Result<()> {
        use UnaryOpImm::*;

        match instr {
            Push => self.push(Integer(arg)),
        }

        Ok(())
    }

    fn eval_alu_unary_cell(&mut self, instr: &UnaryOpCell, arg: CellIndex) -> Result<()> {
        use UnaryOpCell::*;

        match instr {
            Not => {
                let val = self.read(arg)?;
                if let Integer(val) = val {
                    self.push(Integer(!*val));
                } else {
                    return Err(TypeError {
                        expected: Integer(0),
                        found: val.clone(),
                    });
                }
            }
            Read => {
                let val = self.read(arg)?;
                self.push(val.clone());
            }
            ReadReverse => {
                // like python's negative indexing.
                let index = u16::try_from(self.cells_len())
                    .ok()
                    .and_then(|len| len.checked_sub(1))
                    .and_then(|len| len.checked_sub(arg))
                    .ok_or(InvalidCell)?;
                let val = self.read(index)?;
                self.push(val.clone());
            }
            Pop => {
                self.multi_pop(arg)?;
            }
        }

        Ok(())
    }

    fn eval_alu_binary(
        &mut self,
        instr: &BinaryOp,
        arg1: CellIndex,
        arg2: CellIndex,
    ) -> Result<()> {
        use BinaryOp::*;
        fn from_bool<T: From<bool>>(value: bool) -> T {
            value.into()
        }

        let a = self.read(arg1)?;
        let b = self.read(arg2)?;

        if let (Integer(a), Integer(b)) = (a, b) {
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

            self.push(Integer(calculated_value));
        } else {
            return Err(TypeError {
                expected: Integer(0),
                found: self
                    .cells
                    .last()
                    .cloned()
                    .expect("Stack should not be empty here"),
            });
        }

        Ok(())
    }

    fn eval_block(&mut self, instrs: &'a [Instruction]) -> Result<()> {
        /* NOTE:
         * Since it is likely that more pops than pushes occur, we must
         * save the ENTIRE state of cells, copying it twice.
         */

        let mut block_self = self.sub_machine_block(instrs);

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

                let mut function_self = self.sub_machine_function(instr);
                let function_result = function_self.eval()?;

                if let Some(val) = function_result {
                    self.push(*val);
                }
            }
        }

        Ok(())
    }
}

impl From<Vec<Cell>> for Executor<'_> {
    fn from(value: Vec<Cell>) -> Self {
        let mut machine = Self::new(&[]);
        machine.cells = value;
        machine
    }
}

#[cfg(test)]
pub mod executor_tests;
