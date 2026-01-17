// make stage1-gen && ./stage1-gen > stage1.nasm && nasm stage1.nasm -o stage1.bin

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <sys/mman.h>

// /usr/src/linux-headers-6.5.0-25-generic/arch/x86/include/generated/uapi/asm/unistd_64.h
#define __NR_mmap 9

int main() {
    printf("BITS 64\n");
    // nop slide because something something rip isn't aligned right, 11 because it makes the file mod 8 long
    for (int i = 0; i < 11; ++i) {
        printf("nop\n");
    }
    // https://en.wikibooks.org/wiki/X86_Assembly/Interfacing_with_Linux#Via_dedicated_system_call_invocation_instruction
    printf("mov rax, %d\n", __NR_mmap);
    printf("xor rdi, rdi\n"); // address hint
    printf("mov rsi, %d\n", 100 * 640 * 1024); // should be enough for anyone
    printf("mov rdx, %d\n", PROT_EXEC | PROT_READ | PROT_WRITE);
    printf("mov r10, %d\n", MAP_PRIVATE | MAP_ANONYMOUS);
    printf("mov r8, %d\n", -1); // no fd
    printf("xor r9, r9\n"); // no offset
    printf("syscall\n");
    printf("int3\n");
}
