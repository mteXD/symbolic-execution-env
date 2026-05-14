use std::{
    collections::HashMap,
    fmt::{Debug, Display, Formatter},
    ops::{Add, AddAssign, Sub},
    rc::Rc,
    cell::RefCell,
    io::{self, Read, Write},
};

use log::{error};

use crate::instruction::Instruction;

pub type CellIndex = u16;
pub type Immediate = i64;

// PERF: Copy for Strings can be expensive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cell {
    Integer(Immediate),
    Text(char),
}

impl PartialEq<Immediate> for Cell {
    fn eq(&self, other: &Immediate) -> bool {
        use Cell::*;

        match self {
            Integer(i) => i == other,
            Text(_) => false,
        }
    }
}

impl Display for Cell {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use Cell::*;

        match self {
            Integer(i) => write!(f, "{}", i),
            Text(s) => write!(f, "{}", s),
        }
    }
}

#[derive(Debug, Clone, Default, Copy)]
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
            },
            Address::Value(v) => Ok(v),
        }
    }
}

#[derive(Debug, Clone)]
pub enum FunctionDataError {
    FunctionRedefinition(String),
    FunctionUndefined(String),
}
use FunctionDataError::*;

#[derive(Debug, Clone, Default)]
pub struct FunctionData {
    function_table: HashMap<String, Instruction>,
}

impl FunctionData {
    pub fn new() -> Self {
        FunctionData::default()
    }

    pub fn insert(&mut self, name: String, instr: Instruction) -> Result<(), FunctionDataError> {
        if self.function_table.contains_key(&name) {
            return Err(FunctionRedefinition(name));
        }

        self.function_table.insert(name, instr);

        Ok(())
    }

    pub fn get(&self, name: &str) -> Result<&Instruction, FunctionDataError> {
        self.function_table
            .get(name)
            .ok_or(FunctionUndefined(name.to_owned()))
    }

    pub fn contains_key(&self, name: &str) -> bool {
        self.function_table.contains_key(name)
    }
}

#[derive(Debug, Clone)]
pub enum ProgramDataError {
    InvalidPC {
        pc: Address,
    },
}
use ProgramDataError::*;

#[derive(Debug, Clone, Default)]
pub struct ProgramData<'a> {
    program: &'a [Instruction],
    pc: Address,
}

impl<'a> ProgramData<'a> {
    pub fn new(program: &'a [Instruction]) -> Self {
        Self { program, pc: Address::Null }
    }

    pub fn reset(&mut self) {
        self.pc = Address::Null;
    }

    pub fn get_pc(&self) -> Address {
        self.pc
    }

    pub fn get_at(&self, pc: Address) -> Result<&Instruction, ProgramDataError> {
        self.program.get::<usize>(pc.try_into()?).ok_or(InvalidPC { pc })
    }

    pub fn get_current(&self) -> Result<&Instruction, ProgramDataError> {
        self.get_at(self.pc)
    }
}

impl<'a> Iterator for ProgramData<'a> {
    type Item = &'a Instruction;

    fn next(&mut self) -> Option<Self::Item> {
        self.pc.inc();
        let instr = self.program.get::<usize>(self.pc.try_into().ok()?)?;
        Some(instr)
    }
}

#[derive(Debug, Clone)]
pub enum Output {
    Stdout,
    File(String),
    Buffer(Rc<RefCell<Vec<u8>>>),
}

#[derive(Debug, Clone)]
pub enum Input {
    Stdin,
    File(String),
    Buffer(Rc<RefCell<Vec<u8>>>),
}

impl Output {
    pub fn write(&mut self, data: &[u8]) {
        match self {
            Output::Stdout => {
                let mut out = io::stdout();
                let _ = out.write_all(data);
            }
            Output::File(path) => {
                use std::fs::OpenOptions;

                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .unwrap();

                let _ = file.write_all(data);
            }
            Output::Buffer(buf) => {
                let mut buf = buf.borrow_mut();
                buf.extend_from_slice(data);
            }
        }
    }
}

impl Input {
    pub fn read_all(&mut self) -> Vec<u8> {
        match self {
            Input::Stdin => {
                let mut buf = Vec::new();
                io::stdin().read_to_end(&mut buf).unwrap();
                buf
            }
            Input::File(path) => std::fs::read(path).unwrap(),
            Input::Buffer(data) => {
                let mut buf = Vec::new();
                data.borrow().iter().for_each(|byte| buf.push(*byte));
                buf
            }
        }
    }
}
