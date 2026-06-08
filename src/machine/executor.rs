use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use super::*;
use crate::{
    instruction::{
        BinaryOp, FunctionOp, Instruction, IntrinsicArg, IntrinsicOp, NullaryOp, UnaryOpCell, UnaryOpImm,
    },
    machine::{CoreError, CoreMachine, StackFrames},
    types::{self, Cell, CellIndex, Immediate, ProgramData},
};
use Cell::*;
use ExecutorError::*;

#[derive(Debug, Clone)]
pub enum ExecutorError {
    DivisionByZero,
    ArithmeticOverflow,
    StackUnderflow,
    NoSavedCells,
    NoRebasedCells,
    InvalidCell,
    TypeError { expected: Cell, found: Cell },
    BlockHasEmptyStack,
    Core(CoreError),
}

impl From<CoreError> for ExecutorError {
    fn from(error: CoreError) -> Self {
        ExecutorError::Core(error)
    }
}

type Result<T> = std::result::Result<T, ExecutorError>;

const RECURSION_LIMIT: usize = 50;

pub struct Executor {
    machine: CoreMachine,
    pub stack: StackFrames<Cell>,
    function_depth: usize,
}

// Convenience: lets `executor.cells`, `executor.base`, and the stack methods
// (`push`, `pop`, `get`) resolve directly through the inner `StackFrames`.
impl Deref for Executor {
    type Target = StackFrames<Cell>;
    fn deref(&self) -> &StackFrames<Cell> {
        &self.stack
    }
}
impl DerefMut for Executor {
    fn deref_mut(&mut self) -> &mut StackFrames<Cell> {
        &mut self.stack
    }
}

impl Executor {
    pub fn new(program: impl Into<Rc<[Instruction]>>) -> Self {
        Self {
            machine: CoreMachine::new(program),
            stack: StackFrames::new(),
            function_depth: 0,
        }
    }

    pub fn redirect_input(&mut self, new_input: types::Input) {
        self.machine.input = new_input;
    }

    pub fn redirect_output(&mut self, new_output: types::Output) {
        self.machine.output = new_output;
    }

    #[inline]
    pub fn read(&self, reg: CellIndex) -> Result<Cell> {
        self.get(reg.into()).copied().ok_or(InvalidCell)
    }

    fn run(&mut self) -> Result<()> {
        while let Some(instr) = self.machine.next() {
            self.evaluate_instruction(&instr)?;
        }
        Ok(())
    }

    pub fn exec(&mut self) -> Result<Option<&Cell>> {
        self.run()?;
        Ok(self.cells.last())
    }

    /// Runs `instrs` as a nested context (block / function body / ifelse branch).
    ///
    /// Saves and restores `program_data` (so the inner program is scoped) and
    /// `function_data` (so inner `FunctionDefine`s don't leak to the parent).
    /// Cells are managed in-place via the [`StackFrames`] helper.
    fn run_nested(&mut self, instrs: Rc<[Instruction]>) -> Result<Option<Cell>> {
        let saved_base = self.enter_block();
        let saved_pd = std::mem::replace(&mut self.machine.program_data, ProgramData::new(instrs));

        let exec_result = self.run();

        self.machine.program_data = saved_pd;
        let (result, _) = self.exit_block(saved_base);

        exec_result?;
        Ok(result)
    }

    /// Runs the body of an ifelse branch *inline* on the parent stack: cells
    /// are not isolated, so pops and pushes persist after the branch ends.
    /// `program_data` and `function_data` are still scoped. The marker frame
    /// pushed via [`StackFrames::enter_ifelse_branch`] makes `Rebase` an error
    /// inside the branch.
    fn run_ifelse_branch(&mut self, instrs: Rc<[Instruction]>) -> Result<()> {
        // Save program_data and function_data; add a ifelse frame
        self.enter_ifelse_branch();
        let saved_pd = std::mem::replace(&mut self.machine.program_data, ProgramData::new(instrs));

        let exec_result = self.run();

        // Restore program_data and function_data; pop the ifelse frame
        self.machine.program_data = saved_pd;
        self.exit_ifelse_branch();

        exec_result
    }
}

impl Evaluate for Executor {
    type Error = ExecutorError;

    fn evaluate_alu_nullary(&mut self, instr: &NullaryOp) -> Result<()> {
        use NullaryOp::*;

        match instr {
            Nop => (),
            Rebase => self.rebase()?,
        }

        Ok(())
    }

    fn evaluate_alu_unary_imm(&mut self, instr: &UnaryOpImm, arg: Immediate) -> Result<()> {
        use UnaryOpImm::*;

        match instr {
            Push => self.push(Integer(arg)),
        }

        Ok(())
    }

    fn evaluate_alu_unary_cell(&mut self, instr: &UnaryOpCell, arg: CellIndex) -> Result<()> {
        use UnaryOpCell::*;

        match instr {
            Not => {
                let val = self.read(arg)?;
                if let Integer(v) = val {
                    self.push(Integer(!v));
                } else {
                    return Err(TypeError {
                        expected: Integer(0),
                        found: val,
                    });
                }
            }
            Read => {
                let val = self.read(arg)?;
                self.push(val);
            }
            ReadReverse => {
                // like python's negative indexing.
                let index = u16::try_from(self.cells.len())
                    .ok()
                    .and_then(|len| len.checked_sub(1))
                    .and_then(|len| len.checked_sub(arg))
                    .ok_or(InvalidCell)?;
                let val = self.read(index)?;
                self.push(val);
            }
            Pop => {
                for _ in 0..arg {
                    self.pop().ok_or(StackUnderflow)?; // Discard the popped value
                }
            }
        }

        Ok(())
    }

    fn evaluate_alu_binary(
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

        debug!("Evaluating binary: {:?} {:?} {:?}", a, instr, b);

        if let (Integer(a), Integer(b)) = (a, b) {
            let calculated_value = match instr {
                Add => a.checked_add(b).ok_or(ArithmeticOverflow)?,
                Mul => a.checked_mul(b).ok_or(ArithmeticOverflow)?,
                Div => a.checked_div(b).ok_or(DivisionByZero)?,
                And => a & b,
                Or => a | b,
                Xor => a ^ b,
                ShiftLeftLogical => a << b,
                ShiftRightLogical => ((a as u64) >> b) as i64,
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
                    .copied()
                    .expect("Stack should not be empty here"),
            });
        }

        Ok(())
    }

    fn evaluate_block(&mut self, instrs: Rc<[Instruction]>) -> Result<()> {
        // Each block must leave at least one value on its local stack so the parent can
        // observe a result. A block that ends with an empty local stack is a "void" error.
        match self.run_nested(instrs)? {
            Some(val) => self.push(val),
            None => return Err(BlockHasEmptyStack),
        }
        Ok(())
    }

    fn evaluate_function(&mut self, instr: &FunctionOp, fun: &String) -> Result<()> {
        use FunctionOp::*;

        match instr {
            FunctionDefine => self.machine.common_function_logic(fun)?,
            FunctionCall => {
                let instr = self.machine.function_get(&fun)?.clone();

                self.function_depth += 1;
                if self.function_depth > RECURSION_LIMIT {
                    panic!("Recursion limit of {RECURSION_LIMIT} exceeded in function '{fun}'");
                }

                let result = self.run_nested(Rc::<[Instruction]>::from(vec![instr]));
                self.function_depth -= 1;

                if let Some(val) = result? {
                    self.push(val);
                }
            }
        }

        Ok(())
    }

    fn evaluate_intrinsic(&mut self, instr: &IntrinsicOp, arg: &IntrinsicArg) -> Result<()> {
        use IntrinsicOp::*;
        use IntrinsicArg::*;
        use types::{Input, Output};

        match (instr, arg) {
            (Print, Cell(cell_idx)) => {
                let val = self.read(*cell_idx)?;
                match &mut self.machine.output {
                    Output::Stdout => print!("{val}"),
                    Output::File(_) | Output::Buffer(_) => {
                        let imm = val.into_immediate().map_err(|_| InvalidCell)?;
                        self.machine.output.write(&[imm]);
                    }
                }
            }
            (Input, Cell(_)) => match &self.machine.input {
                Input::Stdin => {
                    let mut input: String = String::new();
                    std::io::stdin()
                        .read_line(&mut input)
                        .expect("Failed to read input");

                    match input.trim().parse::<i64>() {
                        Ok(val) => self.push(Integer(val)),
                        Err(e) => todo!("For now, invalid input is a fatal error: {e}"),
                    }
                }
                Input::File(_) => {
                    let mut data = self.machine.input.read_all();
                    if let Some(val) = data.pop() {
                        self.push(Integer(val));
                        self.machine.input = Input::Buffer(std::rc::Rc::new(std::cell::RefCell::new(data)));
                    } else {
                        return Err(Core(CoreError::IoReadError));
                    }
                }
                Input::Buffer(ref_cell) => {
                    let new_val = ref_cell
                        .borrow_mut()
                        .pop()
                        .ok_or(Core(CoreError::IoReadError))?;
                    self.push(Integer(new_val));
                }
            },
            (FileRead, Str(path)) => {
                if path.is_empty() {
                    self.machine.input = Input::Stdin;
                } else {
                    self.machine.input = Input::File(path.clone());
                }
            }
            (FileWrite, Str(path)) => {
                if path.is_empty() {
                    self.machine.output = Output::Stdout;
                } else {
                    self.machine.output = Output::File(path.clone());
                }
            }
            _ => return Err(InvalidCell),
        }

        Ok(())
    }

    fn evaluate_ifelse(
        &mut self,
        cond_idx: CellIndex,
        when_true: Rc<Instruction>,
        when_false: Rc<Instruction>,
    ) -> Result<()> {
        let condition = self.stack.get(cond_idx.into()).copied().ok_or(InvalidCell)?;

        let branch = match condition {
            Integer(0) => when_false,
            Integer(_) => when_true,
            other => {
                return Err(TypeError {
                    expected: Integer(0),
                    found: other,
                });
            }
        };

        // The chosen branch runs inline on the parent's cells: its pops and
        // pushes are permanent, and `Rebase` is forbidden inside it.
        // PERF: clone
        self.run_ifelse_branch(Rc::<[Instruction]>::from(vec![(*branch).clone()]))
    }
}

impl From<Vec<Cell>> for Executor {
    fn from(value: Vec<Cell>) -> Self {
        let mut machine = Self::new(Vec::<Instruction>::new());
        machine.stack.cells = value;
        machine
    }
}

#[cfg(test)]
pub mod tests;
