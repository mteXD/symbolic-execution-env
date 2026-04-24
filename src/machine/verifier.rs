use crate::{
    instruction::{BinaryOp, FunctionOp, Instruction, NullaryOp, UnaryOpCell, UnaryOpImm},
    machine::{
        CoreError::{self, *},
        CoreMachine,
    },
    types::{Cell, Immediate},
};
use VerifierError::*;

#[derive(Debug, Clone)]
pub enum VerifierError {
    Core(CoreError),
    FunctionUndefined,
    FunctionRedefinition,
    NotEnoughCells { required: Cell, available: Cell },
}

// #[derive(Clone)]
pub struct Verifier<'a> {
    machine: CoreMachine<'a>,
    cell_count: Cell,
    block_cells: Cell,
}

impl<'a> Verifier<'a> {
    pub fn new(program: &'a [Instruction]) -> Self {
        Self {
            machine: CoreMachine::new(program),
            cell_count: 0,
            block_cells: 0,
        }
    }

    pub fn check_len(&self, required: Cell) -> Result<(), VerifierError> {
        // TODO: When entering a block that's been re-based, check that there are enough cells for
        // operations performed inside. Make a unit test for this.
        if self.cell_count < required {
            return Err(NotEnoughCells {
                required,
                available: self.cell_count,
            });
        }

        Ok(())
    }

    pub fn add_cells(&mut self, count: Cell) {
        self.cell_count += count;
        self.block_cells += count;
    }

    pub fn rm_cells(&mut self, count: Cell) -> Result<(), VerifierError> {
        if self.cell_count < count {
            return Err(NotEnoughCells {
                required: count,
                available: self.cell_count,
            });
        }

        self.cell_count -= count;
        Ok(())
    }

    pub fn verify(&mut self) -> Result<(), VerifierError> {
        while let Some(instr) = self.machine.next() {
            self.verify_instruction(instr)?
        }

        Ok(())
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
        }?;

        Ok(())
    }

    fn verify_alu_nullary(&mut self, instr: &NullaryOp) -> Result<(), VerifierError> {
        use NullaryOp::*;

        match instr {
            Nop => (),
            Rebase => self.block_cells = 0,
            Cond => todo!(),
        }

        Ok(())
    }

    fn verify_alu_unary_imm(
        &mut self,
        instr: &UnaryOpImm,
        _: Immediate,
    ) -> Result<(), VerifierError> {
        use UnaryOpImm::*;

        match instr {
            Push => self.add_cells(1),
        }

        Ok(())
    }

    fn verify_alu_unary_cell(
        &mut self,
        instr: &UnaryOpCell,
        arg: Cell,
    ) -> Result<(), VerifierError> {
        use UnaryOpCell::*;

        let required_len = match instr {
            Not => 1,
            Read | ReadReverse => arg + 1,
            Pop => arg,
            Tail => todo!(),
        };

        self.check_len(required_len)?;

        match instr {
            Not | Read | ReadReverse => self.add_cells(1),
            Pop => self.rm_cells(arg)?,
            Tail => todo!(),
        }

        Ok(())
    }

    fn verify_alu_binary(
        &mut self,
        _instr: &BinaryOp,
        _arg1: Cell,
        _arg2: Cell,
    ) -> Result<(), VerifierError> {
        use BinaryOp::*;

        // match instr {
        //     Add => todo!(),
        //     Mul => todo!(),
        //     Div => todo!(),
        //     And => todo!(),
        //     Or => todo!(),
        //     Xor => todo!(),
        //     ShiftLeftLogical => todo!(),
        //     ShiftRightLogical => todo!(),
        //     ShiftRightArithmetic => todo!(),
        //     SetEqual => todo!(),
        //     SetNotEqual => todo!(),
        //     SetLessThan => todo!(),
        //     SetLessThanOrEqual => todo!(),
        //     SetGreaterThan => todo!(),
        //     SetGreaterThanOrEqual => todo!(),
        // }

        self.check_len(2)?;
        self.add_cells(1);

        Ok(())
    }

    fn verify_block(&mut self, instrs: &[Instruction]) -> Result<(), VerifierError> {
        let mut block_verificator = Verifier::new(instrs);
        block_verificator.cell_count = self.cell_count;
        // Don't copy block_cells since we're starting fresh for this block
        
        block_verificator.verify()?;

        Ok(())
    }

    fn verify_function(&mut self, instr: &FunctionOp, arg: &str) -> Result<(), VerifierError> {
        use FunctionOp::*;

        match instr {
            FunctionDefine => {
                if self.machine.function_exists(&arg) {
                    return Err(VerifierError::FunctionRedefinition);
                }

                let mut definitions = Vec::new();
                definitions.push(arg);

                // Handles fallthrough to function body, which is the next non-fuction-defining
                // instruction.
                while let Some(Instruction::AluFunction(FunctionOp::FunctionDefine, name)) =
                    self.machine.next()
                {
                    definitions.push(name);
                }

                let instruction = self
                    .machine
                    .get_current_instruction()
                    .map(std::slice::from_ref)
                    .ok_or(VerifierError::FunctionUndefined)?;

                definitions
                    .iter()
                    .map(|name| {
                        self.machine
                            .function_insert(String::from(*name), instruction);
                    })
                    .for_each(drop);
            }
            FunctionCall => {
                if !self.machine.function_exists(&arg) {
                    return Err(VerifierError::FunctionUndefined);
                }

                // TODO: Check for infinite recursion.
            }
        }

        Ok(())
    }

    // pub fn verify(&mut self) -> Result<(), VerifierError> {
    //     // use Instruction::*;
    //
    //     while let Some(instr) = self.program_data.next() {
    //         instr.check(self).map_err(|e| {
    //             eprintln!(
    //                 "Error verifying instruction {:?}. Error: {:?} | cells: {:?}",
    //                 instr, e, self.cell_count
    //             );
    //             e
    //         })?;
    //     }
    //
    //     Ok(())
    // }
}
