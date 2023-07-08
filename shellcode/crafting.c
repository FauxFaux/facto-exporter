#include <stdint.h>
#include <stddef.h>

//extern void *malloc(size_t size);
//extern int getStatus(void *crafting);

struct Crafting {
  char unknown[0x98];
  uint32_t unit_number;
  char unknown2[0x168];
  uint32_t products_complete;
};

struct SetEntry {
  void *unknown;
  void *unknown2;
  struct SetEntry *left;
  struct SetEntry *right;
  struct Crafting *data;
};

struct Set {
  void *unknown;
  void *parent;
  struct SetEntry *begin;
  void *end;
  void *unknown2;
  size_t size;
};

struct CraftingLite {
  uint32_t unit_number;
  uint32_t products_complete;
  uint32_t status;
};

static void dbg_break(uint64_t code) {
  // src, dest assembly
  __asm volatile (
    "mov %0, %%r10\n"
    "int3"
    :
    : "r" (code)
    : "r10"
 );
}

extern void entry(
  struct Set *set,
  void* (*malloc)(size_t size),
  void (*free)(void *ptr),
  int (*getStatus)(struct Crafting *crafting)
) {
  size_t size = set->size;
  struct CraftingLite *lites = malloc(size * sizeof(struct CraftingLite));
  if (!lites) dbg_break(2);
  size_t lites_off = 0;
  struct SetEntry **search = malloc(1000 * sizeof(struct SetEntry));
  if (!search) dbg_break(3);
  size_t search_off = 0;
  search[search_off++] = set->begin;
  while (search_off > 0) {
    const struct SetEntry *entry = search[--search_off];
    if (entry->left) {
      search[search_off++] = entry->left;
    }
    if (entry->right) {
      search[search_off++] = entry->right;
    }
    struct Crafting *crafting = entry->data;
    struct CraftingLite lite = {
      .unit_number = crafting->unit_number,
      .products_complete = crafting->products_complete,
      .status = getStatus(crafting),
    };
    lites[lites_off++] = lite;
  }

  free(search);

  // make variables available in named (arbitrary) registers
  // and then trigger the breakpoint

  // src, dest assembly
  __asm volatile (
    "mov %0, %%r10\n"
    "mov %1, %%r11\n"
    "int3"
    :
    : "r" (lites),
      "r" (lites_off)
    : "r10", "r11"
   );
  free(lites);

  __asm volatile ("int3");
}
