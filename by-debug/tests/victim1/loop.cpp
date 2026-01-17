#include <iostream>
#include <string>

#include "obj.h"

int step(std::set<Obj> &s) {
    std::cout << "hello " << s.size() << std::endl;
    return s.size();
}

bool Obj::operator<(const Obj& other) const {
    return this->a < other.a;
}
