use std::{
    collections::HashMap,
    ops::{Add, Deref, DerefMut},
    rc::Rc,
};

use crate::{
    instruction::{
        BinaryOp, FunctionOp, Instruction, IntrinsicArg, IntrinsicOp, NullaryOp, UnaryOpCell, UnaryOpImm,
    },
    machine::{
        CoreError::{self},
        CoreMachine, Evaluate, StackFrames,
    },
    types::{
        self, Address, CellIndex, FunctionDataError, Immediate, ProgramData,
        ProgramDataError,
    },
};
use VerifierError::*;
use log::{debug, error, trace, warn};

#[derive(Debug, Clone)]
pub enum VerifierError {
    Core(CoreError),
    InvalidCell {
        instr: Instruction,
        cell_index: CellIndex,
        cells: Vec<ValueSpan>,
        prog: Rc<[Instruction]>,
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
    CondInvalidCell {
        instr: Instruction,
        cell_index: CellIndex,
        cells: Vec<ValueSpan>,
        prog: Rc<[Instruction]>,
        location: &'static str,
    },
    CondUnequalStackSizes {
        true_branch_cells: usize,
        false_branch_cells: usize,
    },
    BlockHasEmptyStack,
    NestedFunctionDefinition {
        outer_function: String,
        inner_function: String,
    },
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

    #[inline]
    fn is_unbounded(&self) -> bool {
        self.min == Immediate::MIN || self.max == Immediate::MAX
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

/// An index a function reads from its caller's frame while collecting
/// arguments. `Normal` indexes count from the bottom of the stack (as used by
/// `Read`, `Not`, `Add`, ...); `Reverse` indexes count from the top (as used
/// by `ReadReverse`). Both record an index that must resolve to an existing
/// cell in the caller's stack when the function is invoked.
#[derive(Debug, Clone, Copy)]
enum MemorizedIndex {
    Normal(CellIndex),
    Reverse(CellIndex),
}

impl MemorizedIndex {
    /// Minimum number of cells the caller must provide for this index to
    /// resolve to an existing cell.
    fn required_depth(self) -> usize {
        match self {
            MemorizedIndex::Normal(i) | MemorizedIndex::Reverse(i) => usize::from(i) + 1,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct FunctionDefiningInfo {
    function_name: String,
    arg_indices: Vec<MemorizedIndex>,
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

    /// Records an out-of-scope read performed while collecting a function's
    /// arguments. No-op when not currently collecting.
    fn record_arg(&mut self, index: MemorizedIndex) {
        if let Some(info) = self.func_defining.as_mut() {
            info.arg_indices.push(index);
        }
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
        let saved_base = self.stack.enter_block();
        let saved_pd = std::mem::replace(&mut self.machine.program_data, ProgramData::new(instrs));

        let exec_result = self.run_loop();

        self.machine.program_data = saved_pd;
        let exit = self.exit_block(saved_base);

        exec_result?;
        Ok(exit)
    }

    /// Verifies a single ifelse-branch instruction on the parent stack:
    /// cells are mutated in place. `function_data` and `findings` are scoped
    /// (saved on entry, restored on exit) so branches don't leak metadata into
    /// the parent. `Rebase` is forbidden inside the branch via the
    /// `IfElseBranch` marker frame.
    fn run_ifelse_branch(&mut self, instr: &Instruction) -> Result<(), VerifierError> {
        let saved_findings = self.findings.clone();
        // Evaluate the branch instruction directly: a `Block` branch scopes its
        // own `program_data` via `evaluate_block`, and other instructions don't
        // touch it, so there's no need to wrap the branch in a one-element
        // program (which would force cloning the instruction).
        self.stack.enter_ifelse_branch();
        let exec_result = self.evaluate_instruction(instr);
        self.stack.exit_ifelse_branch();

        self.findings = saved_findings;

        exec_result
    }

    /// Runs `instrs` as a fully-scoped block: `cells`, `program_data`,
    /// `findings`, and `function_data` are all saved on entry and restored on
    /// exit. Used by `evaluate_block` and `evaluate_ifelse`.
    fn run_block_scoped(
        &mut self,
        instrs: Rc<[Instruction]>,
    ) -> Result<(Option<ValueSpan>, usize), VerifierError> {
        let saved_findings = self.findings.clone();
        let result = self.run_nested(instrs);
        self.findings = saved_findings;
        result
    }

    /// Verifies the body of a `FunctionDefine`. The function (and any
    /// consecutive aliases) is registered globally by `common_function_logic`,
    /// then its body is verified in a scoped stack while we count how many
    /// arguments it reads from the caller's frame (see `ReadReverse` and
    /// `Rebase`). The discovered argument count is published into
    /// `findings.func_data` at the `Rebase` instruction, so it is visible both
    /// to recursive calls inside the body and to callers after the definition.
    ///
    /// Nested function definitions are intentionally unsupported: they clash
    /// with recursion (the function would be redefined on the second recursion
    /// step) and would force cloning `function_data` per definition.
    fn verify_function_definition(&mut self, fun: &str) -> Result<(), VerifierError> {
        if let Some(ref outer) = self.findings.func_defining {
            return Err(NestedFunctionDefinition {
                outer_function: outer.function_name.clone(),
                inner_function: fun.to_owned(),
            });
        }
        self.machine.common_function_logic(fun)?;
        // Shadowing is not permitted; compilers can generate unique function names.
        // Borrow the current instruction and clone only the block's `Rc` (a
        // cheap pointer bump) rather than deep-cloning the instruction tree.
        let Instruction::Block(inner) = self.machine.program_data.get_current()? else {
            warn!(
                "Function '{}' is not defined by a block; skipping argument analysis.",
                fun
            );
            return Ok(());
        };
        let inner = Rc::clone(inner);

        // Scope only the argument-collection state. `func_data` (the discovered
        // argument counts) must persist so callers and recursive calls see it.
        let saved_defining = self.findings.func_defining.take();
        let saved_after_rebase = self.findings.values_after_rebase.take();

        self.findings.func_defining = Some(FunctionDefiningInfo {
            function_name: fun.to_owned(),
            arg_indices: Vec::new(),
        });
        self.findings.values_after_rebase = None;

        let run_result = self.run_nested(inner);

        self.findings.func_defining = saved_defining;
        self.findings.values_after_rebase = saved_after_rebase;

        run_result?;
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

    pub fn read(&self, reg: CellIndex) -> Result<ValueSpan, VerifierError> {
        self.stack.get(reg.into()).copied().ok_or(InvalidCell {
            instr: self.machine.program_data.get_current()?.clone(),
            cell_index: reg,
            cells: self.cells.clone(),
            prog: self.machine.program_data.get_program(),
            location: "Verifier::read, get()",
        })
    }

    /// True if a `Normal`-indexed read of `idx` falls outside the function's
    /// own loaded cells (`[base, len)`) and therefore reads a caller-supplied
    /// argument rather than function-local data.
    fn reads_argument_normal(&self, idx: CellIndex) -> bool {
        !(self.stack.base..self.cells.len()).contains(&usize::from(idx))
    }

    /// Like [`read`](Self::read), but while collecting a function's arguments
    /// an out-of-scope read is recorded as a `Normal` argument index and
    /// yields an unbounded span instead of erroring.
    fn read_normal(&mut self, idx: CellIndex) -> Result<ValueSpan, VerifierError> {
        if self.findings.is_collecting_func_args() && self.reads_argument_normal(idx) {
            self.findings.record_arg(MemorizedIndex::Normal(idx));
            return Ok(ValueSpan::inf());
        }
        self.read(idx)
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
                self.stack.rebase()?;
                // Crossing `Rebase` ends argument collection: freeze the count
                // discovered so far and publish it for callers / recursion.
                if self.findings.is_collecting_func_args() {
                    let info = self
                        .findings
                        .func_defining
                        .clone()
                        .expect("func_defining must be Some while collecting arguments");
                    self.findings
                        .func_data
                        .insert(info.function_name.clone(), info);
                }
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
            Not => match self.read_normal(arg)? {
                ValueSpan { min, max } if min == max => {
                    self.push(ValueSpan::new(!min, !max));
                }
                _ => {
                    self.push(ValueSpan::inf());
                }
            },
            Read => {
                let val = self.read_normal(arg)?;
                self.push(val);
            }
            ReadReverse => {
                debug!("ReadReverse with arg: {}", arg);
                if self.findings.is_collecting_func_args() {
                    // While collecting a function's arguments, a `ReadReverse`
                    // reads a fresh caller argument only if it reaches *past*
                    // the values already loaded in this phase. Reaching into
                    // the already-loaded region just re-reads function-local
                    // data and is not an argument.
                    let loaded = self.cells.len().saturating_sub(self.stack.base);
                    if usize::from(arg) >= loaded {
                        self.findings.record_arg(MemorizedIndex::Reverse(arg));
                    }
                    self.push(ValueSpan::inf());
                } else {
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
                            prog: self.machine.program_data.get_program(),
                            location: "Verifier::verify_alu_unary_cell, calculating reverse index",
                        })?;

                    let val = self.read(index)?;
                    self.push(val);
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

        let get_min_max = |array: [Option<Immediate>; 4]| {
            let min = array
                .iter()
                .map(|&x| x.unwrap_or(Immediate::MIN))
                .min()
                .expect("Array cannot be empty");
            let max = array
                .iter()
                .map(|&x| x.unwrap_or(Immediate::MAX))
                .max()
                .expect("Array cannot be empty");
            ValueSpan::new(min, max)
        };

        let valuespan_check = |va: ValueSpan, vb: ValueSpan, vn: ValueSpan| {
            if va.is_single_value() && vb.is_single_value() && vn.is_unbounded() {
                Err(ArithmeticOverflow)
            } else {
                Ok(vn)
            }
        };

        let a = self.read_normal(arg1)?;
        let b = self.read_normal(arg2)?;

        // TODO: Write tests for arithmetic overflow checks
        let calculated_value = match instr {
                Add => {
                    let min = a.min.saturating_add(b.min);
                    let max = a.max.saturating_add(b.max);

                    let vs_new = ValueSpan::new(min, max);

                    valuespan_check(a, b, vs_new)?
                }
                Mul => {
                    let candidates = [
                        a.min.checked_mul(b.min),
                        a.min.checked_mul(b.max),
                        a.max.checked_mul(b.min),
                        a.max.checked_mul(b.max),
                    ];

                    let vs_new = get_min_max(candidates);

                    valuespan_check(a, b, vs_new)?
                }
                Div => {
                    if b == ValueSpan::new(0, 0) {
                        return Err(DivisionByZero);
                    }
                    if a.is_single_value() && b.is_single_value() {
                        if let Some(result) = a.min.checked_div(b.min) {
                            ValueSpan::new(result, result)
                        } else {
                            return Err(ArithmeticOverflow);
                        }
                    } else {
                        if b.min <= 0 && b.max >= 0 {
                            // Division by zero is possible, so we return the widest possible range.
                            ValueSpan::inf()
                        } else {
                            let candidates = [
                                a.min.checked_div(b.min),
                                a.min.checked_div(b.max),
                                a.max.checked_div(b.min),
                                a.max.checked_div(b.max),
                            ];

                            let vs_new = get_min_max(candidates);

                            valuespan_check(a, b, vs_new)?
                        }
                    }
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
                self.machine.function_get(fun)?;

                let available = self.cells.len();

                // Every index the function reads as an argument must resolve to
                // an existing cell in the caller's current stack.
                let deepest_missing = self
                    .findings
                    .func_data
                    .get(fun)
                    .ok_or_else(|| VerifierError::from(FunctionUndefined(fun.to_owned())))?
                    .arg_indices
                    .iter()
                    .map(|index| index.required_depth())
                    .filter(|depth| *depth > available)
                    .max();

                if let Some(required) = deepest_missing {
                    return Err(NotEnoughArguments {
                        required: required.try_into().unwrap_or(CellIndex::MAX),
                        available,
                    });
                }

                self.push(ValueSpan::inf());
            }
        }

        Ok(())
    }

    fn evaluate_intrinsic(&mut self, instr: &IntrinsicOp, arg: &IntrinsicArg) -> Result<(), Self::Error> {
        use IntrinsicOp::*;
        use IntrinsicArg::*;
        use types::{Input, Output};

        match (instr, arg) {
            (Print, Cell(_)) => (),
            (Input, Cell(_)) => self.push(ValueSpan::inf()),
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
            _ => return Err(DebugError("Invalid intrinsic argument type")),
        }

        Ok(())
    }

    fn evaluate_ifelse(
        &mut self,
        cond_idx: CellIndex,
        when_true: Rc<Instruction>,
        when_false: Rc<Instruction>,
    ) -> Result<(), Self::Error> {
        /* First, check if the previous instr was a comparison instr.
        If not, warn that this is not really safe. */

        let condition = self.read(cond_idx)?;
        self.check_good_if_placement()?;
        let value = Self::check_unnecessary_if(&condition);

        match value {
            // Condition is statically known: only the taken branch runs, and it
            // mutates the parent stack directly (no comparison needed).
            Some(taken) => {
                let chosen = if taken { when_true } else { when_false };
                self.run_ifelse_branch(&chosen)?;
            }
            // Condition is unknown: both branches are explored. They each mutate
            // the parent stack in place, so we keep a single copy of the initial
            // cells to re-run the false branch from the same starting point.
            None => {
                let initial = self.stack.cells.clone(); // unavoidable: both branches start here

                self.run_ifelse_branch(&when_true)?;
                let true_cells = std::mem::replace(&mut self.stack.cells, initial);

                self.run_ifelse_branch(&when_false)?;
                let false_cells = std::mem::replace(&mut self.stack.cells, Vec::new());

                let (true_len, false_len) = (true_cells.len(), false_cells.len());
                if true_len != false_len {
                    // Restore a valid stack before returning the error.
                    self.stack.cells = true_cells;
                    return Err(CondUnequalStackSizes {
                        true_branch_cells: true_len,
                        false_branch_cells: false_len,
                    });
                }

                // Cell-by-cell merge of the two final stacks.
                self.stack.cells = true_cells
                    .into_iter()
                    .zip(false_cells.into_iter())
                    .map(|(a, b)| a.combine(b))
                    .collect();
            }
        }

        debug!("Finished verifying ifelse; cells = {:?}", self.cells);

        self.findings.is_conditional = true;

        Ok(())
    }
}

#[cfg(test)]
pub mod tests;
