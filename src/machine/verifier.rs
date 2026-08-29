//! This module is the implementation of the verifier. Top-level function calls
//! are abstractly executed; calls reached from within an ordinary function are
//! summarized to guarantee verifier termination, while calls inside a
//! downgrader are rejected.
//!
//! For the "entry point", see [`Verifier::verify`].

use std::{collections::HashMap, fmt::Debug, ops::Add, rc::Rc};

use crate::{
    information_flow::{FlowError, SecurityPolicy, TagTrait, validate_program_tags},
    instruction::{
        BinaryOp, Instruction, NullaryOp, UnaryOpCell, UnaryOpCellAmnt, UnaryOpImm, UnaryOpString,
    },
    machine::{
        Cell,
        CoreError::{self},
        CoreMachine, Evaluate, Stack, reverse_index,
    },
    types::{
        self, CellAmount, CellIndex, FunctionDataError, Immediate, ProgramData, ProgramDataError,
    },
};
use VerifierError::*;
use log::{debug, warn};

type CallableBody<Tag> = (CellAmount, Rc<[Instruction<Tag>]>);

#[derive(Debug, Clone)]
pub enum VerifierError<Tag: TagTrait = ()> {
    Core(CoreError),
    /// When Pop removes too many cells.
    StackUnderflow,
    /// When cell index is invalid
    InvalidCell {
        instr: Instruction<Tag>,
        cell_index: CellIndex,
    },
    ArithmeticOverflow,
    DivisionByZero,
    /// When IfElse branches differ in stack size
    CondUnequalStackSizes {
        true_branch_cells: usize,
        false_branch_cells: usize,
    },
    /// When block cannot return a value due to having an empty stack
    BlockHasEmptyStack,
    /// When no instructions are inside a block
    EmptyBlock,
    /// When FunctionDefine is inside another function's body
    NestedFunctionDefinition {
        outer_function: String,
        inner_function: String,
    },
    /// When FunctionCall is inside a downgrader's body
    FunctionCallInsideDowngrader {
        function: String,
        downgrader: String,
    },
    /// When a FunctionDefine is inside IfElse
    ConditionalDefinition {
        function: String,
    },
    /// When instruction is used in a meaningless way -- curretnly, that's Pop 0
    InstructionError,
    /// When there is no doubt going to be an infinite recursion
    InfiniteRecursion {
        function: String,
    },
    Flow(FlowError<Tag>),
    // For debugging purposes
    DebugError(&'static str),
}

impl<Tag: TagTrait> From<CoreError> for VerifierError<Tag> {
    fn from(e: CoreError) -> Self {
        VerifierError::Core(e)
    }
}

impl<Tag: TagTrait> From<ProgramDataError> for VerifierError<Tag> {
    fn from(e: ProgramDataError) -> Self {
        VerifierError::Core(e.into())
    }
}

impl<Tag: TagTrait> From<FunctionDataError> for VerifierError<Tag> {
    fn from(e: FunctionDataError) -> Self {
        VerifierError::Core(e.into())
    }
}

impl<Tag: TagTrait> From<FlowError<Tag>> for VerifierError<Tag> {
    fn from(e: FlowError<Tag>) -> Self {
        VerifierError::Flow(e)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueSpan {
    pub min: Immediate,
    pub max: Immediate,
}

impl From<Immediate> for ValueSpan {
    fn from(val: Immediate) -> Self {
        ValueSpan::new(val, val)
    }
}

impl ValueSpan {
    /// Creates an inclusive interval of values the verifier may produce.
    ///
    /// This is useful when inspecting verifier results in external tools and
    /// integration tests. Panics if `min` is greater than `max`.
    pub fn new(min: Immediate, max: Immediate) -> Self {
        if min > max {
            panic!(
                "ValueSpan cannot have min greater than max. Got min: {}, max: {}",
                min, max
            );
        }
        Self { min, max }
    }

    /// Returns the interval containing every representable [`Immediate`].
    ///
    /// The verifier uses this span for values whose concrete value is unknown.
    pub fn inf() -> Self {
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

    pub fn map(self, f: impl Fn(Immediate) -> Immediate) -> Self {
        Self {
            min: f(self.min),
            max: f(self.max),
        }
    }

    /// Exact interval model of bitwise `Not` for signed 64-bit values.
    ///
    /// [`Immediate`] is `i64`, for which `!x == -1 - x`. This is a strictly
    /// decreasing bijection over the entire representable domain, so the image
    /// of `[min, max]` is exactly `[!max, !min]`. Both endpoint complements are
    /// always representable, including at `Immediate::MIN` and
    /// `Immediate::MAX`.
    fn not_span(self) -> Self {
        Self::new(!self.max, !self.min)
    }

    /// Interval model of mathematical negation. Negation reverses the bounds;
    /// a singleton at [`Immediate::MIN`] is a definite overflow, while a wider
    /// interval containing it retains the span of all representable results.
    fn neg_span(self) -> Option<Self> {
        if self.is_single_value() {
            let result = self.min.checked_neg()?;
            Some(Self::new(result, result))
        } else {
            Some(Self::new(
                self.max.saturating_neg(),
                self.min.saturating_neg(),
            ))
        }
    }

    /// Interval model of `Add`. Exact operands use checked arithmetic so a
    /// representable boundary result is not confused with overflow; wider
    /// intervals saturate their raw bounds. `None` means two exact operands
    /// overflowed.
    fn add_span(self, other: Self) -> Option<Self> {
        if self.is_single_value() && other.is_single_value() {
            let result = self.min.checked_add(other.min)?;
            Some(Self::new(result, result))
        } else {
            Some(Self::new(
                self.min.saturating_add(other.min),
                self.max.saturating_add(other.max),
            ))
        }
    }

    /// Interval model of `Sub`: the lowest result uses the left lower bound
    /// and right upper bound, while the highest uses the opposite pair.
    /// `None` means two exact operands overflowed.
    fn sub_span(self, other: Self) -> Option<Self> {
        if self.is_single_value() && other.is_single_value() {
            let result = self.min.checked_sub(other.min)?;
            Some(Self::new(result, result))
        } else {
            Some(Self::new(
                self.min.saturating_sub(other.max),
                self.max.saturating_sub(other.min),
            ))
        }
    }

    /// Interval model of `Mul`. Exact operands use checked arithmetic; wider
    /// intervals cover all four corner products and widen conservatively when
    /// a corner overflows. `None` means two exact operands overflowed.
    fn mul_span(self, other: Self) -> Option<Self> {
        if self.is_single_value() && other.is_single_value() {
            let result = self.min.checked_mul(other.min)?;
            Some(Self::new(result, result))
        } else {
            let candidates = [
                self.min.checked_mul(other.min),
                self.min.checked_mul(other.max),
                self.max.checked_mul(other.min),
                self.max.checked_mul(other.max),
            ];
            Some(Self::from_candidates(candidates))
        }
    }

    /// Interval model of `Div`, assuming `other` is not exactly zero (the
    /// caller rejects that as a division by zero). If zero is merely
    /// *possible* in `other`, the result is the unbounded span (with a
    /// warning). `None` means two exact operands overflowed
    /// (`Immediate::MIN / -1`).
    fn div_span(self, other: Self) -> Option<Self> {
        if self.is_single_value() && other.is_single_value() {
            // Division by 0 not possible
            let result = self.min.checked_div(other.min)?;
            Some(ValueSpan::new(result, result))
        } else if other.min <= 0 && other.max >= 0 {
            // Division by 0 possible, so we return the widest possible range.
            warn!("Division by zero possible");
            Some(Self::inf())
        } else {
            let candidates = [
                self.min.checked_div(other.min),
                self.min.checked_div(other.max),
                self.max.checked_div(other.min),
                self.max.checked_div(other.max),
            ];
            Some(Self::from_candidates(candidates))
        }
    }

    /// Interval model shared by the bit and shift ops: exact when both
    /// operands are exact, unbounded otherwise (bitwise results do not
    /// interpolate between interval bounds).
    fn bitop_span(self, other: Self, op: fn(Immediate, Immediate) -> Immediate) -> Self {
        if self.is_single_value() && other.is_single_value() {
            let result = op(self.min, other.min);
            ValueSpan::new(result, result)
        } else {
            Self::inf()
        }
    }

    /// Builds the span covering all four candidate corner results; a `None`
    /// candidate (its checked arithmetic overflowed) widens the result to both
    /// representable limits.
    fn from_candidates(candidates: [Option<Immediate>; 4]) -> Self {
        let min = candidates
            .iter()
            .map(|&x| x.unwrap_or(Immediate::MIN))
            .min()
            .expect("Array cannot be empty");
        let max = candidates
            .iter()
            .map(|&x| x.unwrap_or(Immediate::MAX))
            .max()
            .expect("Array cannot be empty");
        ValueSpan::new(min, max)
    }

    fn chck_eq(&self, other: &Self) -> Self {
        if self.is_single_value() && other.is_single_value() {
            if self.min == other.min {
                ValueSpan::new(1, 1)
            } else {
                ValueSpan::new(0, 0)
            }
        } else if self.disjunct(other) {
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

/// Execution context used to distinguish global calls, which are interpreted,
/// from calls reached inside an ordinary function, which are summarized, and
/// calls reached inside a downgrader, which are rejected. Top-level blocks and
/// branches preserve `TopLevel`.
#[derive(Debug, Clone)]
enum VerificationContext {
    TopLevel,
    Function(String),
    Downgrader(String),
}

impl VerificationContext {
    fn is_top_level(&self) -> bool {
        matches!(self, Self::TopLevel)
    }

    fn enclosing_name(&self) -> Option<&str> {
        match self {
            Self::TopLevel => None,
            Self::Function(name) | Self::Downgrader(name) => Some(name),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Verifier<Tag: TagTrait = ()> {
    machine: CoreMachine<Tag>,
    stack: Stack<ValueSpan, Tag>,
    policy: SecurityPolicy<Tag>,
    pc_tag: Tag,
    /// Whether instructions are running globally or in a function-like body.
    context: VerificationContext,
    /// How many times each downgrader is called, counted statically. The call
    /// limit spans the whole program (with branch-specific merging in `ifelse`).
    downgrader_calls: HashMap<String, usize>,
    /// True while the verifier is evaluating an ifelse branch. Function and
    /// downgrader definitions are rejected there because they may not execute.
    in_conditional_branch: bool,
}

impl Verifier<()> {
    /// Creates an ordinary verifier with information-flow monitoring disabled.
    pub fn new(program: impl Into<Rc<[Instruction]>>) -> Self {
        let policy = SecurityPolicy::no_flow();
        Self {
            machine: CoreMachine::new(program),
            stack: Stack::new(),
            policy,
            pc_tag: (),
            context: VerificationContext::TopLevel,
            downgrader_calls: HashMap::new(),
            in_conditional_branch: false,
        }
    }
}

impl<Tag: TagTrait> Verifier<Tag> {
    /// Creates a monitored verifier and validates every tag embedded in the program.
    pub fn with_policy(
        program: impl Into<Rc<[Instruction<Tag>]>>,
        policy: SecurityPolicy<Tag>,
    ) -> Result<Self, VerifierError<Tag>> {
        let program = program.into();
        validate_program_tags(&program, &policy)?;
        let pc_tag = policy.default_tag();
        Ok(Self {
            machine: CoreMachine::new(program),
            stack: Stack::new(),
            policy,
            pc_tag,
            context: VerificationContext::TopLevel,
            downgrader_calls: HashMap::new(),
            in_conditional_branch: false,
        })
    }

    pub fn redirect_input(mut self, new_input: types::Input) -> Self {
        self.machine.input = new_input;
        self
    }

    pub fn redirect_output(mut self, new_output: types::Output) -> Self {
        self.machine.output = new_output;
        self
    }

    // ---- Tag helpers --------------------------------------------------------

    /// Calculates ccd(left, right).
    fn combine_tags(&self, left: Tag, right: Tag) -> Result<Tag, VerifierError<Tag>> {
        self.policy.ccd(left, right).map_err(VerifierError::Flow)
    }

    /// Pushes a newly-created value, including the current control-flow tag.
    fn push_with_tag(&mut self, value: ValueSpan, tag: Tag) -> Result<(), VerifierError<Tag>> {
        let effective_tag = self.combine_tags(tag, self.pc_tag)?;
        self.stack.push(Cell::new(value, effective_tag));
        Ok(())
    }

    /// Pushes a value whose tag is already final (e.g. block / function return).
    /// Does NOT combine with pc_tag.
    fn push_existing(&mut self, value: ValueSpan, tag: Tag) {
        self.stack.push(Cell::new(value, tag));
    }

    /// Reads the tag corresponding to a value cell.
    pub fn read_tag(&self, idx: CellIndex) -> Result<Tag, VerifierError<Tag>> {
        self.stack.tag_at(idx.into()).ok_or_else(|| InvalidCell {
            instr: self
                .machine
                .program_data
                .get_current()
                .cloned()
                .unwrap_or(Instruction::Nullary(NullaryOp::Nop)),
            cell_index: idx,
        })
    }

    /// Returns the value cells, cloned out for inspection.
    pub fn values(&self) -> Vec<ValueSpan> {
        self.stack.values()
    }

    /// Returns the parallel tag stack.
    pub fn tags(&self) -> Vec<Tag> {
        self.stack.tags()
    }

    /// Returns the tag of the top value cell.
    pub fn last_tag(&self) -> Option<Tag> {
        self.stack.last_tag()
    }

    fn ensure_output_allowed(&self, value_tag: Tag) -> Result<(), VerifierError<Tag>> {
        let effective_tag = self.combine_tags(value_tag, self.pc_tag)?;
        let output_guard = self.policy.output_tag();

        if self
            .policy
            .can_flow(effective_tag, output_guard)
            .map_err(VerifierError::Flow)?
        {
            Ok(())
        } else {
            Err(VerifierError::Flow(FlowError::PGViolation {
                found: effective_tag,
                guard: output_guard,
            }))
        }
    }

    // ---- Core execution -----------------------------------------------------

    fn run_loop(&mut self) -> Result<(), VerifierError<Tag>> {
        while let Some(instr) = self.machine.next() {
            self.evaluate_instruction(&instr)?;
        }
        Ok(())
    }

    /// Runs a counted block while scoping its value/tag cells and program data.
    /// Returns `(last_value, last_tag, body_stack_size)`.
    fn run_nested(
        &mut self,
        argument_count: CellAmount,
        instrs: Rc<[Instruction<Tag>]>,
    ) -> Result<(Option<ValueSpan>, Option<Tag>, usize), VerifierError<Tag>> {
        self.stack.enter_block(argument_count)?;
        self.run_entered_block(instrs)
    }

    /// Completes a block whose stack frame has already been entered. Cleanup is
    /// unconditional so an abstract-execution error cannot leak scoped state.
    fn run_entered_block(
        &mut self,
        instrs: Rc<[Instruction<Tag>]>,
    ) -> Result<(Option<ValueSpan>, Option<Tag>, usize), VerifierError<Tag>> {
        let saved_pd = std::mem::replace(&mut self.machine.program_data, ProgramData::new(instrs));

        let exec_result = self.run_loop();

        self.machine.program_data = saved_pd;
        let (last, body_size) = self.stack.exit_block();
        let (last_value, last_tag) = match last {
            Some(slot) => (Some(slot.value), Some(slot.tag)),
            None => (None, None),
        };

        exec_result?;
        Ok((last_value, last_tag, body_size))
    }

    /// Verifies an ifelse branch sequence on the parent stack. Cells are
    /// mutated in place, while definitions are rejected as conditional. A
    /// block within the sequence applies its own declared semantics.
    ///
    /// The `condition_tag` is combined with the current `pc_tag` so that all
    /// values pushed inside the branch carry the condition's taint.
    fn run_ifelse_branch(
        &mut self,
        instrs: Rc<[Instruction<Tag>]>,
        condition_tag: Tag,
    ) -> Result<(), VerifierError<Tag>> {
        let saved_pc_tag = self.pc_tag;
        let saved_in_conditional_branch = self.in_conditional_branch;
        self.pc_tag = self.combine_tags(self.pc_tag, condition_tag)?;
        self.in_conditional_branch = true;

        let saved_program =
            std::mem::replace(&mut self.machine.program_data, ProgramData::new(instrs));
        let exec_result = self.run_loop();

        self.machine.program_data = saved_program;
        self.pc_tag = saved_pc_tag;
        self.in_conditional_branch = saved_in_conditional_branch;

        exec_result
    }

    /// Registers an ordinary function without interpreting its body. The body
    /// is checked with actual abstract arguments when called globally.
    fn verify_function_definition(&mut self, fun: &str) -> Result<(), VerifierError<Tag>> {
        if self.in_conditional_branch {
            return Err(ConditionalDefinition {
                function: fun.to_owned(),
            });
        }
        self.ensure_not_nested(fun)?;
        self.machine.common_function_logic(fun)?;
        Ok(())
    }

    /// Registers a downgrader without interpreting its body. Policy membership
    /// is still validated at the definition; source/target semantics are
    /// checked when the downgrader is called globally.
    fn verify_downgrader_definition(&mut self, fun: &str) -> Result<(), VerifierError<Tag>> {
        if self.in_conditional_branch {
            return Err(ConditionalDefinition {
                function: fun.to_owned(),
            });
        }
        self.ensure_not_nested(fun)?;

        if self.policy.downgrader(fun).is_none() {
            return Err(VerifierError::Flow(FlowError::DowngraderUndefined {
                name: fun.to_owned(),
            }));
        }
        self.machine.common_downgrader_logic(fun)?;
        Ok(())
    }

    /// Nested function definitions are intentionally unsupported: they clash
    /// with recursion (the function would be redefined on the second recursion
    /// step) and would force cloning `function_data` per definition.
    fn ensure_not_nested(&self, inner: &str) -> Result<(), VerifierError<Tag>> {
        if let Some(outer) = self.context.enclosing_name() {
            return Err(NestedFunctionDefinition {
                outer_function: outer.to_owned(),
                inner_function: inner.to_owned(),
            });
        }
        Ok(())
    }

    /// Resolves a function-like body and validates its block shape. Empty
    /// bodies are rejected when called, matching concrete call behavior.
    fn callable_body(
        &self,
        function_name: &str,
        is_downgrader: bool,
    ) -> Result<CallableBody<Tag>, VerifierError<Tag>> {
        let body = if is_downgrader {
            self.machine.downgrader_get(function_name)?
        } else {
            self.machine.function_get(function_name)?
        };

        match body {
            Instruction::Block(_, instrs) if instrs.is_empty() => Err(EmptyBlock),
            Instruction::Block(argument_count, instrs) => Ok((*argument_count, Rc::clone(instrs))),
            _ => Err(FunctionDataError::FunctionMissingBody(function_name.to_owned()).into()),
        }
    }

    /// Interprets an ordinary function at a global call site. Calls reached
    /// inside an ordinary function are not traversed; after
    /// declaration/shape/arity checks they publish a conservative result.
    /// Ordinary calls inside downgraders are prohibited by the verifier.
    fn verify_function_call(&mut self, function_name: &str) -> Result<(), VerifierError<Tag>> {
        if let VerificationContext::Downgrader(downgrader) = &self.context {
            return Err(FunctionCallInsideDowngrader {
                function: function_name.to_owned(),
                downgrader: downgrader.clone(),
            });
        }

        let (argument_count, body) = self.callable_body(function_name, false)?;

        if !self.context.is_top_level() {
            self.push_existing(ValueSpan::inf(), self.policy.default_tag());
            return Ok(());
        }

        let saved_context = std::mem::replace(
            &mut self.context,
            VerificationContext::Function(function_name.to_owned()),
        );
        let result = self.run_nested(argument_count, body);
        self.context = saved_context;

        match result? {
            (Some(value), Some(tag), _) => self.push_existing(value, tag),
            (Some(value), None, _) => self.push_existing(value, self.policy.default_tag()),
            (None, _, _) => {}
        }
        Ok(())
    }

    /// Verifies a `Downgrade` site by re-executing the downgrader body with the
    /// actual caller argument tags, then applying the implicit retag. Rejects
    /// names not registered as downgraders.
    fn verify_downgrader_call(&mut self, function_name: &str) -> Result<(), VerifierError<Tag>> {
        let Some(downgrader) = self.policy.downgrader(function_name) else {
            return Err(VerifierError::Flow(FlowError::DowngraderUndefined {
                name: function_name.to_owned(),
            }));
        };

        // Downgrades must happen at the top level of the program: a `Downgrade`
        // reached while interpreting a function or downgrader body is rejected.
        if !self.context.is_top_level() {
            return Err(VerifierError::Flow(FlowError::NestedDowngraderCall {
                downgrader: function_name.to_owned(),
            }));
        }

        // Charge the call against the downgrader's total call limit (mirrors the
        // executor: the charge precedes everything the call would do).
        let calls = self
            .downgrader_calls
            .entry(function_name.to_owned())
            .or_insert(0);
        *calls += 1;
        if let Some(limit) = downgrader.max_calls
            && *calls > limit
        {
            return Err(VerifierError::Flow(
                FlowError::DowngraderCallLimitExceeded {
                    downgrader: function_name.to_owned(),
                    limit,
                },
            ));
        }

        // Re-execute the body with the actual top caller suffix. The counted
        // block clones values and tags together before isolating the body.
        let (argument_count, body) = self.callable_body(function_name, true)?;
        let saved_context = std::mem::replace(
            &mut self.context,
            VerificationContext::Downgrader(function_name.to_owned()),
        );
        let result = self.run_nested(argument_count, body);
        self.context = saved_context;

        let connection = downgrader.connection;
        if let (Some(return_value), Some(return_tag), _) = result? {
            if return_tag != connection.source {
                return Err(VerifierError::Flow(
                    FlowError::DowngraderReturnTagMismatch {
                        found: return_tag,
                        expected: connection.source,
                    },
                ));
            }
            self.push_existing(return_value, connection.target);
        }
        Ok(())
    }

    /// Reads a cell's value and tag from the currently visible stack.
    pub fn read(&mut self, idx: CellIndex) -> Result<(ValueSpan, Tag), VerifierError<Tag>> {
        let value = self.stack.value_at(idx.into()).ok_or(InvalidCell {
            instr: self.machine.program_data.get_current()?.clone(),
            cell_index: idx,
        })?;
        let tag = self.read_tag(idx)?;
        Ok((value, tag))
    }

    /// Verifies the program, returning self.
    pub fn verify(mut self) -> Result<Self, VerifierError<Tag>> {
        self.run_loop()?;
        Ok(self)
    }

    fn known_truth_value(condition: &ValueSpan) -> Option<bool> {
        if condition.is_single_value() {
            if condition.min == 0 {
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

impl<Tag: TagTrait> Evaluate<Tag> for Verifier<Tag> {
    type Error = VerifierError<Tag>;

    fn evaluate_alu_nullary(&mut self, instr: &NullaryOp) -> Result<(), Self::Error> {
        use NullaryOp::*;

        match instr {
            Nop => (),
            Input => {
                self.push_with_tag(ValueSpan::inf(), self.policy.input_tag())?;
            }
        }

        Ok(())
    }

    fn evaluate_alu_unary_imm(
        &mut self,
        instr: &UnaryOpImm<Tag>,
        arg: Immediate,
    ) -> Result<(), Self::Error> {
        use UnaryOpImm::*;

        match instr {
            Push => self.push_with_tag(ValueSpan::new(arg, arg), self.policy.default_tag())?,
            TaggedPush(tag) => self.push_with_tag(ValueSpan::new(arg, arg), *tag)?,
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
            Neg => {
                let (val, tag) = self.read(arg)?;
                let result = val.neg_span().ok_or(ArithmeticOverflow)?;
                self.push_with_tag(result, tag)?;
            }
            Not => {
                let (val, tag) = self.read(arg)?;
                let result = val.not_span();
                self.push_with_tag(result, tag)?;
            }
            Read => {
                let (val, tag) = self.read(arg)?;
                self.push_with_tag(val, tag)?;
            }
            ReadReverse => {
                debug!("ReadReverse with arg: {}", arg);
                let index = reverse_index(self.stack.len(), arg).ok_or(InvalidCell {
                    instr: self.machine.program_data.get_current()?.clone(),
                    cell_index: arg,
                })?;

                let (val, tag) = self.read(index)?;
                self.push_with_tag(val, tag)?;
            }
            Print => {
                let tag = self.read_tag(arg)?;
                self.ensure_output_allowed(tag)?;
            }
        }

        Ok(())
    }

    fn evaluate_alu_unary_cell_amnt(
        &mut self,
        instr: &UnaryOpCellAmnt,
        amount: CellAmount,
    ) -> Result<(), Self::Error> {
        use UnaryOpCellAmnt::*;

        match instr {
            Pop => {
                if amount == 0 {
                    return Err(InstructionError);
                }
                for _ in 0..amount {
                    self.stack.pop().ok_or(StackUnderflow)?;
                }
            }
        }

        Ok(())
    }

    fn evaluate_alu_unary_string(
        &mut self,
        instr: &UnaryOpString,
        name: &str,
    ) -> Result<(), Self::Error> {
        use UnaryOpString::*;

        match instr {
            FunctionDefine => self.verify_function_definition(name)?,
            Downgrader => self.verify_downgrader_definition(name)?,
            FunctionCall => self.verify_function_call(name)?,
            Downgrade => self.verify_downgrader_call(name)?,
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

        let (a, tag_a) = self.read(arg1)?;
        let (b, tag_b) = self.read(arg2)?;
        let result_tag = self.combine_tags(tag_a, tag_b)?;

        let calculated_value = match instr {
            Add => a.add_span(b).ok_or(ArithmeticOverflow)?,
            Sub => a.sub_span(b).ok_or(ArithmeticOverflow)?,
            Mul => a.mul_span(b).ok_or(ArithmeticOverflow)?,
            Div => {
                if b == ValueSpan::new(0, 0) {
                    return Err(DivisionByZero);
                }
                a.div_span(b).ok_or(ArithmeticOverflow)?
            }
            And => a.bitop_span(b, |x, y| x & y),
            Or => a.bitop_span(b, |x, y| x | y),
            Xor => a.bitop_span(b, |x, y| x ^ y),
            ShiftLeftLogical => a.bitop_span(b, |x, y| x << y),
            ShiftRightLogical => a.bitop_span(b, |x, y| (x as u64 >> y as u64) as Immediate),
            ShiftRightArithmetic => a.bitop_span(b, |x, y| x >> y),
            CmpEqual => a.chck_eq(&b),
            CmpNotEqual => a.chck_neq(&b),
            CmpLessThan => a.chck_lt(&b),
            CmpLessThanOrEqual => a.chck_lte(&b),
            CmpGreaterThan => a.chck_gt(&b),
            CmpGreaterThanOrEqual => a.chck_gte(&b),
        };

        self.push_with_tag(calculated_value, result_tag)?;

        Ok(())
    }

    fn evaluate_block(
        &mut self,
        argument_count: CellAmount,
        instrs: Rc<[Instruction<Tag>]>,
    ) -> Result<(), Self::Error> {
        if instrs.is_empty() {
            return Err(EmptyBlock);
        }
        let result = self.run_nested(argument_count, instrs);

        match result? {
            (Some(val), Some(tag), _) => self.push_existing(val, tag),
            (Some(val), None, _) => self.push_existing(val, self.policy.default_tag()),
            (None, _, _) => return Err(BlockHasEmptyStack),
        }
        Ok(())
    }

    fn evaluate_ifelse(
        &mut self,
        cond_idx: CellIndex,
        when_true: Rc<[Instruction<Tag>]>,
        when_false: Rc<[Instruction<Tag>]>,
    ) -> Result<(), Self::Error> {
        let (condition, condition_tag) = self.read(cond_idx)?;
        let known_truth_value = Self::known_truth_value(&condition);

        match known_truth_value {
            // Condition is statically known: only the taken branch runs, and it
            // mutates the parent stack directly (no comparison needed).
            Some(taken) => {
                let chosen = if taken { when_true } else { when_false };
                self.run_ifelse_branch(chosen, condition_tag)?;
            }
            // Condition is unknown: both branches are explored. They each mutate
            // the parent stack in place, so we keep a copy of the initial stack
            // to re-run the false branch from the same starting point. This
            // Downgrader call counts are also tracked per branch and merged
            // with the per-downgrader MAX: at runtime only one branch executes, so
            // exploring both must not double-charge.
            None => {
                let initial = self.stack.slots().to_vec();
                let initial_calls = self.downgrader_calls.clone();

                self.run_ifelse_branch(when_true, condition_tag)?;
                let true_cells = self.stack.replace_slots(initial);
                let true_calls = std::mem::replace(&mut self.downgrader_calls, initial_calls);

                self.run_ifelse_branch(when_false, condition_tag)?;
                let false_cells = self.stack.take_slots();

                for (name, count) in true_calls {
                    let entry = self.downgrader_calls.entry(name).or_insert(0);
                    *entry = (*entry).max(count);
                }

                let true_len = true_cells.len();
                let false_len = false_cells.len();
                if true_len != false_len {
                    // Restore a valid stack before returning the error.
                    self.stack.set_slots(true_cells);
                    return Err(CondUnequalStackSizes {
                        true_branch_cells: true_len,
                        false_branch_cells: false_len,
                    });
                }

                // Cell-by-cell merge of the two final stacks: values combine,
                // tags take their closest common descendant.
                let merged = true_cells
                    .into_iter()
                    .zip(false_cells)
                    .map(|(a, b)| {
                        Ok(Cell {
                            value: a.value.combine(b.value),
                            tag: self.policy.ccd(a.tag, b.tag).map_err(VerifierError::Flow)?,
                        })
                    })
                    .collect::<Result<Vec<_>, VerifierError<Tag>>>()?;
                self.stack.set_slots(merged);
            }
        }

        debug!(
            "Finished verifying ifelse; cells = {:?}",
            self.stack.values()
        );

        Ok(())
    }
}
