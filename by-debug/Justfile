shell: crafting

crafting:
  clang -march=x86-64-v3 -Wall -Wextra -fPIC -O1 -c -std=c2x shellcode/crafting.c
  objcopy -O binary -j .text crafting.o shellcode/crafting.bin
  rm crafting.o
