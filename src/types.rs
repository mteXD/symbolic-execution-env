use std::{
    cell::RefCell,
    collections::HashMap,
    fmt::{Debug, Display, Formatter},
    io::{self, Read, Write},
    rc::Rc,
};

use log::error;

use crate::instruction::Instruction;

pub type CellIndex = u16;
/// Number of cells operated on by an instruction.
pub type CellAmount = u16;
pub type Immediate = i64;

/// The integer value stored in a concrete executor cell.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq)]
pub enum Value {
    Integer(Immediate),
}

impl PartialEq<Immediate> for Value {
    fn eq(&self, other: &Immediate) -> bool {
        use Value::*;

        match self {
            Integer(i) => i == other,
        }
    }
}

impl Value {
    pub fn into_immediate(self) -> Immediate {
        let Value::Integer(immediate) = self;
        immediate
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use Value::*;

        match self {
            Integer(i) => write!(f, "{}", i),
        }
    }
}

#[derive(Debug, Clone, Default, Copy, PartialEq, Eq)]
pub enum Address {
    #[default]
    Null,
    Value(usize),
}

impl Address {
    pub fn inc(&mut self) {
        use Address::*;

        *self = match self {
            Null => Value(0),
            Value(v) => Value(*v + 1),
        };
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use Address::*;

        match self {
            Null => write!(f, "Null"),
            Value(v) => write!(f, "@{}", v),
        }
    }
}

impl TryInto<usize> for Address {
    type Error = ProgramDataError;

    fn try_into(self) -> Result<usize, Self::Error> {
        match self {
            Address::Null => {
                error!("Address struct Null to usize");
                Err(InvalidPC { pc: self })
            }
            Address::Value(v) => Ok(v),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionDataError {
    FunctionRedefinition(String),
    FunctionUndefined(String),
    FunctionMissingBody(String),
}
use FunctionDataError::*;

#[derive(Debug, Clone)]
pub struct FunctionData<Tag = ()> {
    function_table: HashMap<String, Instruction<Tag>>,
}

impl<Tag> Default for FunctionData<Tag> {
    fn default() -> Self {
        Self {
            function_table: HashMap::new(),
        }
    }
}

impl<Tag> FunctionData<Tag> {
    pub fn insert(
        &mut self,
        name: String,
        instruction: Instruction<Tag>,
    ) -> Result<(), FunctionDataError> {
        if self.function_table.contains_key(&name) {
            return Err(FunctionRedefinition(name));
        }

        self.function_table.insert(name, instruction);

        Ok(())
    }

    fn entry(&self, name: &str) -> Result<&Instruction<Tag>, FunctionDataError> {
        self.function_table
            .get(name)
            .ok_or(FunctionUndefined(name.to_owned()))
    }

    pub fn get(&self, name: &str) -> Result<&Instruction<Tag>, FunctionDataError> {
        self.entry(name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgramDataError {
    InvalidPC { pc: Address },
}
use ProgramDataError::*;

#[derive(Debug, Clone)]
pub struct ProgramData<Tag = ()> {
    program: Rc<[Instruction<Tag>]>,
    pc: Address,
}

impl<Tag> ProgramData<Tag> {
    pub fn new(program: impl Into<Rc<[Instruction<Tag>]>>) -> Self {
        Self {
            program: program.into(),
            pc: Address::Null,
        }
    }

    pub fn get_at(&self, pc: Address) -> Result<&Instruction<Tag>, ProgramDataError> {
        let index: usize = pc.try_into()?;
        self.program.get(index).ok_or(InvalidPC { pc })
    }

    pub fn get_current(&self) -> Result<&Instruction<Tag>, ProgramDataError> {
        self.get_at(self.pc)
    }
}

impl<Tag: Clone> Iterator for ProgramData<Tag> {
    type Item = Instruction<Tag>;

    fn next(&mut self) -> Option<Self::Item> {
        self.pc.inc();
        let index: usize = self.pc.try_into().ok()?;
        self.program.get(index).cloned()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IoBuffer {
    buffer: Rc<RefCell<Vec<Immediate>>>,
}

impl IoBuffer {
    pub fn new(list: impl IntoIterator<Item = Immediate>) -> Self {
        Self {
            buffer: Rc::new(RefCell::new(list.into_iter().collect())),
        }
    }

    pub fn borrow_mut(&self) -> std::cell::RefMut<'_, Vec<Immediate>> {
        self.buffer.borrow_mut()
    }

    pub fn borrow(&self) -> std::cell::Ref<'_, Vec<Immediate>> {
        self.buffer.borrow()
    }
}

#[derive(Debug, Clone)]
pub enum Output {
    Stdout,
    Buffer(IoBuffer),
}

#[derive(Debug, Clone)]
pub enum Input {
    Stdin,
    Buffer(IoBuffer),
}

impl Output {
    pub fn write(&mut self, data: &[Immediate]) {
        let bytes = data.iter().map(|byte| *byte as u8).collect::<Vec<u8>>();

        match self {
            Output::Stdout => {
                let mut out = io::stdout();
                let _ = out.write_all(bytes.as_slice());
            }
            Output::Buffer(buf) => {
                let mut buf = buf.borrow_mut();
                buf.extend_from_slice(data);
            }
        }
    }
}

impl Input {
    pub fn read_all(&mut self) -> Vec<Immediate> {
        match self {
            Input::Stdin => {
                let mut buf: Vec<u8> = Vec::new();
                io::stdin().read_to_end(&mut buf).unwrap();

                buf.iter().map(|byte| *byte as Immediate).collect()
            }
            Input::Buffer(data) => {
                let mut buf = Vec::new();
                data.borrow().iter().for_each(|byte| buf.push(*byte));
                buf
            }
        }
    }
}

impl From<IoBuffer> for Input {
    fn from(buf: IoBuffer) -> Self {
        Input::Buffer(buf)
    }
}

impl From<IoBuffer> for Output {
    fn from(buf: IoBuffer) -> Self {
        Output::Buffer(buf)
    }
}
