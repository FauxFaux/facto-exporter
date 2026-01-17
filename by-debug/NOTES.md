### 2023

 * find `giveProducts` (unused)
 * find `craftingInsert`
 * find `gameUpdateStateStep`
 * find `main`
 * find `craftingStatus` (called from shellcode)
 * find `malloc` and `free`
 * on `game update` thread
 * stop
 * overwrite `main` (the crash handler) with the shellcode
 * breakpoint `craftingInsert` and `gameUpdateStep`
 * wait for a breakpoint
 * if it was the `craftingInsert` (assembler place) breakpoint,
   * store the pointer to the list of assemblers
 * back up registers
 * debugger jump to the shellcode, at `main`
 * continue
 * wait for break
 * get registers
 * bulk read from the specified pointer
 * continue (to allow cleanup), then it breaks again
 * restore registers

### 2024 v1

 * find some function (possibly `main`)
 * overwrite it with a call to mmap ("stage1")
 * call that from the debugger, and capture the region
 * use the debugger to restore state

 * write the shellcode(s) into the new region

 * patch the start of `step` and `place` to call into the new region
 * new shellcode in that region updates an argument with its values
 * int3 back into the debugger?
 * read the values out of the struct
 * resume

---

 * all completely untested
 * doesn't handle detach, which supposedly works with the old one
 * we could ptrace-breakpoint the place the int3 would be if it was there, but not actually have it there
