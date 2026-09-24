## Installation

### Prerequisites

- Rust and Cargo
- GCC
- [`just`](https://github.com/casey/just)

### OR Setup with [Nix](https://nix.dev/install-nix.html)

```sh
nix develop
```

## Build and run

To run the program in `src/lang.lang`:

```sh
just
```

The output is emitted to the terminal. Additional information is written to `dump.txt`

To run another program:

```sh
cargo run -- <FILEPATH>
```

## Test

```sh
just t
```

Note: just running `cargo test` has a race condition, leading to incorrectly failed tests.

## Sample contents of `dump.txt`:

```text
# Source program:
typedef int A;
A pow(int b, int e) {
  if (e == 0) {
    return 1;
  } else {
    return b * pow(b, e - 1);
  }
}

int main() {
  // computes 1*1 + 2*2 +3*3
  int sum = 0;
  for (int i = 1; i <= 3; i++) {
    sum += pow(i, 2);
  }

  assert(1*1 + 2*2 + 3*3 == 14);
  assert(sum == 14);
  return sum;
}

# Lexed Tokens:
[Typedef, Int, Ident("A"), Semicolon, Ident("A"), Ident("pow"), LParen, Int, Ident("b"), Comma, Int, Ident("e"), RParen, LBrace, If, LParen, Ident("e"), EqualEq, Num(0), RParen, LBrace, Return, Num(1), Semicolon, RBrace, Else, LBrace, Return, Ident("b"), Times, Ident("pow"), LParen, Ident("b"), Comma, Ident("e"), Minus, Num(1), RParen, Semicolon, RBrace, RBrace, Int, Ident("main"), LParen, RParen, LBrace, Int, Ident("sum"), Eq, Num(0), Semicolon, For, LParen, Int, Ident("i"), Eq, Num(1), Semicolon, Ident("i"), LessEq, Num(3), Semicolon, Ident("i"), DoublePlus, RParen, LBrace, Ident("sum"), PlusEq, Ident("pow"), LParen, Ident("i"), Comma, Num(2), RParen, Semicolon, RBrace, Assert, LParen, Num(1), Times, Num(1), Plus, Num(2), Times, Num(2), Plus, Num(3), Times, Num(3), EqualEq, Num(14), RParen, Semicolon, Assert, LParen, Ident("sum"), EqualEq, Num(14), RParen, Semicolon, Return, Ident("sum"), Semicolon, RBrace, Eof]

# Parse AST:
(program
  (typedef int A)
  (fun_def A pow [int b, int e]
    (block
      (if (== e 0)
        (then (block
          (return 1)
        ))
        (else (block
          (return (* b (call pow b (- e 1))))
        )))
    ))
  (fun_def int main []
    (block
      (declare sum int 0)
      (for (declare i int 1) (<= i 3) (++ i)
        (block
            (+= sum (call pow i 2))
          )
      assert (== (+ (+ (* 1 1) (* 2 2)) (* 3 3)) 14)
      assert (== sum 14)
      (return sum)
    ))
)

# AST:
(program
  (typedef int A)
  (fun_def A pow [int b, int e]
    (block
      (if (== e 0)
        (then (block
          (return 1)
        ))
        (else (block
          (return (* b (call pow b (- e 1))))
        )))
    ))
  (fun_def int main []
    (block
      (declare sum int
        (block
          (assign sum 0)
          (declare i int
            (block
              (assign i 1)
              (for (; (<= i 3); (assign i (+ i 1)))
                (block
                    (assign sum (+ sum (call pow i 2)))
                  )
            ))
          (assert (== (+ (+ (* 1 1) (* 2 2)) (* 3 3)) 14))
          (assert (== sum 14))
          (return sum)
        ))
    ))
)

# (IR) linear:
fn pow(t0, t1):
    if t1 == 0 jump L0 else jump L1
L0:
    return 1
L1:
    t2 ← t1 sub 1
    t3 ← call pow(t0, t2)
    t4 ← t0 mul t3
    return t4

fn main():
    t5 ← 0
    t6 ← 1
    jump L3
L3:
    if t6 <= 3 jump L2 else jump L5
L2:
    t7 ← call pow(t6, 2)
    t8 ← t5 add t7
    t5 ← t8
    jump L4
L4:
    t9 ← t6 add 1
    t6 ← t9
    jump L3
L5:
    t10 ← 1 mul 1
    t11 ← 2 mul 2
    t12 ← t10 add t11
    t13 ← 3 mul 3
    t14 ← t12 add t13
    if t14 == 14 jump L6 else jump L7
L6:
    jump L8
L7:
    abort
L8:
    if t5 == 14 jump L9 else jump L10
L9:
    jump L11
L10:
    abort
L11:
    return t5


# (IR) function with calling convention:
fn pow(t0, t1):
    t0 ← arg1
    t1 ← arg2
    if t1 == 0 jump L0 else jump L1
L0:
    res0 ← 1
    return
L1:
    t2 ← t1 sub 1
    arg1 ← t0
    arg2 ← t2
    call pow
    t3 ← res0
    t4 ← t0 mul t3
    res0 ← t4
    return

fn main():
    t5 ← 0
    t6 ← 1
    jump L3
L3:
    if t6 <= 3 jump L2 else jump L5
L2:
    arg1 ← t6
    arg2 ← 2
    call pow
    t7 ← res0
    t8 ← t5 add t7
    t5 ← t8
    jump L4
L4:
    t9 ← t6 add 1
    t6 ← t9
    jump L3
L5:
    t10 ← 1 mul 1
    t11 ← 2 mul 2
    t12 ← t10 add t11
    t13 ← 3 mul 3
    t14 ← t12 add t13
    if t14 == 14 jump L6 else jump L7
L6:
    jump L8
L7:
    abort
L8:
    if t5 == 14 jump L9 else jump L10
L9:
    jump L11
L10:
    abort
L11:
    res0 ← t5
    return

# Emitted assembly:
.text
.globl pow
.type pow, @function
pow:
    pushq %rbp
    movq %rsp, %rbp
    pushq %rbx
    subq $8, %rsp
    movl %edi, %ebx
    cmpl $0, %esi
    jne L1
L0:
    movl $1, %eax
    jmp L12
L1:
    subl $1, %esi
    movl %ebx, %edi
    call pow
    imull %ebx, %eax
    jmp L12
L12:
    addq $8, %rsp
    popq %rbx
    popq %rbp
    ret
.size pow, .-pow

.globl main
.type main, @function
main:
    pushq %rbp
    movq %rsp, %rbp
    pushq %rbx
    pushq %r12
    movl $0, %r12d
    movl $1, %ebx
    jmp L3
L3:
    cmpl $3, %ebx
    jg L5
L2:
    movl %ebx, %edi
    movl $2, %esi
    call pow
    movl %r12d, %r11d
    addl %eax, %r11d
    movl %r11d, %eax
    movl %eax, %r12d
    jmp L4
L4:
    movl %ebx, %eax
    addl $1, %eax
    movl %eax, %ebx
    jmp L3
L5:
    movl $1, %ecx
    imull $1, %ecx
    movl $2, %eax
    imull $2, %eax
    addl %eax, %ecx
    movl $3, %eax
    imull $3, %eax
    movl %ecx, %r11d
    addl %eax, %r11d
    movl %r11d, %eax
    cmpl $14, %eax
    jne L7
L6:
    jmp L8
L7:
    call abort
L8:
    cmpl $14, %r12d
    jne L10
L9:
    jmp L11
L10:
    call abort
L11:
    movl %r12d, %eax
    jmp L13
L13:
    popq %r12
    popq %rbx
    popq %rbp
    ret
.size main, .-main

.section .note.GNU-stack,"",@progbits

# Result:
14
