.text
.globl main
.type main, @function
main:
    pushq %rbp
    movq %rsp, %rbp
    movl $0, %esi
    jmp L1
L0:
    movl %esi, %ebx
    movl $1, %ecx
    movl %ebx, %esi
    addl %ecx, %esi
L1:
    movl %esi, %ebx
    movl $10, %ecx
    cmpl %ecx, %ebx
    jl L0
    movl %esi, %ebx
    movl $20, %ecx
    cmpl %ecx, %ebx
    je L2
    movl $0, %edi
    jmp L5
L4:
    movl %esi, %ebx
    movl $1, %ecx
    movl %ebx, %esi
    subl %ecx, %esi
    movl %edi, %ebx
    movl $1, %ecx
    movl %ebx, %edi
    addl %ecx, %edi
L5:
    movl %edi, %ebx
    movl $10, %ecx
    cmpl %ecx, %ebx
    jl L4
    movl %esi, %ebx
    movl $2, %ecx
    movl %ebx, %esi
    addl %ecx, %esi
    jmp L3
L2:
    movl $0, %eax
L3:
    movl %esi, %eax
    popq %rbp
    ret
.size main, .-main
