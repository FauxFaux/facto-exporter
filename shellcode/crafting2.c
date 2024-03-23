#include <stdint.h>
#include <stddef.h>

struct Crafting {
  // ish
  uint32_t data[0x90];
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

struct Shared {
  // in
  struct Set *set;
  int (*getStatus)(struct Crafting *crafting);
  size_t capacity;

  // out
  size_t size;
  struct CraftingLite crafting[];
};

extern int entry(
  struct Shared *mem
);

static void walk(
  struct SetEntry *entry,
  struct Shared *mem
) {
  if (entry == NULL) {
    return;
  }

  walk(entry->left, mem);

  if (mem->size >= mem->capacity) {
    return;
  }

  struct Crafting *crafting = entry->data;
  struct CraftingLite *lite = &mem->crafting[mem->size];
  // untested
  lite->unit_number = crafting->data[0x26];
  lite->products_complete = crafting->data[0x81];
  lite->status = mem->getStatus(crafting);
  mem->size++;

  walk(entry->right, mem);
}

extern int entry(
  struct Shared *mem
) {
  mem->size = 0;
  walk(mem->set->begin, mem);
  return 0;
}

