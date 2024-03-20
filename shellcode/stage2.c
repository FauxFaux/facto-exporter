struct Wumpus {
    void *a;
    void *b;
    void *c;
};

extern void baz(struct Wumpus *);

extern void bar(struct Wumpus *w) {
    baz(w);
}
