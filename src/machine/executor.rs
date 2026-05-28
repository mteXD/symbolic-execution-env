use super::*;
use crate::{
    instruction::{
        BinaryOp, FunctionOp, Instruction, IntrinsicOp, NullaryOp, UnaryOpCell, UnaryOpImm,
    },
    machine::{CoreError, CoreMachine},
    types::{self, Cell, CellIndex, Immediate},
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

const RECURSION_LIMIT: usize = 5;

pub struct Executor<'a> {
    machine: CoreMachine<'a>,
    cells: Vec<Cell>,
    pub base: usize, // The index in `cells` where the current block/function's cells start.
    pub base_stack: Vec<usize>,
    recursion_depth: usize,
}

impl<'a> Executor<'a> {
    pub fn new(program: &'a [Instruction]) -> Self {
        Self {
            machine: CoreMachine::new(program),
            cells: Vec::new(),
            base: 0,
            base_stack: Vec::new(),
            recursion_depth: 0,
        }
    }

    fn sub_machine(&self, program: &'a [Instruction]) -> Self {
        // TODO: Optimize cells cloning (use Rc?)
        Self {
            machine: CoreMachine::sub_machine(&self.machine, program),
            cells: self.cells.clone(),
            base: 0,
            base_stack: Vec::new(),
            recursion_depth: 0,
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

    pub fn pop(&mut self) -> Option<Cell> {
        self.cells.pop()
    }

    pub fn read(&self, reg: CellIndex) -> Result<&Cell> {
        self.cells.get::<usize>(reg.into()).ok_or(InvalidCell)
    }

    pub fn exec(&mut self) -> Result<Option<&Cell>> {
        while let Some(instr) = self.machine.next() {
            self.evaluate_instruction(instr)?;
        }

        Ok(self.cells.last())
    }
}

impl Evaluate for Executor<'_> {
    type Error = ExecutorError;

    fn evaluate_alu_nullary(&mut self, instr: &NullaryOp) -> Result<()> {
        use NullaryOp::*;

        match instr {
            Nop => (),
            Rebase => {
                if self.base > self.cells.len() {
                    return Err(RebaseError);
                }

                self.cells = self.cells.split_off(self.base);
            } // Cond => match self.pop() {
              // },
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

    fn evaluate_block(&mut self, instrs: &[Instruction]) -> Result<()> {
        /* NOTE:
         * Since it is likely that more pops than pushes occur, we must
         * save the ENTIRE state of cells, copying it twice.
         */

        let mut block_executor = self.sub_machine(instrs);
        block_executor.base_stack.push(self.base);
        block_executor.base = self.cells.len();

        // We recognize an empty stack of the block_verifier as an error.
        // This way, each block is guaranteed to leave at least one value on the stack, and user can
        // then discard this value, producing a "void" block.
        match block_executor.exec()?.cloned() {
            Some(val) => self.push(val.clone()),
            None => return Err(BlockHasEmptyStack),
        }

        self.base = block_executor.base_stack.pop().ok_or(RebaseError)?.clone();

        Ok(())
    }

    fn evaluate_function(&mut self, instr: &FunctionOp, fun: &String) -> Result<()> {
        use FunctionOp::*;

        match instr {
            FunctionDefine => self.machine.common_function_logic(fun)?,
            FunctionCall => {
                let instr = self.machine.function_get(&fun).map(std::slice::from_ref)?;

                let mut function_self = self.sub_machine(instr);
                function_self.recursion_depth = self.recursion_depth + 1;
                if function_self.recursion_depth > RECURSION_LIMIT {
                    panic!("Recursion limit of {RECURSION_LIMIT} exceeded in function '{fun}'");
                }
                let function_result = function_self.exec()?;

                if let Some(val) = function_result {
                    self.push(*val);
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

        let mut machine = self.sub_machine(std::slice::from_ref(&branch));
        machine.base_stack.push(self.base);
        machine.base = self.cells.len();

        match machine.exec()?.cloned() {
            Some(val) => self.push(val.clone()),
            None => return Err(BlockHasEmptyStack),
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
