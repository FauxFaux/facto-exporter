#include <set>

struct Obj {
    int a;
    int b;
    int c;
    int d;
    int e;
    void *f;
    int g;
    float h;
    void *i;
    int j;
    int k;
    
    bool operator<(const Obj& other) const;
};

extern int step(std::set<Obj> &);
