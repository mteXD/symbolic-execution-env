# TODO

Here are all the TODOs and ideas.

- [ ] User I/O
    - [x] Standard I/O
    - [x] Buffer I/O (mostly for internal testing)
    - [ ] File I/O
        - [ ] File read
        - [ ] File write
- [!] Infinite recursion check
- [ ] Stack overflow check
    - [ ] Add a minimum and maximum stack size
    - [ ] Simulate the program and check if the stack size exceeds the maximum
- [x] Check for functions that return void
- [ ] Heap & pointers
- [ ] Named arguments, equivalent to normal cell positions
- [ ] Get rid of machine duplication

    
Things that can go wrong:
- Function not defined
- Stack underflow
- Infinite recursion (something connected to the Cond instruction)
- Integer overflow
- Not enough arguments after rebase
- Make a data structure that will hold, for each function:
  - is it recursive
  - if so, is it finite
    - FunctionCall must be inside a conditional block
    - Cond instruction must immediately follow some comparison executed on a critical value
      that decreases in each recursion
  - if so, what is permissible data input to make it final, using a predicate system,
    something like:
    - critical value (e.g. "3", "0", "1"...)
    - qualifier (e.g. "Greater Than", "Equal", "Greater or equal"...)
    - predicates can be combined, maybe? (e.g. {0, "Greater Than"} OR {0, "Equal"}, which
      would result in >= 0)


Other things to do:
- think about implementing pseudo-instructions (e.g. CondBlock that expands into a Block
  preceeded by a Cond)
- support for chars and strings
  - arithmetic (and others) not allowed, or maybe specific arithmetic instructions
- support for arbitrary input:
  - user input, file input?
  - in this case, if statements need to be verified in a fork-and-join manner. Possible need
    for implementing Kildall's algorithm.
- better error messages
- some src/main.rs that runs some predefined programs
- Maximum stack size for verification (and maybe execution)
- Maximum recursion depth for verification (and maybe execution)

Some differences with other systems:
- SSA is already built-in (unavoidable for the programmer), except for the fact that cells
  can be dropped with pop()
- Jumps are only in form of function calls, and so it is impossible to jump to invalid code
