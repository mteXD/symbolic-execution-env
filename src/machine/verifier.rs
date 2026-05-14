use crate::{
    instruction::{
        BinaryOp, FunctionOp, Instruction, IntrinsicOp, NullaryOp, UnaryOpCell, UnaryOpImm,
    },
    machine::{
        CoreError::{self},
        CoreMachine,
    },
    types::{Cell, CellIndex, Immediate},
};
use Cell::*;
use VerifierError::*;
use log::warn;

#[derive(Debug, Clone)]
pub enum VerifierError {
    Core(CoreError),
    RebaseError,
    InvalidCell,
    ArithmeticOverflow,
    DivisionByZero,
    TypeError {
        expected: Cell,
        found: Cell,
    },
    NotEnoughCells {
        required: CellIndex,
        available: usize,
    },
    StackUnderflow,
}

impl From<CoreError> for VerifierError {
    fn from(e: CoreError) -> Self {
        VerifierError::Core(e)
    }
}

// #[derive(Clone)]
pub struct Verifier<'a> {
    machine: CoreMachine<'a>,
    cells: Vec<Cell>,
    base: usize,
    base_stack: Vec<usize>,
}

impl<'a> Verifier<'a> {
    pub fn new(program: &'a [Instruction]) -> Self {
        Self {
            machine: CoreMachine::new(program),
            cells: Vec::new(),
            base: 0,
            base_stack: Vec::new(),
        }
    }

    pub fn sub_machine(&self, program: &'a [Instruction]) -> Self {
        Self {
            machine: CoreMachine::sub_machine(&self.machine, program),
            cells: self.cells.clone(),
            base: 0,
            base_stack: Vec::new(),
        }
    }

    pub fn check_len(&self, required: CellIndex) -> Result<(), VerifierError> {
        // TODO: When entering a block that's been re-based, check that there are enough cells for
        // operations performed inside. Make a unit test for this.
        if self.cells.len()
            < required
                .try_into()
                .expect("Cell value should fit into usize")
        {
            return Err(NotEnoughCells {
                required,
                available: self.cells.len(),
            });
        }

        Ok(())
    }

    pub fn push(&mut self, value: Cell) {
        self.cells.push(value);
    }

    pub fn pop(&mut self) -> Option<Cell> {
        self.cells.pop()
    }

    pub fn read(&self, reg: CellIndex) -> Result<&Cell, VerifierError> {
        self.cells.get::<usize>(reg.into()).ok_or(InvalidCell)
    }

    pub fn verify(&mut self) -> Result<Option<&Cell>, VerifierError> {
        while let Some(instr) = self.machine.next() {
            self.verify_instruction(instr)?
        }

        Ok(self.cells.last())
    }

    fn verify_instruction(&mut self, instr: &Instruction) -> Result<(), VerifierError> {
        use Instruction::*;

        match instr {
            AluNullary(instr) => self.verify_alu_nullary(instr),
            AluUnaryImm(instr, imm) => self.verify_alu_unary_imm(instr, *imm),
            AluUnaryCell(instr, cell) => self.verify_alu_unary_cell(instr, *cell),
            AluBinary(instr, arg1, arg2) => self.verify_alu_binary(instr, *arg1, *arg2),
            Block(instrs) => self.verify_block(instrs),
            AluFunction(instr, fun) => self.verify_function(instr, fun),
            AluIntrinsic(instr, arg) => self.verify_intrinsic(instr, *arg),
        }?;

        Ok(())
    }

    fn verify_alu_nullary(&mut self, instr: &NullaryOp) -> Result<(), VerifierError> {
        use NullaryOp::*;

        match instr {
            Nop => (),
            Rebase => {
                if self.base > self.cells.len() {
                    return Err(RebaseError);
                }

                self.cells = self.cells.split_off(self.base);
            }
            Cond => todo!(),
        }

        Ok(())
    }

    fn verify_alu_unary_imm(
        &mut self,
        instr: &UnaryOpImm,
        arg: Immediate,
    ) -> Result<(), VerifierError> {
        use UnaryOpImm::*;

        match instr {
            Push => self.push(Integer(arg)),
        }

        Ok(())
    }

    fn verify_alu_unary_cell(
        &mut self,
        instr: &UnaryOpCell,
        arg: CellIndex,
    ) -> Result<(), VerifierError> {
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
                self.push(*val);
            }
            ReadReverse => {
                // like python's negative indexing.
                let index = u16::try_from(self.cells.len())
                    .ok()
                    .and_then(|len| len.checked_sub(1))
                    .and_then(|len| len.checked_sub(arg))
                    .ok_or(InvalidCell)?;
                let val = *self.read(index)?;
                self.push(val);
            }
            Pop => {
                for _ in 0..arg {
                    self.pop().ok_or(StackUnderflow)?;
                }
            }
        }

        Ok(())
    }

    fn verify_alu_binary(
        &mut self,
        instr: &BinaryOp,
        arg1: CellIndex,
        arg2: CellIndex,
    ) -> Result<(), VerifierError> {
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
                found: if let Integer(_) = a {
                    b.clone()
                } else if let Integer(_) = b {
                    a.clone()
                } else {
                    panic!("Both operands should be integers, but neither is.")
                },
            });
        }

        Ok(())
    }

    fn verify_block(&mut self, instrs: &[Instruction]) -> Result<(), VerifierError> {
        let mut block_verifier = self.sub_machine(instrs);
        block_verifier.base_stack.push(self.base);
        block_verifier.base = self.cells.len();

        let block_result = block_verifier.verify()?;

        // WARN: What if this block returns "void"? Add this to checker.
        if let Some(val) = block_result {
            self.push(val.clone());
        }

        self.base = block_verifier.base_stack.pop().ok_or(RebaseError)?.clone();

        Ok(())
    }

    fn verify_function(&mut self, instr: &FunctionOp, arg: &str) -> Result<(), VerifierError> {
        use FunctionOp::*;

        match instr {
            FunctionDefine => {
                self.machine.common_function_logic(arg)?;
            }
            FunctionCall => {
                self.machine.function_get(&arg)?;

                // TODO: Check for infinite recursion.
            }
        }

        Ok(())
    }

    fn verify_intrinsic(
        &mut self,
        instr: &IntrinsicOp,
        arg: CellIndex,
    ) -> Result<(), VerifierError> {
        use IntrinsicOp::*;

        match instr {
            Print => {
                let val = self.read(arg)?;
                print!("{val}");
            }
            Input => {
                let mut input: String = String::new();
                std::io::stdin()
                    .read_line(&mut input)
                    .expect("Failed to read input");

                // TODO: Make explicit instructions for Integer and String input.
                let result = input.trim().parse::<i64>();

                match result {
                    Ok(val) => self.push(Integer(val)),
                    Err(e) => {
                        // TODO: Make explicit instructions for Integer and String input.
                        warn!(
                            "Failed to parse input as integer: {e}. Pushing input as string instead."
                        );
                        input.chars().for_each(|c| self.push(Text(c)))
                    }
                }
            }
            FileRead => todo!(),
            FileWrite => todo!(),
        }

        Ok(())
    }
}

#[cfg(test)]
pub mod verifier_tests;
