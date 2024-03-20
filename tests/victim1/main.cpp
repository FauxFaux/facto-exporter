#define _DEFAULT_SOURCE

#include <string>
#include <set>

#include <unistd.h>

#include "obj.h"

int main() {
    int counter = 1;
    std::set<Obj> objs;

    objs.insert(Obj {});

    while (counter > 0) {
        counter += step(objs);
        usleep(400 * 1000);
    }

    return 0;
}
