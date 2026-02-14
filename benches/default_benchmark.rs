use criterion::{Criterion, criterion_group, criterion_main};
use virtual_machine::{
    Instruction::{self, AluUnaryImm, AluBinary},
    UnaryOpImm::{
        self
    },
    BinaryOp::{
        self
    },
    macros::add_instr,
};

fn bench1(c: &mut Criterion) {
    let mut machine = virtual_machine::Machine::new();
    let program: Vec<Instruction> = std::hint::black_box(
        vec![
            (0..10000)
                .map(|i| add_instr!(Push, i))
                .collect::<Vec<Instruction>>(),
            (0..9999)
                .zip(1..10000)
                .map(|(i, j)| add_instr!(Add, i, j))
                .collect::<Vec<Instruction>>(),
        ]
        .iter()
        .flatten()
        .cloned()
        .collect(),
    );
    machine.load_program(&program);

    c.bench_function("simple addition", |b| {
        b.iter(|| {
            let _ = machine.run().expect("Failed to run the program");
        })
    });
}

criterion_group!(benches, bench1);
criterion_main!(benches);
