use std::{
    collections::HashMap,
    ops::{Add, Div, Mul},
    rc::Rc,
};

use crate::{
    instruction::{
        BinaryOp, FunctionOp, Instruction, IntrinsicOp, NullaryOp, UnaryOpCell, UnaryOpImm,
    },
    machine::{
        CoreError::{self},
        CoreMachine, Evaluate,
    },
    types::{self, Address, CellIndex, FdEntry, FunctionDataError, Immediate, ProgramDataError},
};
use VerifierError::*;
use log::{debug, error, trace, warn};

#[derive(Debug, Clone)]
pub enum VerifierError {
    Core(CoreError),
    RebaseError,
    InvalidCell {
        instr: Instruction,
        cell_index: CellIndex,
        cells: Vec<ValueSpan>,
        prog: Vec<Instruction>,
        location: &'static str,
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
    InfiniteRecursion {
        function_name: &'static str,
    },
    CondInvalidCell {
        instr: Instruction,
        cell_index: CellIndex,
        cells: Vec<ValueSpan>,
        prog: Vec<Instruction>,
        location: &'static str,
    },
    CondUnequalStackSizes {
        true_branch_cells: usize,
        false_branch_cells: usize,
    },
    BlockHasEmptyStack,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueSpan {
    pub min: Immediate,
    pub max: Immediate,
}

impl ValueSpan {
    fn new(min: Immediate, max: Immediate) -> Self {
        if min > max {
            panic!(
                "ValueSpan cannot have min greater than max. Got min: {}, max: {}",
                min, max
            );
        }
        Self { min, max }
    }

    fn size(&self) -> u128 {
        (self.max as u128)
            .saturating_sub(self.min as u128)
            .saturating_add(1)
    }

    fn smaller_than(&self, other: &ValueSpan) -> bool {
        self.size() < other.size()
    }

    fn inf() -> Self {
        Self {
            min: Immediate::MIN,
            max: Immediate::MAX,
        }
    }

    fn includes_zero(&self) -> bool {
        self.min <= 0 && self.max >= 0
    }

    fn disjunct(&self, other: &ValueSpan) -> bool {
        self.max < other.min || other.max < self.min
    }

    fn is_single_value(&self) -> bool {
        self.min == self.max
    }

    fn combine(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    fn chck_eq(&self, other: &Self) -> Self {
        if self.is_single_value() && other.is_single_value() {
            if self.min == other.min {
                ValueSpan::new(1, 1)
            } else {
                ValueSpan::new(0, 0)
            }
        } else if self.disjunct(&other) {
            ValueSpan::new(0, 0)
        } else {
            ValueSpan::new(0, 1)
        }
    }

    fn chck_neq(&self, other: &Self) -> Self {
        if self.is_single_value() && other.is_single_value() {
            if self.min != other.min {
                ValueSpan::new(1, 1)
            } else {
                ValueSpan::new(0, 0)
            }
        } else if self.disjunct(other) {
            ValueSpan::new(1, 1)
        } else {
            ValueSpan::new(0, 1)
        }
    }

    fn chck_lt(&self, other: &Self) -> Self {
        if self.max < other.min {
            ValueSpan::new(1, 1)
        } else if self.min >= other.max {
            ValueSpan::new(0, 0)
        } else {
            ValueSpan::new(0, 1)
        }
    }

    fn chck_gt(&self, other: &Self) -> Self {
        if self.min > other.max {
            ValueSpan::new(1, 1)
        } else if self.max <= other.min {
            ValueSpan::new(0, 0)
        } else {
            ValueSpan::new(0, 1)
        }
    }

    fn chck_lte(&self, other: &Self) -> Self {
        if self.max <= other.min {
            ValueSpan::new(1, 1)
        } else if self.min > other.max {
            ValueSpan::new(0, 0)
        } else {
            ValueSpan::new(0, 1)
        }
    }

    fn chck_gte(&self, other: &Self) -> Self {
        if self.min >= other.max {
            ValueSpan::new(1, 1)
        } else if self.max < other.min {
            ValueSpan::new(0, 0)
        } else {
            ValueSpan::new(0, 1)
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
    is_conditional: bool,
    cond_machine_depth: Option<usize>,
}

impl Findings {
    #[inline]
    fn is_collecting_func_args(&self) -> bool {
        self.func_defining.is_some() && self.values_after_rebase.is_none()
    }

    fn inc_cond_depth(&mut self) {
        self.cond_machine_depth = match self.cond_machine_depth {
            Some(depth) => Some(depth + 1),
            None => Some(0),
        };
    }

    fn func_required_arguments(&self, func_name: &str) -> Option<usize> {
        self.func_data
            .get(func_name)
            .map(|info| info.required_arguments())
    }
}

#[derive(Clone)]
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

    fn new_cond_machine(&self) -> Verifier<'_> {
        let mut cond_machine = self.clone();
        cond_machine.findings.is_conditional = true;
        cond_machine.findings.inc_cond_depth();
        cond_machine
    }

    pub fn redirect_input(&mut self, new_input: types::Input) {
        self.machine.input = new_input;
    }

    pub fn redirect_output(&mut self, new_output: types::Output) {
        self.machine.output = new_output;
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
                location: "Verifier::read, get()",
            })
            .map(Some)
    }

    fn get_prev_instr(&self) -> Result<&Instruction, VerifierError> {
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
        self.machine
            .program_data
            .get_at(Address::Value(pc))
            .map_err(Into::into)
    }

    pub fn verify(&mut self) -> Result<Option<&ValueSpan>, VerifierError> {
        while let Some(instr) = self.machine.next() {
            self.evaluate_instruction(instr)?;
            self.findings.processed_instructions += 1;
        }

        Ok(self.cells.last())
    }

    fn check_good_if_placement(&self) -> Result<(), VerifierError> {
        use BinaryOp::*;
        use Instruction::AluBinary;

        let throw_err = || {
            error!(
                "Condition instruction not preceded by a comparison instruction. This is unsafe."
            );
            return Err(VerifierError::UnsafeCondPlacement);
        };

        match self.get_prev_instr()? {
            AluBinary(cmp, _, _) => {
                match cmp {
                    SetNotEqual
                    | SetLessThan
                    | SetLessThanOrEqual
                    | SetGreaterThan
                    | SetGreaterThanOrEqual => (),
                    _ => return throw_err(),
                };
            }
            _ => {
                return throw_err();
            }
        };
        Ok(())
    }

    fn check_unnecessary_if(last: &ValueSpan) -> Option<bool> {
        if last.is_single_value() {
            if last.min == 0 {
                warn!("Condition will always be false, skipping the next instruction.");
                Some(false)
            } else {
                warn!("Condition will always be true, executing the next instruction.");
                Some(true)
            }
        } else {
            None
        }
    }
}

impl Evaluate for Verifier<'_> {
    type Error = VerifierError;

    fn evaluate_alu_nullary(&mut self, instr: &NullaryOp) -> Result<(), Self::Error> {
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
        }

        Ok(())
    }

    fn evaluate_alu_unary_imm(
        &mut self,
        instr: &UnaryOpImm,
        arg: Immediate,
    ) -> Result<(), Self::Error> {
        use UnaryOpImm::*;

        match instr {
            Push => self.push(ValueSpan::new(arg, arg)),
        }

        Ok(())
    }

    fn evaluate_alu_unary_cell(
        &mut self,
        instr: &UnaryOpCell,
        arg: CellIndex,
    ) -> Result<(), Self::Error> {
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
                                "This case cannot happen, as `func_defining` must be Some if `val` is None"
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
                                "This case cannot happen, as `func_defining` must be Some if `val` is None"
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
                                location: "Verifier::verify_alu_unary_cell, calculating reverse index",
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

    fn evaluate_alu_binary(
        &mut self,
        instr: &BinaryOp,
        arg1: CellIndex,
        arg2: CellIndex,
    ) -> Result<(), Self::Error> {
        use BinaryOp::*;

        fn from_bool<T: From<bool>>(value: bool) -> T {
            value.into()
        }

        let a = self.read(arg1)?;
        let b = self.read(arg2)?;

        if let (Some(&a), Some(&b)) = (a, b) {
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
                SetEqual => a.chck_eq(&b),
                SetNotEqual => a.chck_neq(&b),
                SetLessThan => a.chck_lt(&b),
                SetLessThanOrEqual => a.chck_lte(&b),
                SetGreaterThan => a.chck_gt(&b),
                SetGreaterThanOrEqual => a.chck_gte(&b),
            };

            self.push(calculated_value);
        }

        Ok(())
    }

    fn evaluate_block(&mut self, instrs: &[Instruction]) -> Result<(), Self::Error> {
        let mut block_verifier = self.sub_machine(instrs);
        block_verifier.base_stack.push(self.base);
        block_verifier.base = self.cells.len();
        block_verifier.findings = self.findings.clone(); // PERF: clone()

        // We recognize an empty stack of the block_verifier as an error.
        // This way, each block is guaranteed to leave at least one value on the stack, and user can
        // then discard this value, producing a "void" block.
        match block_verifier.verify()?.cloned() {
            Some(val) => self.push(val.clone()),
            None => return Err(BlockHasEmptyStack),
        }

        self.base = block_verifier.base_stack.pop().ok_or(RebaseError)?.clone();

        Ok(())
    }

    fn evaluate_function(&mut self, instr: &FunctionOp, fun: &String) -> Result<(), Self::Error> {
        use FunctionDataError::FunctionUndefined;
        use FunctionOp::*;

        match instr {
            FunctionDefine => {
                self.machine.common_function_logic(fun)?;

                // Shadowing will not be permitted, as compilers could generate function names
                // easily and we can avoid complexity and ambiguity this way.
                let current_instr = &self.machine.program_data.get_current()?.clone();
                let mut function_verifier = self.sub_machine(std::slice::from_ref(current_instr));
                function_verifier.findings.func_defining = Some(FunctionDefiningInfo {
                    function_name: fun.to_owned(),
                    arg_positions: Vec::new(),
                });
                function_verifier.findings.func_data = self.findings.func_data.clone(); // PERF: clone()
                function_verifier.findings.func_data.insert(
                    fun.to_owned(),
                    FunctionDefiningInfo {
                        function_name: fun.to_owned(),
                        arg_positions: Vec::new(),
                    },
                );
                function_verifier.verify()?;

                // PERF: For now, iterate over the whole hashmap and find the keys that have the
                // current function as the value.
                for (k, v) in &function_verifier.machine.function_data.function_table {
                    if let FdEntry::Str(s) = v {
                        if s == fun {
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
                                        .clone(),
                                },
                            );
                        }
                    }
                }

                self.findings.func_data.insert(
                    fun.to_owned(),
                    function_verifier
                        .findings
                        .func_defining
                        .expect("FunctionDefiningInfo should be set during function verification."),
                );
            }
            FunctionCall => {
                self.machine.function_get(&fun)?;

                // Recursion check
                // check if

                // Argument check
                let required_args = self
                    .findings
                    .func_required_arguments(fun)
                    .ok_or(FunctionUndefined(fun.to_owned()))?;
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
            }
        }

        Ok(())
    }

    fn evaluate_intrinsic(&mut self, instr: &IntrinsicOp, _: CellIndex) -> Result<(), Self::Error> {
        use IntrinsicOp::*;

        match instr {
            Print => (),
            Input => self.push(ValueSpan::inf()),
            FileRead => self.push(ValueSpan::inf()),
            FileWrite => todo!(),
        }

        Ok(())
    }

    fn evaluate_ifelse(
        &mut self,
        when_true: Rc<Instruction>,
        when_false: Rc<Instruction>,
    ) -> Result<(), Self::Error> {
        /* First, check if the previous instr was a comparison instr.
        If not, warn that this is not really safe. */

        let condition = self.cells.last().ok_or(StackUnderflow)?;
        self.check_good_if_placement()?;
        let value = Self::check_unnecessary_if(condition);
        let ifelse_result = match value {
            Some(branch) => {
                let when_instr = if branch { when_true } else { when_false };
                let mut machine = self.sub_machine(std::slice::from_ref(&when_instr));
                machine.findings = self.findings.clone();
                *machine.verify()?.ok_or(BlockHasEmptyStack)?
            }
            None => {
                let mut true_machine = self.sub_machine(std::slice::from_ref(&when_true));
                true_machine.findings = self.findings.clone();
                let val1 = *true_machine.verify()?.ok_or(BlockHasEmptyStack)?;

                let mut false_machine = self.sub_machine(std::slice::from_ref(&when_false));
                false_machine.findings = self.findings.clone();
                let val2 = *false_machine.verify()?.ok_or(BlockHasEmptyStack)?;

                let true_machine_size = true_machine.cells.len();
                let false_machine_size = false_machine.cells.len();
                if true_machine_size != false_machine_size {
                    return Err(CondUnequalStackSizes {
                        true_branch_cells: true_machine_size,
                        false_branch_cells: false_machine_size,
                    });
                }

                val1.combine(val2)
            }
        };

        self.push(ifelse_result);
        debug!("Finished verifying ifelse, pushing result: {:?}", ifelse_result);

        self.findings.is_conditional = true;

        Ok(())
    }
}

#[cfg(test)]
pub mod verifier_tests;
