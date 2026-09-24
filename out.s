.text
.globl sum_seven
.type sum_seven, @function
sum_seven:
    pushq %rbp
    movq %rsp, %rbp
    movl 16(%rbp), %eax
    addl %esi, %edi
    movl %edi, %r11d
    addl %edx, %r11d
    movl %r11d, %edx
    movl %edx, %r11d
    addl %ecx, %r11d
    movl %r11d, %ecx
    addl %r8d, %ecx
    addl %r9d, %ecx
    movl %ecx, %r11d
    addl %eax, %r11d
    movl %r11d, %eax
    jmp L0
L0:
    popq %rbp
    ret
.size sum_seven, .-sum_seven

.globl main
.type main, @function
main:
    pushq %rbp
    movq %rsp, %rbp
    subq $16, %rsp
    movl $1, %edi
    movl $2, %esi
    movl $3, %edx
    movl $4, %ecx
    movl $5, %r8d
    movl $6, %r9d
    movl $7, (%rsp)
    call sum_seven
    jmp L1
L1:
    addq $16, %rsp
    popq %rbp
    ret
.size main, .-main

.section .note.GNU-stack,"",@progbits
