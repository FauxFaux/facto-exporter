bins: crafting.bin stage1.bin crafting2.bin call-end.bin mock-get-status.bin

%.o: %.c
	clang -march=x86-64-v3 -Wall -Wextra -fPIC -O1 -c -std=c2x $<

%.bin: %.o
	objcopy -O binary -j .text $< $@
	objdump -t $< | grep '^0' > $@.addr

stage1.nasm: stage1-gen
	./stage1-gen > stage1.nasm

stage1.bin: stage1.nasm
	nasm stage1.nasm -o stage1.bin

call-end.bin: call-end.nasm
	nasm call-end.nasm -o call-end.bin

clean:
	rm -f *.o *.bin *.addr stage1-gen stage1.nasm
