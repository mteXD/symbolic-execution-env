//! This module is the implementation of the verifier. Function calls do not get executed.
//!
//! For the "entry point", see [`Verifier::verify`].

use std::{collections::HashMap, fmt::Debug, ops::Add, rc::Rc};

use crate::{
    information_flow::{
        AwareConnection, FlowError, SecurityPolicy, TagTrait, validate_program_tags,
    },
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

/// Argument-collection state for the definition body currently under
/// analysis. `collecting_args` is true until the body's `Rebase` closes the
/// collection window.
#[derive(Debug, Clone)]
struct DefinitionInProgress {
    name: String,
    args: Vec<MemorizedIndex>,
    collecting_args: bool,
}

/// What the verifier knows about a defined function: the caller-frame indices
/// its body reads as arguments, and its return span/tag. A downgrader's
/// return tag is the connection `target` (recorded at definition time).
#[derive(Debug, Clone)]
struct FunctionFacts<Tag: TagTrait = ()> {
    args: Vec<MemorizedIndex>,
    return_value: Option<ValueSpan>,
    return_tag: Option<Tag>,
}

#[derive(Clone, Debug)]
pub struct Verifier<Tag: TagTrait = ()> {
    machine: CoreMachine<Tag>,
    stack: Stack<ValueSpan, Tag>,
    policy: SecurityPolicy<Tag>,
    pc_tag: Tag,
    /// Argument-collection state while a definition body is being analyzed.
    defining: Option<DefinitionInProgress>,
    /// Facts discovered about each defined function, by name and alias.
    /// Deliberately rolled back with blocks and ifelse branches (see
    /// `evaluate_block` and [`run_ifelse_branch`](Self::run_ifelse_branch)).
    functions: HashMap<String, FunctionFacts<Tag>>,
    /// Active downgrader connection while analyzing its body at definition
    /// time. No stack is needed: nested definitions are forbidden, so at most
    /// one downgrader is under analysis at a time.
    current_downgrader: Option<AwareConnection<Tag>>,
    /// How many times each downgrader is called, counted statically. Unlike
    /// `defining`/`functions` this is never rolled back: the budget spans the
    /// whole program.
    downgrader_calls: HashMap<String, usize>,
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
            defining: None,
            functions: HashMap::new(),
            current_downgrader: None,
            downgrader_calls: HashMap::new(),
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
            defining: None,
            functions: HashMap::new(),
            current_downgrader: None,
            downgrader_calls: HashMap::new(),
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
    /// `defining` and `functions` are NOT scoped here; callers that need that
    /// (e.g. `evaluate_block`) save and restore them around the call.
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
    /// cells are mutated in place. `defining` and `functions` are scoped
    /// (saved on entry, restored on exit): rolling back `functions` is
    /// intended conservatism — a function defined inside a branch only
    /// conditionally exists, so calling it afterwards does not verify. The
    /// machine's global function registry is NOT scoped, matching the
    /// executor. `Rebase` is forbidden inside the branch via the
    /// `IfElseBranch` marker frame.
    ///
    /// Known bug (pinned by an ignored test): argument reads recorded inside
    /// the branch are rolled back with `defining` and thus forgotten.
    ///
    /// The `condition_tag` is combined with the current `pc_tag` so that all
    /// values pushed inside the branch carry the condition's taint.
    fn run_ifelse_branch(
        &mut self,
        instr: &Instruction<Tag>,
        condition_tag: Tag,
    ) -> Result<(), VerifierError<Tag>> {
        let saved_defining = self.defining.clone();
        let saved_functions = self.functions.clone();
        let saved_pc_tag = self.pc_tag;
        self.pc_tag = self.combine_tags(self.pc_tag, condition_tag)?;

        // The IfElseBranch frame makes pops transparent to enclosing blocks and
        // forbids `Rebase` inside a branch. Cells (value and tag) are merged
        // across the two branches by evaluate_ifelse.
        self.stack.enter_ifelse_branch();
        let exec_result = self.evaluate_instruction(instr);
        self.stack.exit_ifelse_branch();

        self.pc_tag = saved_pc_tag;
        self.defining = saved_defining;
        self.functions = saved_functions;

        exec_result
    }

    /// True while a definition body is being analyzed and its `Rebase` has not
    /// yet closed the argument-collection window.
    fn is_collecting_args(&self) -> bool {
        self.defining.as_ref().is_some_and(|d| d.collecting_args)
    }

    /// Records an out-of-scope read performed while collecting a function's
    /// arguments. No-op when not currently defining.
    fn record_arg(&mut self, index: MemorizedIndex) {
        if let Some(defining) = self.defining.as_mut() {
            defining.args.push(index);
        }
    }

    /// Verifies the body of a `FunctionDefine`. The function (and any
    /// consecutive aliases) is registered globally by `common_function_logic`
    /// and its body is analyzed by [`analyze_definition_body`](Self::analyze_definition_body).
    fn verify_function_definition(&mut self, fun: &str) -> Result<(), VerifierError<Tag>> {
        self.ensure_not_nested(fun)?;

        // A registered downgrader must be defined with `Downgrader`, not
        // `FunctionDefine`.
        if self.policy.downgrader(fun).is_some() {
            return Err(VerifierError::Flow(FlowError::DowngraderUndefined {
                name: fun.to_owned(),
            }));
        }

        let (info, aliases) = self.analyze_definition_body(fun)?;
        self.publish_function_facts(fun, aliases, info);
        Ok(())
    }

    /// Verifies a `Downgrader` definition: analyzes the body with the
    /// connection `source` applied to its collected arguments, then performs
    /// the definition-time implicit retag — the return tag must match `source`
    /// and is recorded as `target`, so call sites publish the downgraded tag.
    fn verify_downgrader_definition(&mut self, fun: &str) -> Result<(), VerifierError<Tag>> {
        self.ensure_not_nested(fun)?;

        let Some(downgrader) = self.policy.downgrader(fun) else {
            return Err(VerifierError::Flow(FlowError::DowngraderUndefined {
                name: fun.to_owned(),
            }));
        };
        let connection = downgrader.connection;

        // Establish the downgrader context so the body's arguments carry the
        // connection `source`. No stack is needed: nested definitions are
        // forbidden, so at most one downgrader is under analysis at a time.
        let saved_downgrader = self.current_downgrader;
        self.current_downgrader = Some(connection);
        let analysis = self.analyze_definition_body(fun);
        self.current_downgrader = saved_downgrader;
        let (mut info, aliases) = analysis?;

        match info.return_tag {
            Some(tag) if tag == connection.source => info.return_tag = Some(connection.target),
            Some(tag) => {
                return Err(VerifierError::Flow(
                    FlowError::DowngraderReturnTagMismatch {
                        found: tag,
                        expected: connection.source,
                    },
                ));
            }
            None => {}
        }

        self.publish_function_facts(fun, aliases, info);
        Ok(())
    }

    /// Nested function definitions are intentionally unsupported: they clash
    /// with recursion (the function would be redefined on the second recursion
    /// step) and would force cloning `function_data` per definition.
    fn ensure_not_nested(&self, inner: &str) -> Result<(), VerifierError<Tag>> {
        if let Some(ref outer) = self.defining {
            return Err(NestedFunctionDefinition {
                outer_function: outer.name.clone(),
                inner_function: inner.to_owned(),
            });
        }
        Ok(())
    }

    /// Analyzes a definition body in a scoped stack, collecting the argument
    /// indices it reads from the caller's frame (see `ReadReverse` and
    /// `Rebase`), its return span, and its return tag. Partial facts are
    /// additionally published at the `Rebase` instruction so recursive calls
    /// inside the body resolve. Returns the collected facts and any aliases
    /// registered by consecutive `FunctionDefine`s.
    fn analyze_definition_body(
        &mut self,
        fun: &str,
    ) -> Result<(FunctionFacts<Tag>, Vec<String>), VerifierError<Tag>> {
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

        // Scope only the argument-collection state. `functions` (the
        // discovered facts) must persist so callers and recursive calls see it.
        let saved_defining = self.defining.replace(DefinitionInProgress {
            name: fun.to_owned(),
            args: Vec::new(),
            collecting_args: true,
        });

        let run_result = self.run_nested(to_check);

        let in_progress = self
            .defining
            .take()
            .expect("definition analysis state must still be present");
        self.defining = saved_defining;

        let (return_value, return_tag) = match &run_result {
            Ok((value, tag, _)) => (*value, *tag),
            Err(_) => (None, None),
        };

        run_result?;
        Ok((
            FunctionFacts {
                args: in_progress.args,
                return_value,
                return_tag,
            },
            aliases,
        ))
    }

    /// Publishes a function's discovered facts under its name and all aliases.
    fn publish_function_facts(
        &mut self,
        fun: &str,
        aliases: Vec<String>,
        facts: FunctionFacts<Tag>,
    ) {
        trace!("Finished verifying function definition for '{}'", fun);
        trace!(
            "Discovered argument indices for '{}': {:?}",
            fun, facts.args
        );

        for alias in aliases {
            self.functions.insert(alias, facts.clone());
        }
        self.functions.insert(fun.to_string(), facts);
    }

    /// Verifies an ordinary `FunctionCall` site and pushes the callee's
    /// recorded return span/tag. Rejects names registered as downgraders in
    /// the policy (those must use `Downgrade`).
    fn verify_function_call(&mut self, function_name: &str) -> Result<(), VerifierError<Tag>> {
        if self.policy.downgrader(function_name).is_some() {
            return Err(VerifierError::Flow(FlowError::DowngraderUndefined {
                name: function_name.to_owned(),
            }));
        }

        self.machine.function_get(function_name)?;

        let (return_value, return_tag) = self.callee_return(function_name)?;
        self.push_existing(return_value, return_tag);
        Ok(())
    }

    /// Verifies a `Downgrade` site and pushes the downgrader's recorded return
    /// span/tag (the tag already carries the connection `target`, recorded at
    /// definition time). Rejects names not registered as downgraders.
    fn verify_downgrader_call(&mut self, function_name: &str) -> Result<(), VerifierError<Tag>> {
        let Some(downgrader) = self.policy.downgrader(function_name) else {
            return Err(VerifierError::Flow(FlowError::DowngraderUndefined {
                name: function_name.to_owned(),
            }));
        };

        // Downgrades must happen at the top level of the program: a `Downgrade`
        // inside a function or downgrader body is rejected at definition time.
        if self.defining.is_some() {
            return Err(VerifierError::Flow(FlowError::NestedDowngraderCall {
                downgrader: function_name.to_owned(),
            }));
        }

        // Charge the call against the downgrader's total budget (mirrors the
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

        self.machine.function_get(function_name)?;

        let (return_value, return_tag) = self.callee_return(function_name)?;
        self.push_existing(return_value, return_tag);
        Ok(())
    }

    /// Looks up the callee's discovered facts, checks that every argument
    /// index it reads resolves to an existing caller cell, and returns its
    /// recorded return span/tag (defaults: unbounded span, default tag).
    fn callee_return(&self, function_name: &str) -> Result<(ValueSpan, Tag), VerifierError<Tag>> {
        use FunctionDataError::FunctionUndefined;

        let facts = self
            .functions
            .get(function_name)
            .ok_or_else(|| VerifierError::from(FunctionUndefined(function_name.to_owned())))?;

        trace!("Function facts for '{}': {:#?}", function_name, facts);

        // Every index the function reads as an argument must resolve to an
        // existing cell in the caller's current stack.
        let available = self.stack.len();
        let deepest_missing = facts
            .args
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

        let return_value = facts.return_value.unwrap_or_else(ValueSpan::inf);
        let return_tag = facts
            .return_tag
            .unwrap_or_else(|| self.policy.default_tag());
        Ok((return_value, return_tag))
    }

    pub fn read(&self, reg: CellIndex) -> Result<ValueSpan, VerifierError<Tag>> {
        self.stack.value_at(reg.into()).ok_or(InvalidCell {
            instr: self.machine.program_data.get_current()?.clone(),
            cell_index: reg,
        })
    }

    /// Like [`read`](Self::read), but while collecting a function's arguments
    /// an out-of-scope read is recorded as a `Normal` argument index and
    /// yields an unbounded span (with `default_tag`) instead of erroring.
    fn read_normal(&mut self, idx: CellIndex) -> Result<(ValueSpan, Tag), VerifierError<Tag>> {
        // A read outside the function's own loaded cells (`[base, len)`) reads
        // a caller-supplied argument rather than function-local data.
        let reads_argument = !(self.stack.base()..self.stack.len()).contains(&usize::from(idx));
        if self.is_collecting_args() && reads_argument {
            self.record_arg(MemorizedIndex::Normal(idx));
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
                // Crossing `Rebase` ends argument collection: freeze the
                // arguments discovered so far and publish partial facts so
                // recursive calls inside the body resolve.
                if let Some(defining) = self.defining.as_mut()
                    && defining.collecting_args
                {
                    defining.collecting_args = false;
                    let partial = FunctionFacts {
                        args: defining.args.clone(),
                        return_value: None,
                        return_tag: None,
                    };
                    let name = defining.name.clone();
                    self.functions.insert(name, partial);
                }
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
                if self.is_collecting_args() {
                    // While collecting a function's arguments, a `ReadReverse`
                    // reads a fresh caller argument only if it reaches *past*
                    // the values already loaded in this phase. Reaching into
                    // the already-loaded region just re-reads function-local
                    // data and is not an argument.
                    let loaded = self.stack.len().saturating_sub(self.stack.base());
                    if usize::from(arg) >= loaded {
                        self.record_arg(MemorizedIndex::Reverse(arg));
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
            FunctionDefine => self.verify_function_definition(name)?,
            Downgrader => self.verify_downgrader_definition(name)?,
            FunctionCall => self.verify_function_call(name)?,
            Downgrade => self.verify_downgrader_call(name)?,
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
        // `defining` and `functions` are scoped to the block; rolling back
        // `functions` is the same intended conservatism as in
        // `run_ifelse_branch`.
        let saved_defining = self.defining.clone();
        let saved_functions = self.functions.clone();
        let result = self.run_nested(instrs);
        self.defining = saved_defining;
        self.functions = saved_functions;

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
            // Downgrader budgets are also re-run per branch and merged with the
            // per-downgrader MAX: at runtime only one branch executes, so
            // exploring both must not double-charge.
            None => {
                let initial = self.stack.slots().to_vec();
                let initial_calls = self.downgrader_calls.clone();

                self.run_ifelse_branch(&when_true, condition_tag)?;
                let true_cells = self.stack.replace_slots(initial);
                let true_calls = std::mem::replace(&mut self.downgrader_calls, initial_calls);

                self.run_ifelse_branch(&when_false, condition_tag)?;
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
