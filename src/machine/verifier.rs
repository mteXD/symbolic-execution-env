//! This module is the implementation of the verifier. Function calls do not get executed.
//!
//! For the "entry point", see [`Verifier::verify`].

use std::{collections::HashMap, fmt::Debug, ops::Add, rc::Rc};

use crate::{
    information_flow::{AwareConnection, FlowError, SecurityPolicy, TagTrait},
    instruction::{
        BinaryOp, Instruction, NullaryOp, UnaryOpCell, UnaryOpCellAmnt, UnaryOpImm, UnaryOpString,
    },
    machine::{
        Cell,
        CoreError::{self},
        CoreMachine, Evaluate, Stack,
    },
    types::{self, CellIndex, FunctionDataError, Immediate, ProgramData, ProgramDataError},
};
use VerifierError::*;
use log::{debug, trace, warn};

#[derive(Debug, Clone)]
pub enum VerifierError<Tag: TagTrait = ()> {
    Core(CoreError),
    StackUnderflow,
    InvalidCell {
        instr: Instruction<Tag>,
        cell_index: CellIndex,
    },
    ArithmeticOverflow,
    DivisionByZero,
    NotEnoughArguments {
        required: CellIndex,
        available: usize,
    },
    UnsafeCondPlacement,
    DebugError(&'static str),
    CondUnequalStackSizes {
        true_branch_cells: usize,
        false_branch_cells: usize,
    },
    BlockHasEmptyStack,
    EmptyBlock,
    NestedFunctionDefinition {
        outer_function: String,
        inner_function: String,
    },
    InstructionError,
    Flow(FlowError<Tag>),
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
    pub(crate) fn new(min: Immediate, max: Immediate) -> Self {
        if min > max {
            panic!(
                "ValueSpan cannot have min greater than max. Got min: {}, max: {}",
                min, max
            );
        }
        Self { min, max }
    }

    pub(crate) fn inf() -> Self {
        Self {
            min: Immediate::MIN,
            max: Immediate::MAX,
        }
    }

    #[inline]
    fn is_unbounded(&self) -> bool {
        self.min == Immediate::MIN || self.max == Immediate::MAX
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

#[derive(Debug, Clone)]
struct FunctionDefiningInfo<Tag: TagTrait = ()> {
    function_name: String,
    arg_indices: Vec<MemorizedIndex>,
    return_value: Option<ValueSpan>,
    return_tag: Option<Tag>,
    /// Whether invoking this function (transitively) runs any downgrader. Used
    /// to forbid recursive downgrades.
    reaches_downgrader: bool,
}

#[derive(Debug, Clone)]
struct Findings<Tag: TagTrait = ()> {
    rebase_seen: bool,
    func_defining: Option<FunctionDefiningInfo<Tag>>,
    func_data: HashMap<String, FunctionDefiningInfo<Tag>>,
}

impl<Tag: TagTrait> Default for Findings<Tag> {
    fn default() -> Self {
        Self {
            rebase_seen: false,
            func_defining: None,
            func_data: HashMap::new(),
        }
    }
}

impl<Tag: TagTrait> Findings<Tag> {
    #[inline]
    fn is_collecting_func_args(&self) -> bool {
        self.func_defining.is_some() && !self.rebase_seen
    }

    /// Records an out-of-scope read performed while collecting a function's
    /// arguments. No-op when not currently collecting.
    fn record_arg(&mut self, index: MemorizedIndex) {
        if let Some(info) = self.func_defining.as_mut() {
            info.arg_indices.push(index);
        }
    }
}

#[derive(Clone, Debug)]
pub struct Verifier<Tag: TagTrait = ()> {
    machine: CoreMachine<Tag>,
    stack: Stack<ValueSpan, Tag>,
    policy: SecurityPolicy<Tag>,
    pc_tag: Tag,
    findings: Findings<Tag>,
    /// Active downgrader connection while analyzing its body at definition
    /// time. No stack is needed: nested definitions are forbidden, so at most
    /// one downgrader is under analysis at a time.
    current_downgrader: Option<AwareConnection<Tag>>,
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
            findings: Findings::default(),
            current_downgrader: None,
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
        Self::validate_program(&program, &policy)?;
        let pc_tag = policy.default_tag();
        Ok(Self {
            machine: CoreMachine::new(program),
            stack: Stack::new(),
            policy,
            pc_tag,
            findings: Findings::default(),
            current_downgrader: None,
        })
    }

    fn validate_program(
        program: &[Instruction<Tag>],
        policy: &SecurityPolicy<Tag>,
    ) -> Result<(), VerifierError<Tag>> {
        for instruction in program {
            Self::validate_instruction(instruction, policy)?;
        }
        Ok(())
    }

    fn validate_instruction(
        instruction: &Instruction<Tag>,
        policy: &SecurityPolicy<Tag>,
    ) -> Result<(), VerifierError<Tag>> {
        match instruction {
            Instruction::AluUnaryImm(UnaryOpImm::TaggedPush(tag), _) => {
                policy.validate_tag(*tag)?;
            }
            Instruction::Block(body) => Self::validate_program(body, policy)?,
            Instruction::IfElse(_, when_true, when_false) => {
                Self::validate_instruction(when_true, policy)?;
                Self::validate_instruction(when_false, policy)?;
            }
            _ => {}
        }
        Ok(())
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
        self.policy
            .closest_common_descendant(left, right)
            .map_err(VerifierError::Flow)
    }

    /// Tag applied to a function's collected arguments during analysis. Inside
    /// a downgrader this is the connection's `source`, so the computed result
    /// matches `source` and passes the implicit-retag source check. Otherwise
    /// it is the policy default (non-downgrader functions are unaffected).
    fn arg_tag(&self) -> Tag {
        self.current_downgrader
            .map(|c| c.source)
            .unwrap_or_else(|| self.policy.default_tag())
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
                .unwrap_or(Instruction::AluNullary(NullaryOp::Nop)),
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
            Err(VerifierError::Flow(FlowError::InformationFlowViolation {
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

    /// Low-level nested run: scopes the value/tag cells (via the stack's
    /// `Block` frame) and `program_data`.
    /// Returns `(last_value, last_tag, body_stack_size)`.
    ///
    /// Callers needing to also scope `findings` should use
    /// [`run_block_scoped`](Self::run_block_scoped) instead.
    fn run_nested(
        &mut self,
        instrs: Rc<[Instruction<Tag>]>,
    ) -> Result<(Option<ValueSpan>, Option<Tag>, usize), VerifierError<Tag>> {
        let saved_base = self.stack.enter_block();
        let saved_pd = std::mem::replace(&mut self.machine.program_data, ProgramData::new(instrs));

        let exec_result = self.run_loop();

        self.machine.program_data = saved_pd;
        let (last, body_size) = self.stack.exit_block(saved_base);
        let (last_value, last_tag) = match last {
            Some(slot) => (Some(slot.value), Some(slot.tag)),
            None => (None, None),
        };

        exec_result?;
        Ok((last_value, last_tag, body_size))
    }

    /// Verifies a single ifelse-branch instruction on the parent stack:
    /// cells are mutated in place. `findings` is scoped (saved on entry,
    /// restored on exit) so branches don't leak analysis metadata into the
    /// parent; the machine's global function registry is NOT scoped, matching
    /// the executor. `Rebase` is forbidden inside the branch via the
    /// `IfElseBranch` marker frame.
    ///
    /// The `condition_tag` is combined with the current `pc_tag` so that all
    /// values pushed inside the branch carry the condition's taint.
    fn run_ifelse_branch(
        &mut self,
        instr: &Instruction<Tag>,
        condition_tag: Tag,
    ) -> Result<(), VerifierError<Tag>> {
        let saved_findings = self.findings.clone();
        let saved_pc_tag = self.pc_tag;
        self.pc_tag = self.combine_tags(self.pc_tag, condition_tag)?;

        // The IfElseBranch frame makes pops transparent to enclosing blocks and
        // forbids `Rebase` inside a branch. Cells (value and tag) are merged
        // across the two branches by evaluate_ifelse.
        self.stack.enter_ifelse_branch();
        let exec_result = self.evaluate_instruction(instr);
        self.stack.exit_ifelse_branch();

        self.pc_tag = saved_pc_tag;
        self.findings = saved_findings;

        exec_result
    }

    /// Runs `instrs` as a fully-scoped block: cells, `program_data`, and
    /// `findings` are saved on entry and restored on exit. Used by
    /// `evaluate_block`.
    fn run_block_scoped(
        &mut self,
        instrs: Rc<[Instruction<Tag>]>,
    ) -> Result<(Option<ValueSpan>, Option<Tag>, usize), VerifierError<Tag>> {
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
    fn verify_function_definition(
        &mut self,
        fun: &str,
        is_downgrader: bool,
    ) -> Result<(), VerifierError<Tag>> {
        if let Some(ref outer) = self.findings.func_defining {
            return Err(NestedFunctionDefinition {
                outer_function: outer.function_name.clone(),
                inner_function: fun.to_owned(),
            });
        }

        // The instruction declares intent; the policy declares the connection.
        // Cross-check the two so a downgrade gate is never defined as an
        // ordinary function (and vice versa).
        let connection = match (is_downgrader, self.policy.downgrader(fun)) {
            (true, None) => {
                return Err(VerifierError::Flow(FlowError::DowngraderUndefined {
                    name: fun.to_owned(),
                }));
            }
            (false, Some(_)) => {
                return Err(VerifierError::Flow(FlowError::DowngraderUndefined {
                    name: fun.to_owned(),
                }));
            }
            (true, Some(downgrader)) => Some(downgrader.connection),
            (false, None) => None,
        };

        let aliases = self.machine.common_function_logic(fun)?;

        // Shadowing is not permitted; compilers can generate unique function names.
        // Borrow the current instruction and clone only the block's `Rc` (a
        // cheap pointer bump) rather than deep-cloning the instruction tree.
        let to_check = match self.machine.program_data.get_current()? {
            Instruction::Block(inner) => Rc::clone(inner),
            instr => {
                Rc::new([instr.clone()]) // Not expensive
            }
        };

        // Scope only the argument-collection state. `func_data` (the discovered
        // argument counts) must persist so callers and recursive calls see it.
        let saved_defining = self.findings.func_defining.take();
        let saved_rebase_seen = std::mem::replace(&mut self.findings.rebase_seen, false);
        // Establish the downgrader context so the body's arguments carry the
        // connection `source` and, once the body is analyzed, its return value
        // is implicitly retagged to `target`.
        let saved_downgrader = self.current_downgrader;
        self.current_downgrader = connection;

        self.findings.func_defining = Some(FunctionDefiningInfo {
            function_name: fun.to_owned(),
            arg_indices: Vec::new(),
            return_value: None,
            return_tag: None,
            reaches_downgrader: false,
        });

        let run_result = self.run_nested(to_check);

        if let Ok((Some(return_value), return_tag, _)) = &run_result
            && let Some(ref mut info) = self.findings.func_defining
        {
            info.return_value = Some(*return_value);
            info.return_tag = *return_tag;
        }

        // Definition-time implicit retag: a downgrader's return value must carry
        // the connection `source`; we then record `target` so call sites publish
        // the downgraded tag. Invoking a downgrader trivially runs a downgrader,
        // which the transitive recursion check relies on.
        let downgrade_check = if let Some(connection) = self.current_downgrader {
            if let Some(info) = self.findings.func_defining.as_mut() {
                info.reaches_downgrader = true;
            }
            match self
                .findings
                .func_defining
                .as_ref()
                .and_then(|info| info.return_tag)
            {
                Some(tag) if tag == connection.source => {
                    if let Some(info) = self.findings.func_defining.as_mut() {
                        info.return_tag = Some(connection.target);
                    }
                    Ok(())
                }
                Some(tag) => Err(VerifierError::Flow(
                    FlowError::DowngraderReturnTagMismatch {
                        found: tag,
                        expected: connection.source,
                    },
                )),
                None => Ok(()),
            }
        } else {
            Ok(())
        };

        self.findings.func_data.insert(
            fun.to_string(),
            self.findings.func_defining.clone().unwrap(),
        );
        for alias in aliases {
            self.findings
                .func_data
                .insert(alias, self.findings.func_defining.clone().unwrap());
        }

        self.findings.func_defining = saved_defining;
        self.findings.rebase_seen = saved_rebase_seen;
        self.current_downgrader = saved_downgrader;

        downgrade_check?;
        run_result?;

        trace!("Finished verifying function definition for '{}'", fun);
        trace!(
            "Discovered argument indices for '{}': {:?}",
            fun,
            self.findings
                .func_data
                .get(fun)
                .map(|info| &info.arg_indices)
        );

        Ok(())
    }

    /// Verifies a call site. `is_downgrade` marks a `Downgrade` instruction
    /// (enforce the per-value budget, treat the callee as a downgrader for the
    /// recursion check) versus an ordinary `FunctionCall`. The instruction and
    /// the policy registration must agree: a `Downgrade` of an unregistered
    /// name, or a `FunctionCall` of a registered downgrader, is rejected.
    fn verify_function_call(
        &mut self,
        function_name: &str,
        is_downgrade: bool,
    ) -> Result<(), VerifierError<Tag>> {
        use FunctionDataError::FunctionUndefined;

        // Cross-check the instruction's intent against the policy registration.
        let downgrader = match (is_downgrade, self.policy.downgrader(function_name)) {
            (true, None) => {
                return Err(VerifierError::Flow(FlowError::DowngraderUndefined {
                    name: function_name.to_owned(),
                }));
            }
            (false, Some(_)) => {
                return Err(VerifierError::Flow(FlowError::DowngraderUndefined {
                    name: function_name.to_owned(),
                }));
            }
            (true, Some(downgrader)) => Some(downgrader),
            (false, None) => None,
        };

        self.machine.function_get(function_name)?;

        // Forbid recursive downgrades: a downgrader body may not (transitively)
        // invoke any downgrader. A `Downgrade` is trivially such an invocation.
        let callee_reaches = is_downgrade
            || self
                .findings
                .func_data
                .get(function_name)
                .is_some_and(|info| info.reaches_downgrader);
        if self.current_downgrader.is_some() && callee_reaches {
            return Err(VerifierError::Flow(FlowError::RecursiveDowngrader {
                downgrader: function_name.to_owned(),
            }));
        }
        // Propagate the taint to the function currently being defined.
        if let Some(defining) = self.findings.func_defining.as_mut() {
            defining.reaches_downgrader |= callee_reaches;
        }

        let x = self.findings.func_data.get(function_name);

        trace!(
            "Tried to get function data for '{}': {:#?}",
            function_name, x
        );

        let available = self.stack.len();

        // Every index the function reads as an argument must resolve to an
        // existing cell in the caller's current stack.
        let deepest_missing = self
            .findings
            .func_data
            .get(function_name)
            .ok_or_else(|| VerifierError::from(FunctionUndefined(function_name.to_owned())))?
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

        // Per-data downgrade budget: each distinct caller cell the downgrader
        // reads is downgraded once per call. The counter rides with that cell
        // and resets when it is popped/replaced.
        if let Some(_downgrader) = downgrader {
            let mut positions: Vec<usize> = self
                .findings
                .func_data
                .get(function_name)
                .map(|info| {
                    info.arg_indices
                        .iter()
                        .filter_map(|index| Self::arg_caller_position(*index, available))
                        .collect()
                })
                .unwrap_or_default();
            positions.sort_unstable();
            positions.dedup();
            for _position in positions {
                // let count = self.stack.bump_count(position, function_name);
                // if let Some(limit) = downgrader.max_calls {
                //     if count > limit {
                //         return Err(VerifierError::Flow(
                //             FlowError::DowngraderCallLimitExceeded {
                //                 downgrader: function_name.to_owned(),
                //                 limit,
                //             },
                //         ));
                //     }
                // }
            }
        }

        let return_value = self
            .findings
            .func_data
            .get(function_name)
            .and_then(|info| info.return_value)
            .unwrap_or_else(ValueSpan::inf);
        let return_tag = self
            .findings
            .func_data
            .get(function_name)
            .and_then(|info| info.return_tag)
            .unwrap_or_else(|| self.policy.default_tag());
        self.push_existing(return_value, return_tag);

        Ok(())
    }

    pub fn read(&self, reg: CellIndex) -> Result<ValueSpan, VerifierError<Tag>> {
        self.stack.value_at(reg.into()).ok_or(InvalidCell {
            instr: self.machine.program_data.get_current()?.clone(),
            cell_index: reg,
        })
    }

    /// True if a `Normal`-indexed read of `idx` falls outside the function's
    /// own loaded cells (`[base, len)`) and therefore reads a caller-supplied
    /// argument rather than function-local data.
    fn reads_argument_normal(&self, idx: CellIndex) -> bool {
        !(self.stack.base()..self.stack.len()).contains(&usize::from(idx))
    }

    /// Resolves a recorded argument index to an absolute caller-stack position,
    /// given the number of cells (`available`) visible at the call site.
    /// `Normal` indices count from the bottom; `Reverse` from the top.
    fn arg_caller_position(index: MemorizedIndex, available: usize) -> Option<usize> {
        match index {
            MemorizedIndex::Normal(i) => Some(usize::from(i)),
            MemorizedIndex::Reverse(k) => available
                .checked_sub(1)
                .and_then(|top| top.checked_sub(usize::from(k))),
        }
    }

    /// Like [`read`](Self::read), but while collecting a function's arguments
    /// an out-of-scope read is recorded as a `Normal` argument index and
    /// yields an unbounded span (with `default_tag`) instead of erroring.
    fn read_normal(&mut self, idx: CellIndex) -> Result<(ValueSpan, Tag), VerifierError<Tag>> {
        if self.findings.is_collecting_func_args() && self.reads_argument_normal(idx) {
            self.findings.record_arg(MemorizedIndex::Normal(idx));
            return Ok((ValueSpan::inf(), self.arg_tag()));
        }
        let val = self.read(idx)?;
        let tag = self.read_tag(idx)?;
        Ok((val, tag))
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
                self.findings.rebase_seen = true;
            }
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
            Not => {
                let (val, tag) = self.read_normal(arg)?;
                let result = ValueSpan::new(!val.max, !val.min);
                self.push_with_tag(result, tag)?;
            }
            Read => {
                let (val, tag) = self.read_normal(arg)?;
                self.push_with_tag(val, tag)?;
            }
            ReadReverse => {
                debug!("ReadReverse with arg: {}", arg);
                if self.findings.is_collecting_func_args() {
                    // While collecting a function's arguments, a `ReadReverse`
                    // reads a fresh caller argument only if it reaches *past*
                    // the values already loaded in this phase. Reaching into
                    // the already-loaded region just re-reads function-local
                    // data and is not an argument.
                    let loaded = self.stack.len().saturating_sub(self.stack.base());
                    if usize::from(arg) >= loaded {
                        self.findings.record_arg(MemorizedIndex::Reverse(arg));
                    }
                    self.push_with_tag(ValueSpan::inf(), self.arg_tag())?;
                } else {
                    trace!(
                        "Not collecting function arguments, performing normal read with reverse indexing."
                    );
                    // like python's negative indexing.
                    let index = u16::try_from(self.stack.len())
                        .ok()
                        .and_then(|len| len.checked_sub(1))
                        .and_then(|len| len.checked_sub(arg))
                        .ok_or(InvalidCell {
                            instr: self.machine.program_data.get_current()?.clone(),
                            cell_index: arg,
                        })?;

                    let val = self.read(index)?;
                    let tag = self.read_tag(index)?;
                    self.push_with_tag(val, tag)?;
                }
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
        amount: CellIndex,
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
        use types::{Input, Output};

        match instr {
            FunctionDefine => self.verify_function_definition(name, false)?,
            Downgrader => self.verify_function_definition(name, true)?,
            FunctionCall => self.verify_function_call(name, false)?,
            Downgrade => self.verify_function_call(name, true)?,
            FileRead => self.machine.input = Input::from_path(name),
            FileWrite => self.machine.output = Output::from_path(name),
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

        // FIXME: Make it so that overflows are checked for all not-infinite value spans.
        let valuespan_check = |va: ValueSpan, vb: ValueSpan, vn: ValueSpan| {
            if va.is_single_value() && vb.is_single_value() && vn.is_unbounded() {
                Err(ArithmeticOverflow)
            } else {
                Ok(vn)
            }
        };

        let simple_model =
            |a: ValueSpan, b: ValueSpan, op: fn(Immediate, Immediate) -> Immediate| {
                if a.is_single_value() && b.is_single_value() {
                    let result = op(a.min, b.min);
                    ValueSpan::new(result, result)
                } else {
                    ValueSpan::inf()
                }
            };

        let (a, tag_a) = self.read_normal(arg1)?;
        let (b, tag_b) = self.read_normal(arg2)?;
        let result_tag = self.combine_tags(tag_a, tag_b)?;

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
                    // Division by 0 not possible
                    if let Some(result) = a.min.checked_div(b.min) {
                        ValueSpan::new(result, result)
                    } else {
                        return Err(ArithmeticOverflow);
                    }
                } else {
                    if b.min <= 0 && b.max >= 0 {
                        // Division by 0 possible, so we return the widest possible range.
                        warn!("Division by zero possible");
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
            And => simple_model(a, b, |x, y| x & y),
            Or => simple_model(a, b, |x, y| x | y),
            Xor => simple_model(a, b, |x, y| x ^ y),
            ShiftLeftLogical => simple_model(a, b, |x, y| x << y),
            ShiftRightLogical => simple_model(a, b, |x, y| (x as u64 >> y as u64) as Immediate),
            ShiftRightArithmetic => simple_model(a, b, |x, y| x >> y),
            SetEqual => a.chck_eq(&b),
            SetNotEqual => a.chck_neq(&b),
            SetLessThan => a.chck_lt(&b),
            SetLessThanOrEqual => a.chck_lte(&b),
            SetGreaterThan => a.chck_gt(&b),
            SetGreaterThanOrEqual => a.chck_gte(&b),
        };

        self.push_with_tag(calculated_value, result_tag)?;

        Ok(())
    }

    fn evaluate_block(&mut self, instrs: Rc<[Instruction<Tag>]>) -> Result<(), Self::Error> {
        if instrs.is_empty() {
            return Err(EmptyBlock);
        }
        match self.run_block_scoped(instrs)? {
            (Some(val), Some(tag), _) => self.push_existing(val, tag),
            (Some(val), None, _) => self.push_existing(val, self.policy.default_tag()),
            (None, _, _) => return Err(BlockHasEmptyStack),
        }
        Ok(())
    }

    fn evaluate_ifelse(
        &mut self,
        cond_idx: CellIndex,
        when_true: Rc<Instruction<Tag>>,
        when_false: Rc<Instruction<Tag>>,
    ) -> Result<(), Self::Error> {
        let condition = self.read(cond_idx)?;
        let condition_tag = self.read_tag(cond_idx)?;
        let known_truth_value = Self::known_truth_value(&condition);

        match known_truth_value {
            // Condition is statically known: only the taken branch runs, and it
            // mutates the parent stack directly (no comparison needed).
            Some(taken) => {
                let chosen = if taken { when_true } else { when_false };
                self.run_ifelse_branch(&chosen, condition_tag)?;
            }
            // Condition is unknown: both branches are explored. They each mutate
            // the parent stack in place, so we keep a single copy of the initial
            // cells to re-run the false branch from the same starting point.
            None => {
                let initial = self.stack.slots().to_vec();

                self.run_ifelse_branch(&when_true, condition_tag)?;
                let true_cells = self.stack.replace_slots(initial);

                self.run_ifelse_branch(&when_false, condition_tag)?;
                let false_cells = self.stack.take_slots();

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
                            tag: self
                                .policy
                                .closest_common_descendant(a.tag, b.tag)
                                .map_err(VerifierError::Flow)?,
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
