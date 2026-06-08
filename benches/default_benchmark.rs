use criterion::{Criterion, criterion_group, criterion_main};
use virtual_machine::{
    add_instr,
    instruction::{
        BinaryOp::{self},
        Instruction::{self, AluBinary, AluUnaryImm},
        UnaryOpImm::{self},
    },
    machine::executor::Executor,
};

fn bench1(c: &mut Criterion) {
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
    let mut executor = Executor::new(program);

    c.bench_function("simple addition", |b| {
        b.iter(|| {
            let _ = executor.exec().expect("Failed to run the program");
        })
    });
}

criterion_group!(benches, bench1);
criterion_main!(benches);
