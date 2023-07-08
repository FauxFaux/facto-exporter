#include <stdint.h>
#include <stddef.h>

//extern void *malloc(size_t size);
//extern int getStatus(void *crafting);

struct Crafting {
  char unknown[0x9c];
  uint32_t unit_number;
  char unknown2[0x164];
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
  void *unknown2;
  void *unknown3;
  struct SetEntry *begin;
  size_t size;
};

struct CraftingLite {
  uint32_t unit_number;
  uint32_t products_complete;
  uint32_t status;
};

extern void entry(
  struct Set *set,
  void* (*malloc)(size_t size),
  void (*free)(void *ptr),
  int (*getStatus)(struct Crafting *crafting)
) {
  size_t size = set->size;
  struct CraftingLite *lites = malloc(size * sizeof(struct CraftingLite));
  size_t lites_off = 0;
  struct SetEntry **search = malloc(1000 * sizeof(struct SetEntry));
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
  __asm volatile (
    "mov %%r10, %0\n"
    "mov %%r11, %1\n"
    "int3"
    :
    : "r" (lites),
      "r" (lites_off)
    : "r10", "r11"
   );
  free(lites);
}
