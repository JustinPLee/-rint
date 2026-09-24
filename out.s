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
    jmp L2
L1:
    subl $1, %esi
    movl %ebx, %edi
    call pow
    imull %ebx, %eax
    jmp L2
L2:
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
    movl $1, %r11d
    movl $1, %eax
    cltd
    idivl %r11d
    movl %eax, %ecx
    movl %ecx, %eax
    movl $2, %edi
    movl $2, %esi
    call pow
    movl %eax, %esi
    movl $2, %edi
    call pow
    jmp L3
L3:
    popq %rbp
    ret
.size main, .-main

.section .note.GNU-stack,"",@progbits
