[group('building')]
build:
    @echo "Building the project..."
    cargo build
    @echo "Build completed successfully."

[group('building')]
release: clean
    @echo "Building the project in release mode..."
    cargo build --release
    @echo "Release build completed successfully."

[group('dev')]
bench: check
    cargo bench

flamegraph test:
    cargo flamegraph --dev --unit-test virtual_machine -- tests::{{test}}

[group('dev')]
test $RUST_BACKTRACE="1":
    cargo test --

[group('dev')]
test-list:
    cargo test -- --list
    
[group('dev')]
check:
    cargo check

[group('cleaning')]
clean:
    cargo clean
    rm flamegraph.svg
    rm perf.data
    rm perf.data.old
