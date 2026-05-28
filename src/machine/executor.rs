use std::rc::Rc;

use super::*;
use crate::{
    instruction::{
        BinaryOp, FunctionOp, Instruction, IntrinsicOp, NullaryOp, UnaryOpCell, UnaryOpImm,
    },
    machine::{CoreError, CoreMachine},
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
    RebaseError,
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

/// Bookkeeping for a single nested execution context (block / function / ifelse branch).
///
/// `start` is the boundary between parent cells and cells pushed inside this context.
/// `saved_below` holds parent cells that were displaced (popped from below `start`,
/// or drained by `Rebase`) and must be restored on exit. They are stored in the order
/// they were displaced; restoration iterates them in reverse.
struct Frame {
    start: usize,
    saved_below: Vec<Cell>,
}

pub struct Executor {
    machine: CoreMachine,
    cells: Vec<Cell>,
    pub base: usize, // The index in `cells` where the current block/function's cells start.
    function_depth: usize,
    frames: Vec<Frame>,
}

impl Executor {
    pub fn new(program: impl Into<Rc<[Instruction]>>) -> Self {
        Self {
            machine: CoreMachine::new(program),
            cells: Vec::new(),
            base: 0,
            function_depth: 0,
            frames: Vec::new(),
        }
    }

    pub fn redirect_input(&mut self, new_input: types::Input) {
        self.machine.input = new_input;
    }

    pub fn redirect_output(&mut self, new_output: types::Output) {
        self.machine.output = new_output;
    }

    pub fn push(&mut self, value: Cell) {
        self.cells.push(value);
    }

    /// Pops the top cell. If popping reaches into the parent's cells (below the current
    /// frame's `start`), the popped cell is saved for restoration on frame exit and the
    /// frame's `start` boundary is decremented to keep accounting consistent.
    pub fn pop(&mut self) -> Option<Cell> {
        let popped = self.cells.pop()?;
        if let Some(frame) = self.frames.last_mut() {
            if self.cells.len() < frame.start {
                frame.saved_below.push(popped);
                frame.start -= 1;
            }
        }
        Some(popped)
    }

    pub fn read(&self, reg: CellIndex) -> Result<&Cell> {
        self.cells.get::<usize>(reg.into()).ok_or(InvalidCell)
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

    /// Runs `instrs` as a nested context (block / function body / ifelse branch) on the
    /// shared `cells` vector. Returns the last cell pushed by the body, or `None` if the
    /// body left the body-local stack empty. Parent cells are restored after execution.
    fn run_nested(&mut self, instrs: Rc<[Instruction]>) -> Result<Option<Cell>> {
        self.frames.push(Frame {
            start: self.cells.len(),
            saved_below: Vec::new(),
        });
        let saved_base = self.base;
        self.base = self.cells.len();

        // Swap in the new program; clone function_data so inner FunctionDefines don't leak.
        let saved_pd = std::mem::replace(
            &mut self.machine.program_data,
            ProgramData::new(instrs),
        );
        let saved_fd = self.machine.function_data.clone();

        let exec_result = self.run();

        self.machine.program_data = saved_pd;
        self.machine.function_data = saved_fd;
        self.base = saved_base;

        let frame = self.frames.pop().expect("frame must exist");

        exec_result?;

        // Match legacy semantics: the block's result is the top of the body-local stack
        // at end of body. With shared cells, a body that did nothing inherits the parent's
        // top cell (a "void" block). Returns None only if the body fully drained the stack.
        let result = self.cells.last().copied();

        // Discard any body-local cells, then restore displaced parent cells.
        self.cells.truncate(frame.start);
        self.cells.extend(frame.saved_below.iter().rev().copied());

        Ok(result)
    }
}

impl Evaluate for Executor {
    type Error = ExecutorError;

    fn evaluate_alu_nullary(&mut self, instr: &NullaryOp) -> Result<()> {
        use NullaryOp::*;

        match instr {
            Nop => (),
            Rebase => {
                if self.base > self.cells.len() {
                    return Err(RebaseError);
                }

                // Drain cells[..base] (the parent's cells visible to this frame) and
                // hand them to the current frame so they can be restored on exit.
                let drained: Vec<Cell> = self.cells.drain(..self.base).collect();
                if let Some(frame) = self.frames.last_mut() {
                    frame.saved_below.extend(drained.into_iter().rev());
                    frame.start = 0;
                }
            }
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
                let index = u16::try_from(self.cells.len())
                    .ok()
                    .and_then(|len| len.checked_sub(1))
                    .and_then(|len| len.checked_sub(arg))
                    .ok_or(InvalidCell)?;
                let val = self.read(index)?;
                self.push(val.clone());
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

        debug!(
            "Evaluating binary: {:?} {:?} {:?}",
            a, instr, b
        );

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

    fn evaluate_intrinsic(&mut self, instr: &IntrinsicOp, arg: CellIndex) -> Result<()> {
        use IntrinsicOp::*;
        use types::Input;

        match instr {
            Print => {
                let val = self.read(arg)?;
                print!("{val}");
            }
            Input => {
                match &self.machine.input {
                    Input::Stdin => {
                        let mut input: String = String::new();
                        std::io::stdin()
                            .read_line(&mut input)
                            .expect("Failed to read input");

                        // TODO: Make explicit instructions for Integer and String input.
                        let result = input.trim().parse::<i64>();

                        match result {
                            Ok(val) => self.push(Integer(val)),
                            Err(e) => {
                                todo!("For now, invalid input is a fatal error: {e}");
                            }
                        }
                    }
                    Input::File(_) => todo!(),
                    Input::Buffer(ref_cell) => {
                        let new_val = ref_cell
                            .borrow_mut()
                            .pop()
                            .expect("Not enough input in buffer")
                            .clone();
                        self.push(Cell::Integer(
                            new_val.try_into().expect("Couldn't transform u8 into i64?"),
                        ));
                    }
                }
            }
            FileRead => todo!(),
            FileWrite => todo!(),
        }

        Ok(())
    }

    fn evaluate_ifelse(
        &mut self,
        when_true: Rc<Instruction>,
        when_false: Rc<Instruction>,
    ) -> Result<()> {
        let branch = match self.cells.last() {
            Some(Integer(0)) => when_false,
            Some(Integer(_)) => when_true,
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
        };

        match self.run_nested(Rc::<[Instruction]>::from(vec![(*branch).clone()]))? {
            Some(val) => self.push(val),
            None => return Err(BlockHasEmptyStack),
        }

        Ok(())
    }
}

impl From<Vec<Cell>> for Executor {
    fn from(value: Vec<Cell>) -> Self {
        let mut machine = Self::new(Vec::<Instruction>::new());
        machine.cells = value;
        machine
    }
}

#[cfg(test)]
pub mod executor_tests;
