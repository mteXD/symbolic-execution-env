use std::{
    collections::HashMap,
    ops::{Add, Deref, DerefMut, Div, Mul},
    rc::Rc,
};

use crate::{
    instruction::{
        BinaryOp, FunctionOp, Instruction, IntrinsicOp, NullaryOp, UnaryOpCell, UnaryOpImm,
    },
    machine::{
        CoreError::{self},
        CoreMachine, Evaluate, StackFrames,
    },
    types::{
        self, Address, CellIndex, FdEntry, FunctionDataError, Immediate, ProgramData,
        ProgramDataError,
    },
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

    fn inf() -> Self {
        Self {
            min: Immediate::MIN,
            max: Immediate::MAX,
        }
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

#[derive(Clone)]
pub struct Verifier {
    machine: CoreMachine,
    pub stack: StackFrames<ValueSpan>,
    findings: Findings,
}

// Convenience: lets `verifier.cells`, `verifier.base`, and the stack methods
// resolve directly through the inner `StackFrames`.
impl Deref for Verifier {
    type Target = StackFrames<ValueSpan>;
    fn deref(&self) -> &StackFrames<ValueSpan> {
        &self.stack
    }
}
impl DerefMut for Verifier {
    fn deref_mut(&mut self) -> &mut StackFrames<ValueSpan> {
        &mut self.stack
    }
}

impl Verifier {
    pub fn new(program: impl Into<Rc<[Instruction]>>) -> Self {
        Self {
            machine: CoreMachine::new(program),
            stack: StackFrames::new(),
            findings: Findings::default(),
        }
    }

    pub fn redirect_input(&mut self, new_input: types::Input) {
        self.machine.input = new_input;
    }

    pub fn redirect_output(&mut self, new_output: types::Output) {
        self.machine.output = new_output;
    }

    fn run_loop(&mut self) -> Result<(), VerifierError> {
        while let Some(instr) = self.machine.next() {
            self.evaluate_instruction(&instr)?;
            self.findings.processed_instructions += 1;
        }
        Ok(())
    }

    /// Low-level nested run: scopes `cells` (via [`StackFrames`]) and `program_data`.
    /// Returns `(last_cell_at_end_of_body, body_stack_size)`.
    ///
    /// Callers needing to also scope `findings` or `function_data` should use
    /// [`run_block_scoped`](Self::run_block_scoped) instead.
    fn run_nested(
        &mut self,
        instrs: Rc<[Instruction]>,
    ) -> Result<(Option<ValueSpan>, usize), VerifierError> {
        let saved_base = self.stack.enter();
        let saved_pd = std::mem::replace(
            &mut self.machine.program_data,
            ProgramData::new(instrs),
        );

        let exec_result = self.run_loop();

        self.machine.program_data = saved_pd;
        let exit = self.stack.exit(saved_base);

        exec_result?;
        Ok(exit)
    }

    /// Runs `instrs` as a fully-scoped block: `cells`, `program_data`,
    /// `findings`, and `function_data` are all saved on entry and restored on
    /// exit. Used by `evaluate_block` and `evaluate_ifelse`.
    fn run_block_scoped(
        &mut self,
        instrs: Rc<[Instruction]>,
    ) -> Result<(Option<ValueSpan>, usize), VerifierError> {
        let saved_findings = self.findings.clone();
        let saved_fd = self.machine.function_data.clone();

        let result = self.run_nested(instrs);

        self.machine.function_data = saved_fd;
        self.findings = saved_findings;
        result
    }

    /// Verifies the body of a `FunctionDefine`: runs the body in an inner scope,
    /// collects the discovered argument positions and any nested function aliases,
    /// then publishes them into the parent's `findings.func_data`.
    fn verify_function_definition(&mut self, fun: &str) -> Result<(), VerifierError> {
        self.machine.common_function_logic(fun)?;
        // Shadowing is not permitted; compilers can generate unique function names.
        let body = self.machine.program_data.get_current()?.clone();

        // Scope: snapshot findings + function_data so body mutations don't leak.
        let saved_findings = self.findings.clone();
        let saved_fd = self.machine.function_data.clone();

        // Seed the body's findings: it is collecting args for `fun`, and `fun`
        // itself must be visible inside the body (e.g. for recursion checks).
        let seed = || FunctionDefiningInfo {
            function_name: fun.to_owned(),
            arg_positions: Vec::new(),
        };
        self.findings.func_defining = Some(seed());
        self.findings.func_data.insert(fun.to_owned(), seed());

        let run_result = self.run_nested(Rc::<[Instruction]>::from(vec![body]));

        // Capture body-mutated state before restoring the parent's snapshots.
        let body_func_defining = self.findings.func_defining.take();
        let body_function_table = self.machine.function_data.function_table.clone();

        self.machine.function_data = saved_fd;
        self.findings = saved_findings;

        run_result?;
        let func_defining = body_func_defining
            .expect("FunctionDefiningInfo should be set during function verification.");

        // Inner names that aliased `fun` (consecutive `FunctionDefine`s) get the
        // same arg signature in the parent's func_data.
        // PERF: linear scan; consider an inverse index if this gets hot.
        for (alias, entry) in &body_function_table {
            if matches!(entry, FdEntry::Str(s) if s == fun) {
                self.findings.func_data.insert(
                    alias.to_owned(),
                    FunctionDefiningInfo {
                        function_name: alias.to_owned(),
                        arg_positions: func_defining.arg_positions.clone(),
                    },
                );
            }
        }

        self.findings.func_data.insert(fun.to_owned(), func_defining);
        Ok(())
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

    pub fn read(&self, reg: CellIndex) -> Result<Option<ValueSpan>, VerifierError> {
        if self.findings.is_collecting_func_args() {
            return Result::Ok(None);
        }

        self.stack
            .get(reg.into())
            .copied()
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
        self.run_loop()?;
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

impl Evaluate for Verifier {
    type Error = VerifierError;

    fn evaluate_alu_nullary(&mut self, instr: &NullaryOp) -> Result<(), Self::Error> {
        use NullaryOp::*;

        match instr {
            Nop => (),
            Rebase => {
                self.stack.rebase().map_err(|()| RebaseError)?;
                self.findings.values_after_rebase = Some(self.cells.len());
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
                        self.push(ValueSpan::new(!min, !max));
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

        if let (Some(a), Some(b)) = (a, b) {
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

    fn evaluate_block(&mut self, instrs: Rc<[Instruction]>) -> Result<(), Self::Error> {
        match self.run_block_scoped(instrs)?.0 {
            Some(val) => self.push(val),
            None => return Err(BlockHasEmptyStack),
        }
        Ok(())
    }

    fn evaluate_function(&mut self, instr: &FunctionOp, fun: &String) -> Result<(), Self::Error> {
        use FunctionDataError::FunctionUndefined;
        use FunctionOp::*;

        match instr {
            FunctionDefine => self.verify_function_definition(fun)?,
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
        let single_branch_body = |branch: Rc<Instruction>| Rc::<[Instruction]>::from(vec![(*branch).clone()]);
        let ifelse_result = match value {
            Some(taken) => {
                let body = single_branch_body(if taken { when_true } else { when_false });
                self.run_block_scoped(body)?.0.ok_or(BlockHasEmptyStack)?
            }
            None => {
                let (v1, sz1) = self.run_block_scoped(single_branch_body(when_true))?;
                let val1 = v1.ok_or(BlockHasEmptyStack)?;

                let (v2, sz2) = self.run_block_scoped(single_branch_body(when_false))?;
                let val2 = v2.ok_or(BlockHasEmptyStack)?;

                if sz1 != sz2 {
                    return Err(CondUnequalStackSizes {
                        true_branch_cells: sz1,
                        false_branch_cells: sz2,
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
