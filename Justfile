shell: crafting

crafting:
  clang -Wall -Wextra -fPIC -O1 -c -std=c2x shellcode/crafting.c
  objcopy -O binary -j .text crafting.o shellcode/crafting.bin
  rm crafting.o
