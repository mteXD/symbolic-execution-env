use std::{
    collections::HashMap,
    ops::{self, Add, BitAnd, Div, Mul},
};

use crate::{
    instruction::{
        BinaryOp, FunctionOp, Instruction, IntrinsicOp, NullaryOp, UnaryOpCell, UnaryOpImm,
    },
    machine::{
        CoreError::{self},
        CoreMachine,
    },
    types::{Address, Cell, CellIndex, FdEntry, FunctionDataError, Immediate, ProgramDataError},
};
use Cell::*;
use VerifierError::*;
use log::{debug, trace, warn};

#[derive(Debug, Clone)]
pub enum VerifierError {
    Core(CoreError),
    RebaseError,
    InvalidCell {
        instr: Instruction,
        cell_index: CellIndex,
        cells: Vec<ValueSpan>,
        prog: Vec<Instruction>,
    },
    ArithmeticOverflow,
    DivisionByZero,
    TypeError {
        expected: ValueSpan,
        found: ValueSpan,
    },
    NotEnoughCells {
        required: CellIndex,
        available: usize,
    },
    NotEnoughArguments {
        required: CellIndex,
        available: usize,
    },
    StackUnderflow,
    UnsafeCondPlacement,
    DebugError(&'static str),
}

impl From<CoreError> for VerifierError {
    fn from(e: CoreError) -> Self {
        VerifierError::Core(e)
    }
}

impl From<ProgramDataError> for VerifierError {
    fn from(e: ProgramDataError) -> Self {
        VerifierError::Core(e.into())
    }
}

impl From<FunctionDataError> for VerifierError {
    fn from(e: FunctionDataError) -> Self {
        VerifierError::Core(e.into())
    }
}

#[derive(Debug, Clone)]
enum MemorizedIndex {
    Normal(CellIndex),
    Reverse(CellIndex),
}

#[derive(Debug, Clone, Default)]
struct FunctionDefiningInfo {
    function_name: String,
    arg_positions: Vec<MemorizedIndex>,
}

impl FunctionDefiningInfo {
    fn required_arguments(&self) -> usize {
        self.arg_positions.len()
    }
}

#[derive(Debug, Clone, Default)]
struct Findings {
    values_after_rebase: Option<usize>,
    func_defining: Option<FunctionDefiningInfo>,
    func_data: HashMap<String, FunctionDefiningInfo>,
    processed_instructions: usize,
}

impl Findings {
    #[inline]
    fn is_collecting_func_args(&self) -> bool {
        self.func_defining.is_some() && self.values_after_rebase.is_none()
    }

    fn func_required_arguments(&self, func_name: &str) -> Option<usize> {
        self.func_data
            .get(func_name)
            .map(|info| info.required_arguments())
    }
}

enum Comparator {
    LessThan,
    Equal,
    GreaterThan,
    NotEqual,
    LessThanOrEqual,
    GreaterThanOrEqual,
}

struct ConvergenceInfo {
    critical_cell_index: CellIndex,
    critical_cell_value_span: ValueSpan,
    critical_ref_cell_index: CellIndex,
    critical_ref_cell_value_span: ValueSpan,
    comparator: Comparator,
    does_converge: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueSpan {
    pub min: Immediate,
    pub max: Immediate,
}

impl ValueSpan {
    fn new(min: Immediate, max: Immediate) -> Self {
        Self { min, max }
    }

    fn inf() -> Self {
        Self {
            min: Immediate::MIN,
            max: Immediate::MAX,
        }
    }
}

impl Add<Immediate> for ValueSpan {
    type Output = Self;

    fn add(self, rhs: Immediate) -> Self::Output {
        Self {
            min: self.min.checked_add(rhs).unwrap_or(Immediate::MIN),
            max: self.max.checked_add(rhs).unwrap_or(Immediate::MAX),
        }
    }
}

impl Add<ValueSpan> for ValueSpan {
    type Output = Self;

    fn add(self, rhs: ValueSpan) -> Self::Output {
        Self {
            min: self.min.checked_add(rhs.min).unwrap_or(Immediate::MIN),
            max: self.max.checked_add(rhs.max).unwrap_or(Immediate::MAX),
        }
    }
}

impl Mul<ValueSpan> for ValueSpan {
    type Output = Self;

    fn mul(self, rhs: ValueSpan) -> Self::Output {
        let candidates = [
            self.min.checked_mul(rhs.min),
            self.min.checked_mul(rhs.max),
            self.max.checked_mul(rhs.min),
            self.max.checked_mul(rhs.max),
        ];

        // PERF: Perhaps inefficient
        let min = candidates
            .iter()
            .filter_map(|&x| x)
            .min()
            .unwrap_or(Immediate::MIN);
        let max = candidates
            .iter()
            .filter_map(|&x| x)
            .max()
            .unwrap_or(Immediate::MAX);

        Self { min, max }
    }
}

impl Div<ValueSpan> for ValueSpan {
    type Output = Self;

    fn div(self, rhs: ValueSpan) -> Self::Output {
        if rhs.min <= 0 && rhs.max >= 0 {
            // Division by zero is possible, so we return the widest possible range.
            return Self::inf();
        }

        let candidates = [
            self.min.checked_div(rhs.min),
            self.min.checked_div(rhs.max),
            self.max.checked_div(rhs.min),
            self.max.checked_div(rhs.max),
        ];

        let min = candidates
            .iter()
            .filter_map(|&x| x)
            .min()
            .unwrap_or(Immediate::MIN);
        let max = candidates
            .iter()
            .filter_map(|&x| x)
            .max()
            .unwrap_or(Immediate::MAX);

        Self { min, max }
    }
}

// #[derive(Clone)]
pub struct Verifier<'a> {
    machine: CoreMachine<'a>,
    cells: Vec<ValueSpan>,
    base: usize,
    base_stack: Vec<usize>,
    findings: Findings,
}

impl<'a> Verifier<'a> {
    pub fn new(program: &'a [Instruction]) -> Self {
        Self {
            machine: CoreMachine::new(program),
            cells: Vec::new(),
            base: 0,
            base_stack: Vec::new(),
            findings: Findings::default(),
        }
    }

    pub fn sub_machine(&self, program: &'a [Instruction]) -> Self {
        Self {
            machine: CoreMachine::sub_machine(&self.machine, program),
            cells: self.cells.clone(),
            base: 0,
            base_stack: Vec::new(),
            findings: Findings::default(),
        }
    }

    pub fn check_len(&self, required: CellIndex) -> Result<(), VerifierError> {
        // TODO: When entering a block that's been re-based, check that there are enough cells for
        // operations performed inside. Make a unit test for this.
        if self.cells.len()
            < required
                .try_into()
                .expect("CellIndex value should fit into usize")
        {
            return Err(NotEnoughCells {
                required,
                available: self.cells.len(),
            });
        }

        Ok(())
    }

    pub fn push(&mut self, value: ValueSpan) {
        self.cells.push(value);
    }

    pub fn pop(&mut self) -> Option<ValueSpan> {
        self.cells.pop()
    }

    pub fn read(&self, reg: CellIndex) -> Result<Option<&ValueSpan>, VerifierError> {
        if self.findings.is_collecting_func_args() {
            return Result::Ok(None);
        }

        self.cells
            .get::<usize>(reg.into())
            .ok_or(InvalidCell {
                instr: self.machine.program_data.get_current()?.clone(),
                cell_index: reg,
                cells: self.cells.clone(),
                prog: self.machine.program_data.get_program().to_vec(),
            })
            .map(Some)
    }

    pub fn verify(&mut self) -> Result<Option<&ValueSpan>, VerifierError> {
        while let Some(instr) = self.machine.next() {
            self.verify_instruction(instr)?;
            self.findings.processed_instructions += 1;
        }

        Ok(self.cells.last())
    }

    fn verify_instruction(&mut self, instr: &Instruction) -> Result<(), VerifierError> {
        use Instruction::*;

        match instr {
            Block(_) => {}
            _ => debug!("Verifying instruction: {:#?}", instr),
        }

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

                let avail_values = self.cells.len();
                self.findings.values_after_rebase = Some(avail_values);
            }
            Cond => {
                use BinaryOp::*;
                use Comparator::*;
                use Instruction::AluBinary;

                /* First, check if the previous instr was a comparison instr.
                 * If not, warn that this is not really safe.
                 *
                 *  next,
                 */

                self.cells.pop().ok_or(StackUnderflow)?;

                let pc = match self.machine.program_data.get_pc() {
                    Address::Null => {
                        panic!("PC cannot be null here, program already started executing.")
                    }
                    Address::Value(0) => {
                        return Err(VerifierError::NotEnoughCells {
                            required: 1,
                            available: 0,
                        });
                    }
                    Address::Value(v) => v - 1,
                };

                let i = self.machine.program_data.get_at(Address::Value(pc))?;

                let comp_info = match i {
                    AluBinary(SetEqual, _, _) => Some(Equal),
                    AluBinary(SetNotEqual, _, _) => Some(NotEqual),
                    AluBinary(SetLessThan, _, _) => Some(LessThan),
                    AluBinary(SetLessThanOrEqual, _, _) => Some(LessThanOrEqual),
                    AluBinary(SetGreaterThan, _, _) => Some(GreaterThan),
                    AluBinary(SetGreaterThanOrEqual, _, _) => Some(GreaterThanOrEqual),
                    _ => {
                        warn!(
                            "Condition instruction not preceded by a comparison instruction. This is unsafe."
                        );
                        // None
                        return Err(VerifierError::UnsafeCondPlacement);
                    }
                };
            }
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
            Push => self.push(ValueSpan::new(arg, arg)),
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

                match val {
                    Some(ValueSpan { min, max }) if min == max => {
                        self.push(ValueSpan::new(!*min, !*max));
                    }
                    Some(_) => {
                        self.push(ValueSpan::inf());
                    }
                    None => {
                        // When collecting function arguments
                        match self.findings.func_defining.as_mut() {
                            Some(func_info) => {
                                func_info.arg_positions.push(MemorizedIndex::Normal(arg));
                            }
                            None => panic!(
                                "We got None, which can only happen when collecting function arguments."
                            ),
                        }
                    }
                }
            }
            Read => {
                let val = self.read(arg)?;

                match val {
                    Some(v) => self.push(v.clone()),
                    None => {
                        // When collecting function arguments
                        match self.findings.func_defining.as_mut() {
                            Some(func_info) => {
                                func_info.arg_positions.push(MemorizedIndex::Normal(arg));
                            }
                            None => panic!(
                                "We got None, which can only happen when collecting function arguments."
                            ),
                        }
                    }
                }
            }
            ReadReverse => {
                debug!("ReadReverse with arg: {}", arg);
                match self.findings.func_defining.as_mut() {
                    Some(func_info) => {
                        trace!(
                            "Collecting argument for function '{}'",
                            func_info.function_name
                        );
                        func_info.arg_positions.push(MemorizedIndex::Reverse(arg));
                        self.push(ValueSpan::inf());
                    }
                    None => {
                        trace!(
                            "Not collecting function arguments, performing normal read with reverse indexing."
                        );
                        // like python's negative indexing.
                        let index = u16::try_from(self.cells.len())
                            .ok()
                            .and_then(|len| len.checked_sub(1))
                            .and_then(|len| len.checked_sub(arg))
                            .ok_or(InvalidCell {
                                instr: self.machine.program_data.get_current()?.clone(),
                                cell_index: arg,
                                cells: self.cells.clone(),
                                prog: self.machine.program_data.get_program().to_vec(),
                            })?;

                        let val = self.read(index)?;

                        match val {
                            Some(v) => self.push(v.clone()),
                            None => {
                                panic!(
                                    "We got None, which can only happen when collecting function arguments."
                                )
                            }
                        }
                    }
                }
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

        if let (Some(a), Some(b)) = (a, b) {
            let a = *a;
            let b = *b;
            let calculated_value = match instr {
                Add => a + b,
                Mul => a * b,
                Div => {
                    if b == ValueSpan::new(0, 0) {
                        return Err(DivisionByZero);
                    }
                    a / b
                }
                And | Or | Xor | ShiftLeftLogical | ShiftRightLogical | ShiftRightArithmetic => {
                    ValueSpan::inf()
                }
                SetEqual
                | SetNotEqual
                | SetLessThan
                | SetLessThanOrEqual
                | SetGreaterThan
                | SetGreaterThanOrEqual => ValueSpan::new(0, 1),
            };

            self.push(calculated_value);
        }

        Ok(())
    }

    fn verify_block(&mut self, instrs: &[Instruction]) -> Result<(), VerifierError> {
        let mut block_verifier = self.sub_machine(instrs);
        block_verifier.base_stack.push(self.base);
        block_verifier.base = self.cells.len();
        block_verifier.findings = self.findings.clone(); // PERF: clone()

        let block_result = block_verifier.verify()?;

        // WARN: What if this block returns "void"? Add this to checker.
        if let Some(val) = block_result {
            self.push(val.clone());
        }

        self.base = block_verifier.base_stack.pop().ok_or(RebaseError)?.clone();

        Ok(())
    }

    fn verify_function(&mut self, instr: &FunctionOp, arg: &str) -> Result<(), VerifierError> {
        use FunctionDataError::FunctionUndefined;
        use FunctionOp::*;

        match instr {
            FunctionDefine => {
                self.machine.common_function_logic(arg)?;

                // Shadowing will not be permitted, as compilers could generate function names
                // easily and we can avoid complexity and ambiguity this way.
                let current_instr = &self.machine.program_data.get_current()?.clone();
                let mut function_verifier = self.sub_machine(std::slice::from_ref(current_instr));
                function_verifier.findings.func_defining = Some(FunctionDefiningInfo {
                    function_name: arg.to_owned(),
                    arg_positions: Vec::new(),
                });
                function_verifier.findings.func_data = self.findings.func_data.clone(); // PERF: clone()
                function_verifier.findings.func_data.insert(
                    arg.to_owned(),
                    FunctionDefiningInfo {
                        function_name: arg.to_owned(),
                        arg_positions: Vec::new(),
                    },
                );
                function_verifier.verify()?;

                // PERF: For now, iterate over the whole hashmap and find the keys that have the
                // current function as the value.
                for (k, v) in &function_verifier.machine.function_data.function_table {
                    if let FdEntry::Str(s) = v {
                        self.findings.func_data.insert(
                            k.to_owned(),
                            FunctionDefiningInfo {
                                function_name: k.to_owned(),
                                arg_positions: function_verifier
                                    .findings
                                    .func_defining
                                    .as_ref()
                                    .expect("FunctionDefiningInfo should be set during function verification.")
                                    .arg_positions
                                    .clone(), // PERF: clone()
                            },
                        );
                    }
                }

                self.findings.func_data.insert(
                    arg.to_owned(),
                    function_verifier
                        .findings
                        .func_defining
                        .expect("FunctionDefiningInfo should be set during function verification."),
                );
            }
            FunctionCall => {
                self.machine.function_get(&arg)?;

                let required_args = self
                    .findings
                    .func_required_arguments(arg)
                    .ok_or(FunctionUndefined(arg.to_owned()))?;
                let available_args = self.cells.len();

                if available_args < required_args {
                    return Err(NotEnoughArguments {
                        required: required_args
                            .try_into()
                            .expect("Required arguments should fit into CellIndex"),
                        available: available_args,
                    });
                }

                self.push(ValueSpan::inf());

                // TODO: Check for infinite recursion.
            }
        }

        Ok(())
    }

    fn verify_intrinsic(
        &mut self,
        instr: &IntrinsicOp,
        _arg: CellIndex,
    ) -> Result<(), VerifierError> {
        use IntrinsicOp::*;

        match instr {
            Print => todo!(),
            Input => todo!(),
            FileRead => todo!(),
            FileWrite => todo!(),
        }

        Ok(())
    }
}

#[cfg(test)]
pub mod verifier_tests;
